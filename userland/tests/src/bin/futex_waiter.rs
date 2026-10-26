//! `futex(TEST_DATA, FUTEX_WAIT, 0x11, NULL)` on the test page, then exits with the raw result:
//! 0 when woken, `-EAGAIN` (as a u64) when the word did not hold 0x11.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{SYS_FUTEX, TEST_DATA};
use libsys::raw::syscall6;

const FUTEX_WAIT: usize = 0;

fn main(_: u64) -> u64 {
    syscall6(SYS_FUTEX, [TEST_DATA, FUTEX_WAIT, 0x11, 0, 0, 0]) as u64
}

libsys::entry!(main);
