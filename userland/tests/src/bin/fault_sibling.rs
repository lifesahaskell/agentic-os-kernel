//! One image, several ways to end, chosen by `arg`, for tests that run clones of it as sibling
//! threads of one space and watch the notes the kernel sends to the space's notify endpoint.
//!
//! - 0: exit 7.
//! - 1: store a word into the test page, then exit 8 if the store returns. With the page mapped
//!   read-only the store faults, and the exit is reached only when the fault's verdict resumed
//!   the thread.
//! - any other value: exit 9.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::Ordering::Relaxed;

use libsys::test_page;

fn main(arg: u64) -> u64 {
    match arg {
        0 => 7,
        1 => {
            test_page()[0].store(0x33, Relaxed);
            8
        }
        _ => 9,
    }
}

libsys::entry!(main);
