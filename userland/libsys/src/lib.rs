//! Ring-3 runtime: the syscall stub, process entry and panic handler. The only ring-3 crate
//! with inline asm, confined to `raw.rs` and `rt.rs` (ledger rows U-01, U-02).

#![no_std]
#![deny(unsafe_code)]

pub mod raw;
pub mod rt;

use abi::{SYS_EXIT, SYS_SCHED_YIELD};

pub fn exit(code: u64) -> ! {
    raw::syscall6(SYS_EXIT, [code as usize, 0, 0, 0, 0, 0]);
    loop {}
}

pub fn sched_yield() {
    raw::syscall6(SYS_SCHED_YIELD, [0; 6]);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(255)
}
