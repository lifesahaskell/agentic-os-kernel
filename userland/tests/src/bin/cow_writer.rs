//! Writes 0x42 to the first word of the test page, then exits 0. Touches no other writable
//! page, so a copy-on-write test sees exactly one page copied.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::Ordering::Relaxed;

use libsys::test_page;

fn main(_: u64) -> u64 {
    test_page()[0].store(0x42, Relaxed);
    0
}

libsys::entry!(main);
