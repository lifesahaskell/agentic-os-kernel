//! Lazy copy-on-write address-space clone, callable on another space's behalf (the procmgr's
//! `fork`), and the write-fault handler that completes it.

use core::ops::Range;

use ostd::{
    mm::{
        FrameAllocOptions, PAGE_SIZE, PageFlags, UFrame, Vaddr, VmSpace,
        io::util::HasVmReaderWriter, tlb::TlbFlushOp, vm_space::VmQueriedItem,
    },
    task::disable_preempt,
};

use crate::agent::{Agent, AgentError};

/// Software bit marking a page that is read-only only because it is shared copy-on-write.
const COW: PageFlags = PageFlags::AVAIL2;

/// Clones the mappings of `parent` in `range` into a new space. Writable pages become read-only
/// and copy-on-write in both spaces; nothing is copied until a write faults.
// shortcut: 4 KiB pages only (asserted); huge pages need splitting before protect.
pub fn cow_clone(parent: &VmSpace, range: Range<Vaddr>) -> VmSpace {
    let child = VmSpace::new();
    let guard = disable_preempt();
    let mut pc = parent.cursor_mut(&guard, &range).unwrap();
    let mut cc = child.cursor_mut(&guard, &range).unwrap();

    while let Some(va) = pc.find_next(range.end - pc.virt_addr()) {
        let (r, item) = pc.query().unwrap();
        let Some(VmQueriedItem::MappedRam { frame, mut prop }) = item else {
            panic!("cow_clone: MMIO in a user range at {va:#x}");
        };
        assert_eq!(r.len(), PAGE_SIZE, "cow_clone: huge page at {va:#x}");
        let frame: UFrame = (*frame).clone();
        if prop.flags.contains(PageFlags::W) {
            prop.flags = (prop.flags - PageFlags::W) | COW;
            pc.protect_next(PAGE_SIZE, |f, _| *f = (*f - PageFlags::W) | COW);
        } else {
            pc.jump(va + PAGE_SIZE).unwrap();
        }
        cc.jump(va).unwrap();
        cc.map(frame, prop);
        if pc.virt_addr() >= range.end {
            break;
        }
    }

    pc.flusher().issue_tlb_flush(TlbFlushOp::for_range(range));
    pc.flusher().dispatch_tlb_flush();
    drop((pc, cc));
    child
}

/// Resolves a write fault at `addr` if it hit a copy-on-write page. Returns `Ok(false)` for any
/// other fault (a real SIGSEGV). The copy is charged to `agent`.
pub fn handle_cow_fault(space: &VmSpace, agent: &Agent, addr: Vaddr) -> Result<bool, AgentError> {
    let page = addr & !(PAGE_SIZE - 1);
    let guard = disable_preempt();
    let mut c = space.cursor_mut(&guard, &(page..page + PAGE_SIZE)).unwrap();
    let (mut prop, old) = match c.query().unwrap().1 {
        Some(VmQueriedItem::MappedRam { frame, prop }) if prop.flags.contains(COW) => {
            // Read before cloning: the count includes page-table mappings, so 1 means this
            // mapping is the last one and the frame can be taken over instead of copied.
            let sole = frame.reference_count() == 1;
            (prop, (!sole).then(|| -> UFrame { (*frame).clone() }))
        }
        _ => return Ok(false),
    };
    prop.flags = (prop.flags - COW) | PageFlags::W;

    let Some(old) = old else {
        c.protect_next(PAGE_SIZE, |f, _| *f = (*f - COW) | PageFlags::W);
        c.flusher().issue_tlb_flush(TlbFlushOp::for_single(page));
        c.flusher().dispatch_tlb_flush();
        return Ok(true);
    };

    agent.charge_pages(1)?;
    let new = FrameAllocOptions::new().alloc_frame().unwrap();
    new.writer().write(&mut old.reader());
    c.unmap(PAGE_SIZE);
    c.jump(page).unwrap();
    c.map(new.into(), prop);
    Ok(true)
}
