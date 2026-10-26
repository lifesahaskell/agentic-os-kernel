//! Null-IPC timing client. `arg` is the SEND slot of `nullsrv`; `arg + 1` is the SEND slot of the
//! report endpoint. 1,000 warm-up calls, then 10,000 timed calls with `rdtsc` around each, sorted;
//! sends `[n, median, p99, max]` (cycles; median is element 5,000, p99 element 9,900) to the
//! report endpoint with `call`, then exits 0. The samples live on the stack
//! (`abi::USER_STACK_PAGES` leaves room), because sorting needs a plain `&mut [u64]`.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::IpcArgs;
use libsys::{call, tsc};

const WARMUP: usize = 1_000;
const N: usize = 10_000;

fn main(srv: u64) -> u64 {
    for _ in 0..WARMUP {
        let mut a = IpcArgs::default();
        if call(srv, &mut a).is_err() {
            return 1;
        }
    }

    let mut samples = [0u64; N];
    for s in samples.iter_mut() {
        let mut a = IpcArgs::default();
        let t0 = tsc();
        let r = call(srv, &mut a);
        let t1 = tsc();
        if r.is_err() {
            return 2;
        }
        *s = t1.wrapping_sub(t0);
    }
    samples.sort_unstable();

    let mut report = IpcArgs {
        words: [
            N as u64,
            samples[N / 2],
            samples[N * 99 / 100],
            samples[N - 1],
        ],
        ..IpcArgs::default()
    };
    if call(srv + 1, &mut report).is_err() {
        return 3;
    }
    0
}

libsys::entry!(main);
