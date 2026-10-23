//! B2 spike: capability-microkernel primitives on Asterinas OSTD.
//! See docs/architecture/b2-spike-report.md for scope and findings.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

pub mod agent;
pub mod cap;
pub mod ipc;
pub mod thread;
pub mod user;
pub mod vm;

use ostd::{
    power::{ExitCode, poweroff},
    prelude::*,
};

#[ostd::main]
fn kernel_main() {
    println!("aok: B2 spike kernel booted; primitives are exercised by `cargo osdk test`");
    poweroff(ExitCode::Success);
}
