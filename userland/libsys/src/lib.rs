//! Ring-3 runtime: the syscall stub, process entry and panic handler. The only ring-3 crate
//! with inline asm, confined to `raw.rs`, `rt.rs` and `fpu.rs` (ledger rows U-01, U-02, U-03).

#![no_std]
#![deny(unsafe_code)]

pub mod fpu;
pub mod raw;
pub mod rt;

use abi::{IpcArgs, SYS_CALL, SYS_EXIT, SYS_RECV, SYS_REPLY, SYS_REPLY_RECV, SYS_SCHED_YIELD};

pub fn exit(code: u64) -> ! {
    raw::syscall6(SYS_EXIT, [code as usize, 0, 0, 0, 0, 0]);
    loop {
        core::hint::spin_loop();
    }
}

pub fn sched_yield() {
    raw::syscall6(SYS_SCHED_YIELD, [0; 6]);
}

/// Sends `args` to the endpoint in slot `ep` and waits for the reply, which overwrites
/// `args.words`. `Err` carries the positive errno.
pub fn call(ep: u64, args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(
        SYS_CALL,
        [ep as usize, args as *mut IpcArgs as usize, 0, 0, 0, 0],
    );
    if r < 0 { Err(-r) } else { Ok(()) }
}

/// Waits for a message on the endpoint in slot `ep`. On success `args` holds its `label`,
/// `words`, `tid` and `tag`, and `reply` holds the handle to answer a `call` (0 for a `send`).
pub fn recv(ep: u64, args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(
        SYS_RECV,
        [ep as usize, args as *mut IpcArgs as usize, 0, 0, 0, 0],
    );
    if r < 0 { Err(-r) } else { Ok(()) }
}

/// Answers the call whose handle is `args.reply` with `args.words`. A handle works once.
pub fn reply(args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(SYS_REPLY, [args as *mut IpcArgs as usize, 0, 0, 0, 0, 0]);
    if r < 0 { Err(-r) } else { Ok(()) }
}

/// `reply(args)`, then `recv(ep, args)`. A failed reply returns without receiving.
pub fn reply_recv(ep: u64, args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(
        SYS_REPLY_RECV,
        [ep as usize, args as *mut IpcArgs as usize, 0, 0, 0, 0],
    );
    if r < 0 { Err(-r) } else { Ok(()) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(255)
}
