//! Bulk client: calls the endpoint in the SEND slot with 3 * 4096 + 123 seeded pattern bytes and
//! checks that every reply byte is the pattern byte XOR 0x5A. The buffers are zero-initialised
//! `.bss` statics, so the first write to the receive buffer is the kernel's. `arg` packs the
//! slot (bits 0..8), a receive limit (8..32, 0 = whole buffer) and the reply length the kernel
//! must report (32.., 0 = the whole pattern). Exit code 0, or 1 + the first wrong offset, or
//! `0x10000 | errno` when the call fails, or `0x20000` when the reported length is wrong.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicU8, Ordering::Relaxed};

use abi::IpcArgs;
use libsys::call;

const N: usize = 3 * 4096 + 123;
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;
static SEND: [AtomicU8; N] = [const { AtomicU8::new(0) }; N];
static RECV: [AtomicU8; N] = [const { AtomicU8::new(0) }; N];

fn main(arg: u64) -> u64 {
    let slot = arg & 0xff;
    let limit = match (arg >> 8) & 0xff_ffff {
        0 => N,
        n => n as usize,
    };
    let expect = match arg >> 32 {
        0 => N,
        n => n as usize,
    };

    // Same generator as bulk_srv: xorshift64.
    let mut x = SEED;
    for b in &SEND {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        b.store((x >> 24) as u8, Relaxed);
    }

    let mut args = IpcArgs {
        label: 1,
        send_buf: SEND.as_ptr() as u64,
        send_len: N as u64,
        recv_buf: RECV.as_ptr() as u64,
        recv_len: limit as u64,
        ..IpcArgs::default()
    };
    if let Err(e) = call(slot, &mut args) {
        return 0x10000 | e as u64;
    }
    if args.send_len != expect as u64 {
        return 0x20000;
    }
    // Bytes up to the limit match; the kernel writes nothing past it.
    let got = expect.min(limit);
    for (i, (s, r)) in SEND.iter().zip(&RECV).enumerate() {
        let want = if i < got { s.load(Relaxed) ^ 0x5A } else { 0 };
        if r.load(Relaxed) != want {
            return 1 + i as u64;
        }
    }
    0
}

libsys::entry!(main);
