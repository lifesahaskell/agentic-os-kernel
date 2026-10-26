//! One image, several ways to end, chosen by `arg`, for tests that run clones of it as sibling
//! threads of one space and watch the notes the kernel sends to the space's notify endpoint.
//!
//! - 0: exit 7.
//! - 1: store a word into the test page, then exit 8 if the store returns. With the page mapped
//!   read-only the store faults, and the exit is reached only when the fault's verdict resumed
//!   the thread.
//! - 2: ask for the endpoint in slot 0 to be this space's notify endpoint, twice. Exit with the
//!   first result if it is non-zero, else with the second. A space with no endpoint yet accepts the
//!   first and refuses the second, so the exit code is `-EPERM`.
//! - any other value: exit 9.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::Ordering::Relaxed;

use abi::{SPACE_SELF, SYS_SPACE_NOTIFY};
use libsys::{raw::syscall6, test_page};

fn space_notify_self(ep_slot: usize) -> isize {
    syscall6(SYS_SPACE_NOTIFY, [SPACE_SELF as usize, ep_slot, 0, 0, 0, 0])
}

fn main(arg: u64) -> u64 {
    match arg {
        0 => 7,
        1 => {
            test_page()[0].store(0x33, Relaxed);
            8
        }
        2 => {
            let first = space_notify_self(0);
            if first != 0 {
                first as u64
            } else {
                space_notify_self(0) as u64
            }
        }
        _ => 9,
    }
}

libsys::entry!(main);
