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
    agent::{AgentError, Payer},
    syscall::{Errno, UserSlice},
};

/// Software bit marking a page that is read-only only because it is shared copy-on-write.
const COW: PageFlags = PageFlags::AVAIL2;

/// Why `cow_clone` refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneError {
    /// A range that is not page-aligned, is empty or inverted, or reaches above `abi::USER_TOP`.
    Range,
    /// A huge page or an MMIO mapping at this address.
    Unsupported(Vaddr),
    /// A refused node charge.
    Budget,
}

/// Page-table node frames a clone allocates to map `va`, given the previously mapped `prev`: 3
/// for the first page (one node per level below the root), otherwise one for each of the 2 MiB,
/// 1 GiB and 512 GiB windows whose index differs from `prev`'s.
fn nodes_for(prev: Option<Vaddr>, va: Vaddr) -> usize {
    match prev {
        None => 3,
        Some(p) => [21, 30, 39]
            .into_iter()
            .filter(|s| p >> s != va >> s)
            .count(),
    }
}

/// Clones the mappings of `parent` in `range` into a new space. Writable pages become read-only
/// and copy-on-write in both spaces; nothing is copied until a write faults.
///
/// The child's page-table node frames are charged to `payer` as the walk allocates them: one for
/// the root, then `nodes_for` per mapped page. The caller owns the returned total through
/// whatever ledger `payer` keeps. Known gap (D-07): the data frames the child shares with the
/// parent stay charged to the parent, so the clone under-charges data, never leaks it.
///
/// A failed clone is undone: the parent's TLB is flushed (the walk may already have
/// write-protected pages, which stay copy-on-write and are taken back by the sole-owner takeover
/// on their next fault), every node charge is refunded, and the half-built child is dropped.
/// `Range`: the range is not page-aligned, is empty or inverted, or ends above `abi::USER_TOP`.
/// `Unsupported`: a huge page or an MMIO mapping. `Budget`: a node charge was refused.
pub fn cow_clone(
    parent: &VmSpace,
    range: Range<Vaddr>,
    payer: &dyn Payer,
) -> Result<VmSpace, CloneError> {
    let aligned = |a: Vaddr| a & (PAGE_SIZE - 1) == 0;
    if !aligned(range.start)
        || !aligned(range.end)
        || range.start >= range.end
        || range.end > abi::USER_TOP
    {
        return Err(CloneError::Range);
    }
    payer.charge_pages(1).map_err(|_| CloneError::Budget)?;
    let mut charged = 1;
    let child = VmSpace::new();
    let guard = disable_preempt();
    let (Ok(mut pc), Ok(mut cc)) = (
        parent.cursor_mut(&guard, &range),
        child.cursor_mut(&guard, &range),
    ) else {
        payer.uncharge_pages(charged);
        return Err(CloneError::Range);
    };

    let mut failed = None;
    let mut prev = None;
    while let Some(va) = pc.find_next(range.end - pc.virt_addr()) {
        let Ok((r, item)) = pc.query() else {
            failed = Some(CloneError::Range);
            break;
        };
        let Some(VmQueriedItem::MappedRam { frame, mut prop }) = item else {
            failed = Some(CloneError::Unsupported(va));
            break;
        };
        if r.len() != PAGE_SIZE {
            failed = Some(CloneError::Unsupported(va));
            break;
        }
        let frame: UFrame = (*frame).clone();
        let n = nodes_for(prev, va);
        if n > 0 && payer.charge_pages(n).is_err() {
            failed = Some(CloneError::Budget);
            break;
        }
        charged += n;
        prev = Some(va);
        // The source cursor cannot jump to `range.end`, so the last page ends the walk.
        let last = va + PAGE_SIZE >= range.end;
        if prop.flags.contains(PageFlags::W) {
            prop.flags = (prop.flags - PageFlags::W) | COW;
            pc.protect_next(PAGE_SIZE, |f, _| *f = (*f - PageFlags::W) | COW);
        } else if !last && pc.jump(va + PAGE_SIZE).is_err() {
            failed = Some(CloneError::Range);
            break;
        }
        if cc.jump(va).is_err() {
            failed = Some(CloneError::Range);
            break;
        }
        cc.map(frame, prop);
        if last || pc.virt_addr() >= range.end {
            break;
        }
    }

    pc.flusher().issue_tlb_flush(TlbFlushOp::for_range(range));
    pc.flusher().dispatch_tlb_flush();
    drop((pc, cc));
    match failed {
        Some(e) => {
            payer.uncharge_pages(charged);
            drop(child);
            Err(e)
        }
        None => Ok(child),
    }
}

/// Resolves a write fault at `addr` if it hit a copy-on-write page. Returns `Ok(false)` for any
/// other fault (a real SIGSEGV), including an address outside the user half. The copy is charged
/// to `payer`.
///
/// `addr` comes straight from a ring-3 fault and can be anything. Once it is below `USER_TOP` and
/// `cursor_mut` accepted the page, the `query` and the re-`jump` below cannot fail: the page is
/// inside the cursor's own range. If either does anyway, the query declines the fault and the
/// re-`jump` refunds its charge and answers `NoMemory`, instead of panicking.
pub fn handle_cow_fault(
    space: &VmSpace,
    payer: &dyn Payer,
    addr: Vaddr,
) -> Result<bool, AgentError> {
    // A ring-3 write fault can carry any address, and nothing at or above `USER_TOP` is a page of
    // this space. This also keeps `page + PAGE_SIZE` from overflowing.
    if addr >= abi::USER_TOP {
        return Ok(false);
    }
    let page = addr & !(PAGE_SIZE - 1);
    let guard = disable_preempt();
    // Defence in depth: OSTD rejects a range outside the user page table, and that stays a
    // declined fault, not a panic.
    let Ok(mut c) = space.cursor_mut(&guard, &(page..page + PAGE_SIZE)) else {
        return Ok(false);
    };
    let Ok((_, item)) = c.query() else {
        return Ok(false);
    };
    let (mut prop, old) = match item {
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

    // Allocating first means a failed allocation charges nothing, and a refused charge frees the
    // frame when `new` drops. No refund path is needed.
    let new = FrameAllocOptions::new()
        .alloc_frame()
        .map_err(|_| AgentError::NoMemory)?;
    payer.charge_pages(1)?;
    new.writer().write(&mut old.reader());
    c.unmap(PAGE_SIZE);
    if c.jump(page).is_err() {
        payer.uncharge_pages(1);
        return Err(AgentError::NoMemory);
    }
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
/// breaks it on `payer`'s budget and retries, so the kernel never writes into a frame the
/// parent still shares. `EFAULT`: unmapped, not writable, or the fault would not resolve.
/// `ENOMEM`: the budget refused the copy.
pub fn copy_to_user(
    space: &VmSpace,
    payer: &dyn Payer,
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
                if faults > MAX_FAULTS_PER_PAGE || !handle_cow_fault(space, payer, at)? {
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
        io::IoMem,
        mm::{CachePolicy, PageProperty, VmIo},
        prelude::*,
    };

    use super::*;
    use crate::agent::Agent;

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

    /// One page filled with `fill` at each address in `at`, each mapped through its own cursor.
    fn space_with_at(at: &[Vaddr], fill: u8, flags: PageFlags) -> Arc<VmSpace> {
        let space = VmSpace::new();
        let guard = disable_preempt();
        for &va in at {
            let mut c = space.cursor_mut(&guard, &(va..va + PAGE_SIZE)).unwrap();
            let f = FrameAllocOptions::new().alloc_frame().unwrap();
            f.write_bytes(0, &[fill; PAGE_SIZE]).unwrap();
            c.map(
                f.into(),
                PageProperty::new_user(flags, CachePolicy::Writeback),
            );
        }
        drop(guard);
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
        let child = Arc::new(
            cow_clone(&parent, DATA..DATA + PAGE_SIZE, &*Agent::new_root(1, 64))
                .expect("cow_clone"),
        );
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
        let child = Arc::new(
            cow_clone(
                &parent,
                DATA..DATA + 2 * PAGE_SIZE,
                &*Agent::new_root(1, 64),
            )
            .expect("cow_clone"),
        );
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
        let child = Arc::new(
            cow_clone(&parent, DATA..DATA + PAGE_SIZE, &*Agent::new_root(1, 64))
                .expect("cow_clone"),
        );
        child.activate();
        let dst = UserSlice::new(DATA, 8).unwrap();
        let poor = Agent::new_root(1, 0);
        assert_eq!(
            copy_to_user(&child, &poor, dst, &[1; 8]),
            Err(Errno::ENOMEM)
        );
        assert_eq!(byte(&child, DATA), 0xAA);
        assert_eq!(poor.mem_used_pages(), 0);
        assert_eq!(Errno::from(AgentError::NoMemory), Errno::ENOMEM);
    }

    #[ktest]
    fn cow_fault_outside_user_half_is_declined() {
        let agent = Agent::new_root(1, 4);
        let space = space_with(1, 0xAA, PageFlags::RW);
        // Control: the last user byte is unmapped and inside the user page table, so this
        // declines at HEAD too. It comes first so a red failure is from the kernel-half cases.
        assert_eq!(
            handle_cow_fault(&space, &agent, abi::USER_TOP - 1),
            Ok(false),
            "addr {:#x}",
            abi::USER_TOP - 1
        );
        for addr in [
            0xffff_8000_0000_0000,
            0xffff_ffff_ffff_f000,
            usize::MAX,
            abi::USER_TOP,
        ] {
            assert_eq!(
                handle_cow_fault(&space, &agent, addr),
                Ok(false),
                "addr {addr:#x}"
            );
        }
        assert_eq!(agent.mem_used_pages(), 0);
    }

    const LOW: Vaddr = 0x40_0000;
    const MID: Vaddr = 0x60_0000;

    #[ktest]
    fn cow_clone_partial_failure_unwinds() {
        let parent = space_with_at(&[LOW, MID], 0xAA, PageFlags::RW);
        let poor = Agent::new_root(1, 4);
        let rich = Agent::new_root(1, 5);
        let fresh = Agent::new_root(1, 4);
        let b = crate::probe::settled();

        // Root 1 + first page 3 = 4 fits; the second page opens a new 2 MiB window and needs 1.
        let refused = cow_clone(&parent, LOW..MID + PAGE_SIZE, &*poor);
        assert_eq!(refused.err(), Some(CloneError::Budget));
        assert_eq!(
            poor.mem_used_pages(),
            0,
            "the unwind refunds every node charge"
        );
        assert_eq!(
            crate::probe::settled(),
            b,
            "the unwind frees the child's nodes"
        );
        assert_eq!(byte(&parent, LOW), 0xAA);
        assert_eq!(byte(&parent, MID), 0xAA);

        // The walk write-protected the first page and stopped before the second.
        assert_eq!(handle_cow_fault(&parent, &*fresh, LOW), Ok(true));
        assert_eq!(
            fresh.mem_used_pages(),
            0,
            "sole owner again: taken over, not copied"
        );
        assert_eq!(handle_cow_fault(&parent, &*fresh, MID), Ok(false));

        let child = Arc::new(cow_clone(&parent, LOW..MID + PAGE_SIZE, &*rich).expect("cow_clone"));
        assert_eq!(rich.mem_used_pages(), 5);
        assert_eq!(byte(&child, LOW), 0xAA);
        assert_eq!(byte(&child, MID), 0xAA);
    }

    #[ktest]
    fn cow_clone_rejects_bad_ranges() {
        let parent = space_with_at(&[LOW], 0xAA, PageFlags::RW);
        let payer = Agent::new_root(1, 64);
        for range in [
            LOW + 1..LOW + 2 * PAGE_SIZE,
            LOW..LOW + PAGE_SIZE + 1,
            LOW..LOW,
            LOW + PAGE_SIZE..LOW,
            LOW..abi::USER_TOP + PAGE_SIZE,
        ] {
            assert_eq!(
                cow_clone(&parent, range.clone(), &*payer).err(),
                Some(CloneError::Range),
                "range {range:x?}"
            );
            assert_eq!(payer.mem_used_pages(), 0, "range {range:x?}");
        }

        let mmio = VmSpace::new();
        {
            let guard = disable_preempt();
            let mut c = mmio.cursor_mut(&guard, &(LOW..LOW + PAGE_SIZE)).unwrap();
            let iomem = IoMem::acquire(0x100_000_000_000..0x100_000_001_000).expect("iomem");
            let prop = PageProperty::new_user(PageFlags::RW, CachePolicy::Uncacheable);
            c.map_iomem(iomem, prop, PAGE_SIZE, 0);
        }
        assert_eq!(
            cow_clone(&mmio, LOW..LOW + PAGE_SIZE, &*payer).err(),
            Some(CloneError::Unsupported(LOW))
        );
        assert_eq!(payer.mem_used_pages(), 0);

        assert_eq!(Errno::from(CloneError::Range), Errno::EINVAL);
        assert_eq!(Errno::from(CloneError::Unsupported(LOW)), Errno::EINVAL);
        assert_eq!(Errno::from(CloneError::Budget), Errno::ENOMEM);
    }

    #[ktest]
    fn cow_clone_charge_matches_frames() {
        let at = [LOW, LOW + PAGE_SIZE, MID, 0x4000_0000, 0x80_0000_0000];
        let parent = space_with_at(&at, 0xAA, PageFlags::RW);
        let payer = Agent::new_root(1, 64);
        let b = crate::probe::settled();

        // Root 1, first page 3, same window 0, new 2 MiB window 1, new 1 GiB window 2, new
        // 512 GiB window 3.
        let child = cow_clone(&parent, LOW..0x80_0000_1000, &*payer).expect("cow_clone");
        assert_eq!(payer.mem_used_pages(), 10);
        assert_eq!(b - crate::probe::settled(), 10 * PAGE_SIZE);
        drop(child);
        assert_eq!(crate::probe::settled(), b);
    }
}
