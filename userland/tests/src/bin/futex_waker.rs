//! Stores 1 in the first word of the test page, then `futex(TEST_DATA, FUTEX_WAKE, 1)`, and exits
//! with the raw result: the number of waiters woken.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::Ordering::Relaxed;

use abi::{SYS_FUTEX, TEST_DATA};
use libsys::{raw::syscall6, test_page};

const FUTEX_WAKE: usize = 1;

fn main(_: u64) -> u64 {
    test_page()[0].store(1, Relaxed);
    syscall6(SYS_FUTEX, [TEST_DATA, FUTEX_WAKE, 1, 0, 0, 0]) as u64
}

libsys::entry!(main);
