//! `SYS_SIGRETURN` probe. `arg` = mode.
//! Mode 1: yield until the handler sets `FLAG`, then exit 42. The handler sets `FLAG` and returns
//! through `SYS_SIGRETURN`, so 42 means the kernel resumed the loop it interrupted.
//! Mode 2: `FUTEX_WAIT` on a word that holds 0, with no timeout. The test interrupts the wait; the
//! handler sets `FLAG` and returns through `SYS_SIGRETURN`, and the wait's result comes back in
//! `rax` as the frame saved it: exit with the negated result when `FLAG` is set (4, `EINTR`),
//! else 98.
//! Mode 3: call `SYS_SIGRETURN` from main with no redirect outstanding and exit with the errno:
//! 22 (`EINVAL`).
//! Mode 4: yield forever, and so does the handler after it sets `FLAG`, so the test can nest
//! redirects. Yields, because the scheduler does not preempt: a bare spin would starve the test.
//! The handler exits 99 if `SYS_SIGRETURN` returns to it.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use abi::{FUTEX_WAIT, SYS_FUTEX, SYS_SIGRETURN};
use libsys::raw::syscall6;

static FLAG: AtomicBool = AtomicBool::new(false);
/// The futex word mode 2 sleeps on. Nothing ever changes it or wakes it.
static WORD: AtomicU32 = AtomicU32::new(0);
/// Which mode this process runs, so the handler knows whether to return or to yield forever.
static MODE: AtomicU32 = AtomicU32::new(0);

fn main(mode: u64) -> u64 {
    MODE.store(mode as u32, Ordering::Relaxed);
    match mode {
        1 => {
            // Yields, because the scheduler does not preempt: a bare spin would starve the test
            // task that has to deliver the interrupt.
            while !FLAG.load(Ordering::Acquire) {
                libsys::sched_yield();
            }
            42
        }
        2 => {
            let addr = &WORD as *const AtomicU32 as usize;
            let r = syscall6(SYS_FUTEX, [addr, FUTEX_WAIT, 0, 0, 0, 0]);
            if FLAG.load(Ordering::Acquire) {
                (-r) as u64
            } else {
                98
            }
        }
        3 => (-syscall6(SYS_SIGRETURN, [0; 6])) as u64,
        4 => loop {
            libsys::sched_yield();
        },
        _ => 98,
    }
}

fn handler(_pc: u64) -> u64 {
    FLAG.store(true, Ordering::Release);
    if MODE.load(Ordering::Relaxed) == 4 {
        loop {
            libsys::sched_yield();
        }
    }
    syscall6(SYS_SIGRETURN, [0; 6]);
    99
}

libsys::entry!(main, handler);
