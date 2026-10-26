//! Self-memory probe: `mmap`, `mprotect`, `munmap` and `brk` on this process's own space. `arg`
//! is the mode.
//!
//! Mode 1: map single anonymous pages until a call fails, probing each one with a `FUTEX_WAIT`
//! that expects 1 against a word holding 0. `-EAGAIN` means the page is mapped and readable; any
//! other result exits `0xdead`. Exits `(errno << 16) | pages mapped`, so the kernel test can name
//! both the errno and the exact page where the budget ran out. `0xbeef` if 1,000 maps all worked.
//!
//! Mode 2: `mprotect`, `munmap`, `PROT_NONE` and `MAP_FIXED` in nine steps. Exits 0, or the number
//! of the first step whose result was wrong.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{
    FUTEX_WAIT, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MMAP_TOP, PROT_NONE, PROT_READ, PROT_WRITE,
    SYS_FUTEX, SYS_MMAP, SYS_MPROTECT, SYS_MUNMAP, errno,
};
use libsys::raw::syscall6;

const PAGE: usize = 4096;

/// What `FUTEX_WAIT` says about the word at `addr`, which holds 0 and is expected to hold 1:
/// `-EAGAIN` for a readable page, `-EFAULT` for one that is unmapped or inaccessible.
fn probe(addr: usize) -> isize {
    syscall6(SYS_FUTEX, [addr, FUTEX_WAIT, 1, 0, 0, 0])
}

fn map_pages() -> u64 {
    let mut count = 0u64;
    while count < 1000 {
        let r = syscall6(
            SYS_MMAP,
            [
                0,
                PAGE,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                usize::MAX,
                0,
            ],
        );
        if r < 0 {
            return ((-r as u64) << 16) | count;
        }
        if probe(r as usize) != -errno::EAGAIN {
            return 0xdead;
        }
        count += 1;
    }
    0xbeef
}

fn mmap(addr: usize, pages: usize, prot: usize, extra: usize) -> isize {
    syscall6(
        SYS_MMAP,
        [
            addr,
            pages * PAGE,
            prot,
            MAP_PRIVATE | MAP_ANONYMOUS | extra,
            usize::MAX,
            0,
        ],
    )
}

fn mprotect(addr: usize, pages: usize, prot: usize) -> isize {
    syscall6(SYS_MPROTECT, [addr, pages * PAGE, prot, 0, 0, 0])
}

fn munmap(addr: usize, pages: usize) -> isize {
    syscall6(SYS_MUNMAP, [addr, pages * PAGE, 0, 0, 0, 0])
}

/// The nine steps of mode 2. Returns the first failing step number, or 0.
fn protect_and_unmap() -> u64 {
    let rw = PROT_READ | PROT_WRITE;
    let base = mmap(0, 2, rw, 0);
    if base < 0 {
        return 1;
    }
    let (p0, p1) = (base as usize, base as usize + PAGE);
    if mprotect(p0, 1, PROT_NONE) != 0 || probe(p0) != -errno::EFAULT {
        return 2;
    }
    if mprotect(p0, 1, rw) != 0 || probe(p0) != -errno::EAGAIN {
        return 3;
    }
    if munmap(p1, 1) != 0 || probe(p1) != -errno::EFAULT {
        return 4;
    }
    if munmap(p1, 1) != 0 {
        return 5;
    }
    let none = mmap(0, 1, PROT_NONE, 0);
    if none < 0 || probe(none as usize) != -errno::EFAULT {
        return 6;
    }
    if mprotect(none as usize, 1, PROT_READ) != 0 || probe(none as usize) != -errno::EAGAIN {
        return 7;
    }
    if mmap(p1, 1, rw, MAP_FIXED) != p1 as isize {
        return 8;
    }
    if mprotect(MMAP_TOP - 2 * PAGE, 1, rw) != -errno::ENOMEM {
        return 9;
    }
    0
}

fn main(mode: u64) -> u64 {
    match mode {
        1 => map_pages(),
        2 => protect_and_unmap(),
        _ => 98,
    }
}

libsys::entry!(main);
