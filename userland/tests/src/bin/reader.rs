//! Stream reader for the Cancel-then-wait tests. It calls the endpoint in slot `arg` with label 1
//! and a receive buffer at the next free byte, 8 bytes at a time, until the server answers with
//! no bytes. The test interrupts it while it waits; the redirect lands in `handler`, which first
//! counts the reply the interrupted call wrote (nothing, if the call never started) and then
//! carries on with the same loop. Exit 0 if byte i of the stream is `i * 7 % 251`, else 1; a
//! failed call exits `0x100 | errno`.
//!
//! The `IpcArgs` block is a static array of atomics so the handler can reach it; its fields sit
//! in `IpcArgs` order (label 0, send_buf 5, send_len 6, recv_buf 7, recv_len 8).

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::{AtomicU8, AtomicU64, Ordering::Relaxed};

use abi::SYS_CALL;
use libsys::raw::syscall6;

const CHUNK: u64 = 8;
const MAX: usize = 64;
const SEND_LEN: usize = 6;

static ARGS: [AtomicU64; 21] = [const { AtomicU64::new(0) }; 21];
// A full stream plus one chunk of room for the call that reads the end.
static GOT: [AtomicU8; MAX + CHUNK as usize] = [const { AtomicU8::new(0) }; MAX + CHUNK as usize];
static COUNT: AtomicU64 = AtomicU64::new(0);
static SLOT: AtomicU64 = AtomicU64::new(0);

fn main(slot: u64) -> u64 {
    SLOT.store(slot, Relaxed);
    read_all()
}

/// A redirect arrives only while the call is blocked in the kernel, so the reply length in
/// `ARGS` (zero when the call was taken back out of the queue) is counted exactly once.
fn handler(_pc: u64) -> u64 {
    COUNT.fetch_add(ARGS[SEND_LEN].swap(0, Relaxed), Relaxed);
    read_all()
}

fn read_all() -> u64 {
    loop {
        let n = COUNT.load(Relaxed);
        ARGS[0].store(1, Relaxed);
        ARGS[5].store(0, Relaxed);
        ARGS[SEND_LEN].store(0, Relaxed);
        ARGS[7].store(GOT.as_ptr() as u64 + n, Relaxed);
        ARGS[8].store(CHUNK, Relaxed);
        let r = syscall6(
            SYS_CALL,
            [
                SLOT.load(Relaxed) as usize,
                ARGS.as_ptr() as usize,
                0,
                0,
                0,
                0,
            ],
        );
        if r < 0 {
            return 0x100 | (-r) as u64;
        }
        let got = ARGS[SEND_LEN].swap(0, Relaxed);
        if got == 0 {
            break;
        }
        COUNT.fetch_add(got, Relaxed);
    }
    let n = COUNT.load(Relaxed) as usize;
    let ok = n == MAX && (0..MAX).all(|i| GOT[i].load(Relaxed) == (i * 7 % 251) as u8);
    if ok { 0 } else { 1 }
}

libsys::entry!(main, handler);
