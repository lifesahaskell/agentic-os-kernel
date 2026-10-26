//! Ring-3 runtime: the syscall stub, process entry and panic handler. The only ring-3 crate
//! with `unsafe`, confined to `raw.rs`, `rt.rs`, `fpu.rs`, `regs.rs` and the three small modules
//! below (ledger rows U-01 to U-07).

#![no_std]
#![deny(unsafe_code)]

pub mod fpu;
pub mod ledger;
pub mod raw;
pub mod regs;
pub mod rt;

use abi::{IpcArgs, SYS_CALL, SYS_EXIT, SYS_RECV, SYS_REPLY, SYS_REPLY_RECV, SYS_SCHED_YIELD};

/// The fixed test data page the kernel's tests map at `abi::TEST_DATA`.
pub mod testpage {
    #![allow(unsafe_code)]

    use core::sync::atomic::AtomicU32;

    /// The test page as 1024 atomic words. The harness maps it RW before the program touches it
    /// (except in tests whose point is the fault), and every access is an atomic operation.
    pub fn test_page() -> &'static [AtomicU32] {
        // Dereferenceable only once the harness has mapped the page; touching it earlier takes a
        // hardware page fault, which the pager tests exercise.
        // SAFETY: U-04 `abi::TEST_DATA` is page-aligned; 1024 `AtomicU32` fit; access is atomic.
        unsafe { core::slice::from_raw_parts(abi::TEST_DATA as *const AtomicU32, 1024) }
    }
}

pub use testpage::test_page;

/// Deliberate faults for the exception tests.
pub mod trap {
    #![allow(unsafe_code)]

    /// Executes `ud2`: an invalid-opcode exception, vector 6.
    pub fn ud2() -> ! {
        // SAFETY: U-06 `ud2` raises #UD and never returns; it reads and writes no memory and
        // touches no register the compiler relies on.
        unsafe { core::arch::asm!("ud2", options(noreturn, nomem, nostack)) }
    }
}

/// The time-stamp counter, for the null-IPC timing.
pub mod clock {
    #![allow(unsafe_code)]

    /// `rdtsc`: cycles of the invariant TSC, not serialised against neighbouring instructions.
    pub fn tsc() -> u64 {
        // SAFETY: U-05 `rdtsc` reads a counter; it has no memory or register side effects beyond
        // its output, and CR4.TSD is clear in ring 3 on this kernel (OSTD never sets it).
        unsafe { core::arch::x86_64::_rdtsc() }
    }
}

pub use clock::tsc;

pub fn exit(code: u64) -> ! {
    raw::syscall6(SYS_EXIT, [code as usize, 0, 0, 0, 0, 0]);
    loop {
        core::hint::spin_loop();
    }
}

pub fn sched_yield() {
    raw::syscall6(SYS_SCHED_YIELD, [0; 6]);
}

/// Sends `args` (with the `ncaps` capabilities named in `caps`, attenuated by `cap_rights`) to the
/// endpoint in slot `ep` and waits for the reply, which overwrites `args.words` and the capability
/// list (`caps`, `cap_rights`, `ncaps` hold the slots the reply installed). `Err` carries the
/// positive errno.
pub fn call(ep: u64, args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(
        SYS_CALL,
        [ep as usize, args as *mut IpcArgs as usize, 0, 0, 0, 0],
    );
    if r < 0 { Err(-r) } else { Ok(()) }
}

/// Waits for a message on the endpoint in slot `ep`. On success `args` holds its `label`,
/// `words`, `tid` and `tag`, `reply` holds the handle to answer a `call` (0 for a `send`), and
/// `caps`, `cap_rights`, `ncaps` hold the slots and rights of the capabilities it installed.
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
