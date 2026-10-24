//! Lazy copy-on-write address-space clone, callable on another space's behalf (the procmgr's
//! `fork`), and the write-fault handler that completes it.

use core::ops::Range;

use ostd::{
    arch::cpu::context::CpuException,
    mm::{
        FallibleVmRead, FallibleVmWrite, FrameAllocOptions, PAGE_SIZE, PageFlags, UFrame, Vaddr,
        VmReader, VmSpace, VmWriter, fault::inject_user_page_fault_handler,
        io::util::HasVmReaderWriter, tlb::TlbFlushOp, vm_space::VmQueriedItem,
    },
    task::disable_preempt,
};

use crate::{
    agent::{Agent, AgentError},
    syscall::{Errno, UserSlice},
};

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
        // The source cursor cannot jump to `range.end`, so the last page ends the walk.
        let last = va + PAGE_SIZE >= range.end;
        if prop.flags.contains(PageFlags::W) {
            prop.flags = (prop.flags - PageFlags::W) | COW;
            pc.protect_next(PAGE_SIZE, |f, _| *f = (*f - PageFlags::W) | COW);
        } else if !last {
            pc.jump(va + PAGE_SIZE).unwrap();
        }
        cc.jump(va).unwrap();
        cc.map(frame, prop);
        if last || pc.virt_addr() >= range.end {
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

/// Page faults one copy may resolve on a single page before it gives up. One copy-on-write break
/// is the normal case; the second absorbs a racing `cow_clone` re-sharing the page.
const MAX_FAULTS_PER_PAGE: usize = 2;

/// OSTD panics on a kernel-mode fault at a user address unless a handler is registered. This one
/// declines every fault: the copy primitives below resolve copy-on-write themselves, because
/// the fault carries no agent to charge. The registration is idempotent, so no init order.
fn decline_kernel_user_faults() {
    inject_user_page_fault_handler(|_: &CpuException| Err(()));
}

/// Copies `src` into user memory at `dst` (the active `space`). Writing a copy-on-write page
/// breaks it on `agent`'s budget and retries, so the kernel never writes into a frame the
/// parent still shares. `EFAULT`: unmapped, not writable, or the fault would not resolve.
/// `ENOMEM`: the budget refused the copy.
pub fn copy_to_user(
    space: &VmSpace,
    agent: &Agent,
    dst: UserSlice,
    src: &[u8],
) -> Result<(), Errno> {
    if dst.len() != src.len() {
        return Err(Errno::EINVAL);
    }
    decline_kernel_user_faults();
    let (mut done, mut page, mut faults) = (0, usize::MAX, 0);
    while done < src.len() {
        // `writer` fails when `space` is not the active one or the range leaves user space.
        let mut w = space
            .writer(dst.addr() + done, src.len() - done)
            .map_err(|_| Errno::EFAULT)?;
        match w.write_fallible(&mut VmReader::from(&src[done..])) {
            Ok(n) => done += n,
            Err((_, n)) => {
                done += n;
                let at = dst.addr() + done;
                if at & !(PAGE_SIZE - 1) != page {
                    (page, faults) = (at & !(PAGE_SIZE - 1), 0);
                }
                faults += 1;
                if faults > MAX_FAULTS_PER_PAGE || !handle_cow_fault(space, agent, at)? {
                    return Err(Errno::EFAULT);
                }
            }
        }
    }
    Ok(())
}

/// Copies user memory at `src` (the active `space`) into `dst`. Reading never needs a
/// copy-on-write break; `EFAULT` for an unmapped or unreadable range.
pub fn copy_from_user(space: &VmSpace, dst: &mut [u8], src: UserSlice) -> Result<(), Errno> {
    if dst.len() != src.len() {
        return Err(Errno::EINVAL);
    }
    if dst.is_empty() {
        return Ok(());
    }
    decline_kernel_user_faults();
    let mut r = space
        .reader(src.addr(), src.len())
        .map_err(|_| Errno::EFAULT)?;
    match r.read_fallible(&mut VmWriter::from(dst)) {
        Ok(n) if n == src.len() => Ok(()),
        _ => Err(Errno::EFAULT),
    }
}

#[cfg(ktest)]
mod test {
    use alloc::sync::Arc;

    use ostd::{
        mm::{CachePolicy, PageProperty, VmIo},
        prelude::*,
    };

    use super::*;

    const DATA: Vaddr = 0x50_0000;

    /// `pages` pages at DATA, each filled with `fill`.
    fn space_with(pages: usize, fill: u8, flags: PageFlags) -> Arc<VmSpace> {
        let space = VmSpace::new();
        let guard = disable_preempt();
        let mut c = space
            .cursor_mut(&guard, &(DATA..DATA + pages * PAGE_SIZE))
            .unwrap();
        for _ in 0..pages {
            let f = FrameAllocOptions::new().alloc_frame().unwrap();
            f.write_bytes(0, &[fill; PAGE_SIZE]).unwrap();
            c.map(
                f.into(),
                PageProperty::new_user(flags, CachePolicy::Writeback),
            );
        }
        drop(c);
        Arc::new(space)
    }

    fn byte(space: &Arc<VmSpace>, at: Vaddr) -> u8 {
        space.activate();
        let mut b = [0u8; 1];
        copy_from_user(space, &mut b, UserSlice::new(at, 1).unwrap()).unwrap();
        b[0]
    }

    #[ktest]
    fn first_kernel_write_after_cow_clone_breaks_cow() {
        let agent = Agent::new_root(1, 4);
        let parent = space_with(1, 0xAA, PageFlags::RW);
        let child = Arc::new(cow_clone(&parent, DATA..DATA + PAGE_SIZE));
        child.activate();
        let dst = UserSlice::new(DATA, 64).unwrap();
        copy_to_user(&child, &agent, dst, &[0x55; 64]).unwrap();
        assert_eq!(byte(&child, DATA), 0x55);
        assert_eq!(byte(&child, DATA + 63), 0x55);
        assert_eq!(byte(&child, DATA + 64), 0xAA);
        assert_eq!(byte(&parent, DATA), 0xAA, "parent frame untouched");
        assert_eq!(agent.mem_used_pages(), 1, "one page copied, charged");
    }

    #[ktest]
    fn copy_to_user_spanning_two_cow_pages_breaks_both() {
        let agent = Agent::new_root(1, 4);
        let parent = space_with(2, 0xAA, PageFlags::RW);
        let child = Arc::new(cow_clone(&parent, DATA..DATA + 2 * PAGE_SIZE));
        child.activate();
        let dst = UserSlice::new(DATA + PAGE_SIZE - 32, 64).unwrap();
        copy_to_user(&child, &agent, dst, &[0x55; 64]).unwrap();
        for at in [DATA + PAGE_SIZE - 32, DATA + PAGE_SIZE + 31] {
            assert_eq!(byte(&child, at), 0x55);
            assert_eq!(byte(&parent, at), 0xAA);
        }
        assert_eq!(agent.mem_used_pages(), 2);
    }

    #[ktest]
    fn copy_to_user_unmapped_is_efault() {
        let agent = Agent::new_root(1, 4);
        let rw = space_with(1, 0xAA, PageFlags::RW);
        rw.activate();
        let past = UserSlice::new(DATA + PAGE_SIZE, 8).unwrap();
        assert_eq!(copy_to_user(&rw, &agent, past, &[1; 8]), Err(Errno::EFAULT));
        // Half mapped: the mapped half is written, then the copy fails.
        let straddle = UserSlice::new(DATA + PAGE_SIZE - 8, 16).unwrap();
        assert_eq!(
            copy_to_user(&rw, &agent, straddle, &[1; 16]),
            Err(Errno::EFAULT)
        );
        assert_eq!(copy_from_user(&rw, &mut [0; 8], past), Err(Errno::EFAULT));

        // Read-only and not copy-on-write: a plain fault, no break, no charge.
        let ro = space_with(1, 0xAA, PageFlags::R);
        ro.activate();
        let dst = UserSlice::new(DATA, 8).unwrap();
        assert_eq!(copy_to_user(&ro, &agent, dst, &[1; 8]), Err(Errno::EFAULT));
        assert_eq!(byte(&ro, DATA), 0xAA);
        assert_eq!(agent.mem_used_pages(), 0);
    }

    #[ktest]
    fn copy_to_user_over_budget_is_enomem() {
        let parent = space_with(1, 0xAA, PageFlags::RW);
        let child = Arc::new(cow_clone(&parent, DATA..DATA + PAGE_SIZE));
        child.activate();
        let dst = UserSlice::new(DATA, 8).unwrap();
        let poor = Agent::new_root(1, 0);
        assert_eq!(
            copy_to_user(&child, &poor, dst, &[1; 8]),
            Err(Errno::ENOMEM)
        );
        assert_eq!(byte(&child, DATA), 0xAA);
    }
}
