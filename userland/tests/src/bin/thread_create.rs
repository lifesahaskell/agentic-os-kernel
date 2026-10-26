//! A holder of a Space capability starts a thread in that other space, interrupts it, and is
//! refused where it holds nothing. `recv` on slot `arg` takes `[space_slot, rip, rsp, entry]`.
//! With label 0 the program
//!
//! 1. `thread_create(space_slot, &ctx, 0x77)` with `rip` and `rsp` as given,
//! 2. `thread_interrupt(that thread, entry)`,
//! 3. `thread_create(5, &ctx, 0)` on a slot that holds nothing,
//!
//! and replies `[thread slot, tid the kernel wrote into ctx, interrupt result, step 3 result]`,
//! each a raw `rax` (a negative errno on failure), then exits 0.
//!
//! With label 1 it only calls `thread_create(space_slot, TEST_DATA, 0x77)`, where the test has
//! mapped a read-only page holding a valid `ThreadCtx`, and replies `[result, 0, 0, 0]`: the
//! kernel cannot write the tid back, so nothing may start.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::hint::black_box;

use abi::{IpcArgs, SYS_THREAD_CREATE, SYS_THREAD_INTERRUPT, TEST_DATA, ThreadCtx};
use libsys::{raw::syscall6, recv, reply};

fn create(space: u64, ctx: usize, tag: u64) -> isize {
    syscall6(
        SYS_THREAD_CREATE,
        [space as usize, ctx, tag as usize, 0, 0, 0],
    )
}

fn main(ep: u64) -> u64 {
    let mut args = IpcArgs::default();
    if recv(ep, &mut args).is_err() {
        return 1;
    }
    let [space_slot, rip, rsp, entry] = args.words;

    if args.label == 1 {
        args.words = [create(space_slot, TEST_DATA, 0x77) as u64, 0, 0, 0];
    } else {
        let mut ctx = ThreadCtx {
            rip,
            rsp,
            arg: 1,
            ..ThreadCtx::default()
        };
        let slot = create(space_slot, &mut ctx as *mut ThreadCtx as usize, 0x77);
        // The kernel wrote `tid` through the pointer; keep the compiler from folding it to 0.
        let tid = black_box(&ctx).tid;
        let interrupted = if slot >= 0 {
            syscall6(
                SYS_THREAD_INTERRUPT,
                [slot as usize, entry as usize, 0, 0, 0, 0],
            )
        } else {
            slot
        };
        let refused = create(5, &mut ctx as *mut ThreadCtx as usize, 0);
        args.words = [slot as u64, tid, interrupted as u64, refused as u64];
    }
    args.label = 0;
    args.send_len = 0;
    args.recv_len = 0;
    args.ncaps = 0;
    if reply(&mut args).is_err() {
        return 2;
    }
    0
}

libsys::entry!(main);
