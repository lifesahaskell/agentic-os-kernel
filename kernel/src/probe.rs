//! Test-only free-frame probe: the one leak check that does not run through the teardown code
//! under test (D-03).
//!
//! `free()` reads the frame allocator's free bytes plus the heap allocator's slab bytes. A slab
//! page moves from the free count to the slab count when the heap grows, so the sum is stable
//! across slab growth and moves only when a frame leaves the pair, which is what a leak does.
//!
//! What it misses: heap allocations too large for a slab (A3). Those bypass the slab and come
//! straight out of the frame allocator, so they do show as a frame delta, but a large object that
//! the heap keeps cached is invisible. The kernel-side counters in 02-07 cover that gap.
//!
//! The counters are the OSDK base crate's own: `aok` names the same two crates, and cargo unifies
//! them to one copy, so the statics the base crate registers as the kernel's allocators are the
//! ones read here (assumption A1, decided by `frame_probe_spike_counts_one_frame`). A second,
//! unregistered copy would read 0.

use ostd::mm::PAGE_SIZE;

/// Free frame-allocator bytes plus slab bytes, now. Two reads are comparable only when both
/// were taken after the same protocol; see the gates in 02-07 and 02-09.
pub fn free() -> usize {
    osdk_frame_allocator::load_total_free_size() + osdk_heap_allocator::load_total_slab_size()
}

#[cfg(ktest)]
mod test {
    use alloc::{boxed::Box, vec::Vec};

    use ostd::{mm::FrameAllocOptions, prelude::*};

    use super::*;

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
}
