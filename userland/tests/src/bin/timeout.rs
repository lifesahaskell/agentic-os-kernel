//! Timed waits from ring 3, by `arg`. Each case exits 0 on the expected result, otherwise 100 plus
//! the errno it saw (or 99 for success where a failure was due, 200 for an unknown case).
//!
//! 0: `FUTEX_WAIT` on a stack word holding 0 with a 20 ms timespec: `-ETIMEDOUT`.
//! 1: `RECV` on slot 0 (an endpoint nothing sends to) with a 20 ms timeout: `-ETIMEDOUT`.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicU32, Ordering};

use abi::{IpcArgs, SYS_FUTEX, SYS_RECV, Timespec, errno};
use libsys::raw::syscall6;

const FUTEX_WAIT: usize = 0;

/// Exit code for a result that should have been `-want`.
fn verdict(r: isize, want: isize) -> u64 {
    if r == -want {
        0
    } else if r >= 0 {
        99
    } else {
        100 + (-r) as u64
    }
}

fn futex_timeout() -> u64 {
    let word = AtomicU32::new(0);
    let ts = Timespec {
        sec: 0,
        nsec: 20_000_000,
    };
    let r = syscall6(
        SYS_FUTEX,
        [
            &word as *const AtomicU32 as usize,
            FUTEX_WAIT,
            0,
            &ts as *const Timespec as usize,
            0,
            0,
        ],
    );
    // Keep the word live across the call: the kernel reads it, the compiler cannot see that.
    let _ = word.load(Ordering::Relaxed);
    verdict(r, errno::ETIMEDOUT)
}

/// `recv` on slot 0 with a 20 ms timeout in `a[2]`; nothing sends to it.
fn recv_timeout() -> u64 {
    let mut args = IpcArgs::default();
    let r = syscall6(
        SYS_RECV,
        [0, &mut args as *mut IpcArgs as usize, 20, 0, 0, 0],
    );
    verdict(r, errno::ETIMEDOUT)
}

fn main(arg: u64) -> u64 {
    match arg {
        0 => futex_timeout(),
        1 => recv_timeout(),
        _ => 200,
    }
}

libsys::entry!(main);
