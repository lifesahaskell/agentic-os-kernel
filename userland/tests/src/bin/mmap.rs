//! Self-memory probe: `mmap`, `mprotect`, `munmap` and `brk` on this process's own space. `arg`
//! is the mode.
//!
//! Mode 1: map single anonymous pages until a call fails, probing each one with a `FUTEX_WAIT`
//! that expects 1 against a word holding 0. `-EAGAIN` means the page is mapped and readable; any
//! other result exits `0xdead`. Exits `(errno << 16) | pages mapped`, so the kernel test can name
//! both the errno and the exact page where the budget ran out. `0xbeef` if 1,000 maps all worked.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{
    FUTEX_WAIT, MAP_ANONYMOUS, MAP_PRIVATE, PROT_READ, PROT_WRITE, SYS_FUTEX, SYS_MMAP, errno,
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

fn main(mode: u64) -> u64 {
    match mode {
        1 => map_pages(),
        _ => 98,
    }
}

libsys::entry!(main);
