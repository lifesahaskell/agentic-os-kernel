//! Walking-skeleton program: SSE math, a `.bss` write, an unknown syscall, an empty-slot call and
//! one call/reply round trip. `arg` is the endpoint slot; the exit code is the failing step.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering},
};

use abi::{IpcArgs, errno};
use libsys::{call, raw::syscall6};

static BSS_CELL: AtomicU64 = AtomicU64::new(0);

fn main(ep: u64) -> u64 {
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

    let mut empty = IpcArgs::default();
    if call(63, &mut empty) != Err(errno::EBADF) {
        return 6;
    }

    let mut args = IpcArgs {
        label: 7,
        words: [41, 0, 0, 0],
        ..IpcArgs::default()
    };
    if call(ep, &mut args).is_err() {
        return 4;
    }
    if args.words[0] != 42 || args.words[1] != 7 {
        return 5;
    }
    0
}

libsys::entry!(main);
