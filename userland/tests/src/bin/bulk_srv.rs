//! Bulk server: receives one call, checks the bytes against the pattern `bulk` sends, XORs them
//! with 0x5A in place and replies with them. `arg` packs the RECV slot (bits 0..8) and a receive
//! limit (8..32, 0 = whole buffer); a limit below the message length must truncate the bytes and
//! still report the full length. Exit code 0, or 1 recv failed, 2 wrong reported length, 3 reply
//! failed, `0x1000 + offset` for the first wrong byte.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicU8, Ordering::Relaxed};

use abi::IpcArgs;
use libsys::{recv, reply};

const N: usize = 3 * 4096 + 123;
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;
static BUF: [AtomicU8; N] = [const { AtomicU8::new(0) }; N];

fn main(arg: u64) -> u64 {
    let slot = arg & 0xff;
    let limit = match (arg >> 8) & 0xff_ffff {
        0 => N,
        n => n as usize,
    };

    let mut args = IpcArgs {
        recv_buf: BUF.as_ptr() as u64,
        recv_len: limit as u64,
        ..IpcArgs::default()
    };
    if recv(slot, &mut args).is_err() {
        return 1;
    }
    if args.send_len != N as u64 {
        return 2;
    }
    // Same generator as bulk: xorshift64.
    let mut x = SEED;
    for (i, b) in BUF.iter().enumerate() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        if i >= limit {
            break;
        }
        if b.load(Relaxed) != (x >> 24) as u8 {
            return 0x1000 + i as u64;
        }
        b.store(b.load(Relaxed) ^ 0x5A, Relaxed);
    }

    args.send_buf = BUF.as_ptr() as u64;
    args.send_len = limit as u64;
    args.recv_len = 0;
    if reply(&mut args).is_err() {
        return 3;
    }
    0
}

libsys::entry!(main);
