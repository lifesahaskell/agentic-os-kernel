//! Yields 100 times, counting on the test page, then exits 0. The count must match the loop
//! index every time, so a thread resumed under another space's page tables exits 1 instead.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::Ordering::Relaxed;

use libsys::{sched_yield, test_page};

fn main(_: u64) -> u64 {
    let page = test_page();
    for i in 0..100u32 {
        if page[0].fetch_add(1, Relaxed) != i {
            return 1;
        }
        sched_yield();
    }
    0
}

libsys::entry!(main);
