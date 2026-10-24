//! FPU/SSE switch probe. `arg` = `mode | seed << 8`.
//! Mode 1: hold the seed's xmm pattern across 200 yields, exit 0 if it is intact, else 1.
//! Mode 2: hold it in a bare spin until redirected. Mode 3: hold it across yields until redirected.
//! The redirect handler exits 0 if the xmm registers it found equal the pattern, else 10 + the
//! index of the first mismatching register.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

use libsys::{
    fpu::{hold, pattern},
    rt::handler_xmm,
};

static SEED: AtomicU64 = AtomicU64::new(0);

fn main(arg: u64) -> u64 {
    let (mode, seed) = (arg & 0xff, arg >> 8);
    SEED.store(seed, Ordering::Relaxed);
    match mode {
        1 => u64::from(hold(seed, 200, true) != 0),
        2 => {
            hold(seed, u64::MAX, false);
            99
        }
        3 => {
            hold(seed, u64::MAX, true);
            99
        }
        _ => 98,
    }
}

fn handler(_pc: u64) -> u64 {
    let want = pattern(SEED.load(Ordering::Relaxed));
    let got = handler_xmm();
    (0..16)
        .find(|&i| got[i] != want[i])
        .map_or(0, |i| 10 + i as u64)
}

libsys::entry!(main, handler);
