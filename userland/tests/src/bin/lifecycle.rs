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
//!
//! Mode 2: maps a 4-page stack and starts a thread on it in this space (`SPACE_SELF`). The thread
//! calls `SYS_EXIT_UNMAP` over its own stack. This thread probes the stack with `FUTEX_WAIT` until
//! it is gone, then maps 4 pages again, which fits the owner's budget only if the unmap refunded
//! the first four. Exits 0, or the number of the first failing step: 1 the first mmap, 2
//! `thread_create`, 3 the stack never went away, 4 the second mmap.
//!
//! Mode 3: `SYS_EXIT_UNMAP` with an unaligned address, a zero length and a range that leaves user
//! space must each return `-EINVAL` and leave this thread running. Exits 0, or the step number.
//!
//! Mode 4: starts a thread whose `clear_tid` is the address of a static word holding 1, then
//! `FUTEX_WAIT`s on the word with a 1 s timeout. The thread yields 50 times and exits; the kernel
//! must zero the word and wake this thread, so the wait returns 0 and the word reads 0. Exits 0, or
//! the number of the first failing step: 1 the stack mmap, 2 `thread_create`, 3 the wait timed out,
//! 4 the wait returned another value (`-EAGAIN` means the word was cleared before the wait began),
//! 5 the word did not read 0 after the wait.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::{
    hint::black_box,
    sync::atomic::{AtomicU32, Ordering},
};

use abi::{
    ARCH_GET_FS, ARCH_SET_FS, FUTEX_WAIT, MAP_ANONYMOUS, MAP_PRIVATE, PROT_READ, PROT_WRITE,
    SPACE_SELF, SYS_ARCH_PRCTL, SYS_EXIT_UNMAP, SYS_FUTEX, SYS_MMAP, SYS_THREAD_CREATE, ThreadCtx,
    Timespec, USER_TOP, errno,
};
use libsys::{exit, raw::syscall6, sched_yield};

const PAGE: usize = 4096;

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

fn mmap(pages: usize) -> isize {
    syscall6(
        SYS_MMAP,
        [
            0,
            pages * PAGE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

fn exit_unmap(addr: usize, len: usize, code: usize) -> isize {
    syscall6(SYS_EXIT_UNMAP, [addr, len, code, 0, 0, 0])
}

/// What `FUTEX_WAIT` says about the zeroed word at `addr` when it expects 1: `-EAGAIN` for a
/// readable page, `-EFAULT` for an unmapped one.
fn probe(addr: usize) -> isize {
    syscall6(SYS_FUTEX, [addr, FUTEX_WAIT, 1, 0, 0, 0])
}

/// Starts a thread in this space with the given context; the result is the raw `rax`.
fn spawn(rip: usize, rsp: usize, arg: usize, clear_tid: usize) -> isize {
    let mut ctx = ThreadCtx {
        rip: rip as u64,
        rsp: rsp as u64,
        arg: arg as u64,
        clear_tid: clear_tid as u64,
        ..ThreadCtx::default()
    };
    let r = syscall6(
        SYS_THREAD_CREATE,
        [
            SPACE_SELF as usize,
            &mut ctx as *mut ThreadCtx as usize,
            0,
            0,
            0,
            0,
        ],
    );
    // The kernel wrote `tid` through the pointer; keep the compiler from dropping the struct.
    black_box(&ctx);
    r
}

/// The entry of the thread mode 2 starts: frees the stack it stands on and exits. `0xdead` if the
/// call returned, which is safe to reach because a refused call leaves the stack mapped.
extern "C" fn unmap_own_stack(base: usize) -> ! {
    exit_unmap(base, 4 * PAGE, 0);
    exit(0xdead)
}

fn exit_unmap_frees_the_stack() -> u64 {
    let base = mmap(4);
    if base < 0 {
        return 1;
    }
    let base = base as usize;
    let entry = unmap_own_stack as *const () as usize;
    if spawn(entry, base + 4 * PAGE - 8, base, 0) < 0 {
        return 2;
    }
    let mut probes = 0;
    while probe(base) != -errno::EFAULT {
        probes += 1;
        if probes > 100_000 {
            return 3;
        }
        sched_yield();
    }
    if mmap(4) < 0 {
        return 4;
    }
    0
}

fn exit_unmap_refuses_bad_ranges() -> u64 {
    let base = mmap(1);
    if base < 0 {
        return 1;
    }
    let base = base as usize;
    if exit_unmap(base + 1, PAGE, 0) != -errno::EINVAL {
        return 2;
    }
    if exit_unmap(base, 0, 0) != -errno::EINVAL {
        return 3;
    }
    if exit_unmap(USER_TOP - PAGE, 2 * PAGE, 0) != -errno::EINVAL {
        return 4;
    }
    // Still here, and the page is still mapped.
    if probe(base) != -errno::EAGAIN {
        return 5;
    }
    0
}

/// The word mode 4's thread names as its `clear_tid`.
static WORD: AtomicU32 = AtomicU32::new(1);

/// The entry of the thread mode 4 starts: lets the main thread reach its wait, then exits.
extern "C" fn yield_then_exit(_: usize) -> ! {
    for _ in 0..50 {
        sched_yield();
    }
    exit(0)
}

fn clear_tid_wakes_the_waiter() -> u64 {
    let base = mmap(4);
    if base < 0 {
        return 1;
    }
    let base = base as usize;
    let word = &WORD as *const AtomicU32 as usize;
    let entry = yield_then_exit as *const () as usize;
    if spawn(entry, base + 4 * PAGE - 8, 0, word) < 0 {
        return 2;
    }
    let timeout = Timespec { sec: 1, nsec: 0 };
    let r = syscall6(
        SYS_FUTEX,
        [
            word,
            FUTEX_WAIT,
            1,
            &timeout as *const Timespec as usize,
            0,
            0,
        ],
    );
    black_box(&timeout);
    if r == -errno::ETIMEDOUT {
        return 3;
    }
    if r != 0 {
        return 4;
    }
    if WORD.load(Ordering::SeqCst) != 0 {
        return 5;
    }
    0
}

fn main(arg: u64) -> u64 {
    match arg & 0xff {
        0 => 0,
        1 => fs_base_round_trip(arg >> 8),
        2 => exit_unmap_frees_the_stack(),
        3 => exit_unmap_refuses_bad_ranges(),
        4 => clear_tid_wakes_the_waiter(),
        _ => 98,
    }
}

libsys::entry!(main);
