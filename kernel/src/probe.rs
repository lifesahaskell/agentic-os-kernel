//! Test-only free-frame probe: the one leak check that does not run through the teardown code
//! under test (D-03).
//!
//! `free()` reads the frame allocator's free bytes plus the heap allocator's slab bytes. A slab
//! page moves from the free count to the slab count when the heap grows, so the sum is stable
//! across slab growth and moves only when a frame leaves the pair, which is what a leak does.
//!
//! What it misses: non-slab heap allocations (A3), which the kernel counters in 02-07 cover.
//!
//! The counters are the OSDK base crate's own: `aok` names the same two crates, and cargo unifies
//! them to one copy, so the statics the base crate registers as the kernel's allocators are the
//! ones read here (assumption A1, decided by `frame_probe_spike_counts_one_frame`). A second,
//! unregistered copy would read 0.

//! # Protocol: park, settle, read
//!
//! Two raw `free()` reads around an address space's life disagree for two reasons that are not
//! leaks. Frames freed through RCU are returned only after grace periods, which OSTD advances in
//! `switch_to_task` (a bare yield with no other runnable task switches nothing). And the per-CPU
//! activated-space slot pins the last space `activate`d, with its page-table nodes. `settled()`
//! is the one protocol the leak gates call before each read they compare: `park_activation`
//! (activate a never-mapped dummy space, releasing whatever the slot pinned), `settle` (force real
//! task switches), then `free()`.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

use ostd::{
    mm::{PAGE_SIZE, VmSpace},
    sync::{LocalIrqDisabled, SpinLock},
    task::{Task, TaskOptions},
};

/// Free frame-allocator bytes plus slab bytes, now. Two reads are comparable only when both
/// were taken after the same protocol; see the gates in 02-07 and 02-09.
pub fn free() -> usize {
    osdk_frame_allocator::load_total_free_size() + osdk_heap_allocator::load_total_slab_size()
}

/// Settle rounds `settled()` runs before it reads. Calibrated: the SUMMARY of 02-11 records the
/// logged delta curve after 1, 2, 4, 8 and 16 rounds. Raise it if the 16-round line ever shows a
/// smaller delta than the 8-round line.
pub const SETTLE_ROUNDS: usize = 8;

/// Runs `rounds` rounds of a no-op peer kernel task. The peer carries no task data, so
/// `sched::eligible` always admits it, and the caller yields until the peer has run. Each round
/// forces at least two real task switches, which is where OSTD advances RCU grace periods.
pub fn settle(rounds: usize) {
    for _ in 0..rounds {
        let done = Arc::new(AtomicBool::new(false));
        let peer = done.clone();
        TaskOptions::new(move || peer.store(true, Ordering::Release))
            .spawn()
            .unwrap();
        while !done.load(Ordering::Acquire) {
            Task::yield_now();
        }
    }
}

/// The dummy space `park_activation` activates. Created once and never mapped, so it pins the
/// same root frame during every read.
static PARK: SpinLock<Option<Arc<VmSpace>>, LocalIrqDisabled> = SpinLock::new(None);

/// Activates the never-mapped dummy space, which releases the space the per-CPU slot pinned
/// before.
pub fn park_activation() {
    let dummy = PARK
        .lock()
        .get_or_insert_with(|| Arc::new(VmSpace::new()))
        .clone();
    dummy.activate();
}

/// The probe protocol: park, settle, then read. Call it before each read you compare.
pub fn settled() -> usize {
    park_activation();
    settle(SETTLE_ROUNDS);
    free()
}

#[cfg(ktest)]
mod test {
    use alloc::{boxed::Box, vec::Vec};

    use ostd::{
        mm::{CachePolicy, FrameAllocOptions, PageFlags, PageProperty, Vaddr, tlb::TlbFlushOp},
        prelude::*,
        task::disable_preempt,
    };

    use super::*;

    const PAGE: Vaddr = 0x40_0000;

    /// A fresh space with one fresh frame mapped at `PAGE`. The mapping holds the only
    /// reference to the frame.
    fn space_with_one_page() -> VmSpace {
        let space = VmSpace::new();
        let frame = FrameAllocOptions::new().alloc_frame().unwrap();
        let guard = disable_preempt();
        let mut c = space.cursor_mut(&guard, &(PAGE..PAGE + PAGE_SIZE)).unwrap();
        c.map(
            frame.into(),
            PageProperty::new_user(PageFlags::RW, CachePolicy::Writeback),
        );
        drop(c);
        space
    }

    fn delta(base: usize) -> isize {
        base as isize - free() as isize
    }

    #[ktest]
    fn frame_probe_spike_counts_one_frame() {
        let frame_free = osdk_frame_allocator::load_total_free_size();
        let slab = osdk_heap_allocator::load_total_slab_size();
        println!("frame_probe: spike frame_free={} slab={}", frame_free, slab);
        assert_ne!(frame_free, 0, "frame allocator counter is dead: A1 fails");
        assert_ne!(slab, 0, "heap allocator counter is dead: A1 fails");

        let before = free();
        let frame = FrameAllocOptions::new().alloc_frame().unwrap();
        let during = free();
        println!("frame_probe: spike before={} during={}", before, during);
        assert_eq!(before - during, PAGE_SIZE);
        drop(frame);
        assert_eq!(free(), before);
    }

    #[ktest]
    fn frame_probe_spike_slab_growth_is_neutral() {
        let b = free();
        let slab_before = osdk_heap_allocator::load_total_slab_size();
        // Both the Vec buffer (128 pointers, 1 KiB) and each element (256 B) are slab-sized.
        let mut held: Vec<Box<[u8; 256]>> = Vec::with_capacity(128);
        for _ in 0..128 {
            held.push(Box::new([0xA5; 256]));
        }
        let slab_during = osdk_heap_allocator::load_total_slab_size();
        println!(
            "frame_probe: spike slab before={} during={}",
            slab_before, slab_during
        );
        assert!(slab_during > slab_before, "the slab did not grow: vacuous");
        assert_eq!(free(), b, "slab growth moved the probe");
        drop(held);
        assert_eq!(free(), b, "slab release moved the probe");
    }

    #[ktest]
    fn frame_probe_settle_returns_an_unmapped_frame() {
        let b = settled();
        {
            let space = space_with_one_page();
            let guard = disable_preempt();
            let range = PAGE..PAGE + PAGE_SIZE;
            let mut c = space.cursor_mut(&guard, &range).unwrap();
            c.unmap(PAGE_SIZE);
            c.flusher()
                .issue_tlb_flush(TlbFlushOp::for_range(range.clone()));
            c.flusher().dispatch_tlb_flush();
            drop(c);
            drop(guard);
        }
        // The space is dropped here. Count the rounds it takes for every frame to come back.
        let mut rounds = 0;
        for step in [1, 1, 2, 4, 8] {
            settle(step);
            rounds += step;
            println!("frame_probe: settle rounds={} delta={}", rounds, delta(b));
        }
        assert!(rounds >= SETTLE_ROUNDS);
        assert_eq!(delta(b), 0, "frames still missing after {} rounds", rounds);
    }

    #[ktest]
    fn frame_probe_activation_pin_released() {
        let b = settled();
        let space = Arc::new(space_with_one_page());
        space.activate();
        drop(space);
        // The per-CPU activated-space slot still holds the space, so its page-table nodes and
        // frame are expected to be missing here. An observation, not an assertion.
        settle(SETTLE_ROUNDS);
        println!("frame_probe: pinned delta={}", delta(b));
        assert_eq!(settled(), b, "the park did not release the pinned space");
    }
}
