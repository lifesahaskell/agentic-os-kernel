//! `SYS_SIGRETURN` probe. `arg` = mode.
//! Mode 1: yield until the handler sets `FLAG`, then exit 42. The handler sets `FLAG` and returns
//! through `SYS_SIGRETURN`, so 42 means the kernel resumed the loop it interrupted.
//! Mode 3: call `SYS_SIGRETURN` from main with no redirect outstanding and exit with the errno:
//! 22 (`EINVAL`).
//! The handler exits 99 if `SYS_SIGRETURN` returns to it.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicBool, Ordering};

use abi::SYS_SIGRETURN;
use libsys::raw::syscall6;

static FLAG: AtomicBool = AtomicBool::new(false);

fn main(mode: u64) -> u64 {
    match mode {
        1 => {
            // Yields, because the scheduler does not preempt: a bare spin would starve the test
            // task that has to deliver the interrupt.
            while !FLAG.load(Ordering::Acquire) {
                libsys::sched_yield();
            }
            42
        }
        3 => (-syscall6(SYS_SIGRETURN, [0; 6])) as u64,
        _ => 98,
    }
}

fn handler(_pc: u64) -> u64 {
    FLAG.store(true, Ordering::Release);
    syscall6(SYS_SIGRETURN, [0; 6]);
    99
}

libsys::entry!(main, handler);
