//! Walking-skeleton program: SSE math, a `.bss` write, one syscall, exit code = failing step.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering},
};

use abi::errno;
use libsys::raw::syscall6;

static BSS_CELL: AtomicU64 = AtomicU64::new(0);

fn main(_arg: u64) -> u64 {
    let product = black_box(1.5f64) * black_box(2.25);
    let quotient = black_box(10.0f64) / black_box(4.0);
    if product != 3.375 || quotient != 2.5 {
        return 1;
    }

    let cell = black_box(&BSS_CELL);
    cell.store(0xA0C, Ordering::Relaxed);
    if cell.load(Ordering::Relaxed) != 0xA0C {
        return 2;
    }

    if syscall6(0xFFFF, [0; 6]) != -errno::ENOSYS {
        return 3;
    }
    0
}

libsys::entry!(main);
