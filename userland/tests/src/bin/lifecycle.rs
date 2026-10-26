//! Thread-lifecycle probe: `arch_prctl`, `SYS_EXIT_UNMAP` and the `clear_tid` word. `arg` is
//! `mode | (salt << 8)`.
//!
//! Mode 0: exits 0 at once.
//!
//! Mode 1: sets the FS base `0x1_2345_0000 + salt * 4096`, then reads it back after each of 100
//! `sched_yield` calls (each one saves the CPU base after `execute` and loads it again in
//! `pre_user_run`), then checks four error results. Exits 0, or the number of the first failing
//! step: 1 a base that did not survive, 2 `ARCH_SET_FS` at `USER_TOP` not `-EPERM` or moving the
//! base, 3 a GS code not `-EINVAL`, 4 `ARCH_GET_FS` through a kernel-half pointer not `-EFAULT`,
//! 5 through an unmapped user pointer not `-EFAULT`. The program has no TLS and never dereferences
//! the base.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::hint::black_box;

use abi::{ARCH_GET_FS, ARCH_SET_FS, SYS_ARCH_PRCTL, USER_TOP, errno};
use libsys::{raw::syscall6, sched_yield};

const KERNEL_HALF: usize = 0xffff_8000_0000_0000;
/// Linux's `ARCH_SET_GS`, which this kernel refuses.
const ARCH_SET_GS: usize = 0x1001;

fn arch_prctl(code: usize, arg: usize) -> isize {
    syscall6(SYS_ARCH_PRCTL, [code, arg, 0, 0, 0, 0])
}

/// The FS base through `ARCH_GET_FS`, or the negative errno.
fn get_fs() -> Result<u64, isize> {
    let mut out = 0u64;
    let r = arch_prctl(ARCH_GET_FS, &mut out as *mut u64 as usize);
    if r < 0 { Err(r) } else { Ok(*black_box(&out)) }
}

fn fs_base_round_trip(salt: u64) -> u64 {
    let base = 0x1_2345_0000 + salt * 4096;
    if arch_prctl(ARCH_SET_FS, base as usize) != 0 {
        return 1;
    }
    for _ in 0..100 {
        sched_yield();
        if get_fs() != Ok(base) {
            return 1;
        }
    }
    if arch_prctl(ARCH_SET_FS, USER_TOP) != -errno::EPERM || get_fs() != Ok(base) {
        return 2;
    }
    if arch_prctl(ARCH_SET_GS, 0) != -errno::EINVAL {
        return 3;
    }
    if arch_prctl(ARCH_GET_FS, KERNEL_HALF) != -errno::EFAULT {
        return 4;
    }
    if arch_prctl(ARCH_GET_FS, 0x1000) != -errno::EFAULT {
        return 5;
    }
    0
}

fn main(arg: u64) -> u64 {
    match arg & 0xff {
        0 => 0,
        1 => fs_base_round_trip(arg >> 8),
        _ => 98,
    }
}

libsys::entry!(main);
