//! All-register redirect probe. `_start` loads every general register and xmm0-xmm15 with known
//! values and spins; the kernel interrupts it, the handler scrambles every register and returns
//! through `SYS_SIGRETURN`, and the exit code is `libsys::regs::check()`: 0 when all registers
//! and the 4 KiB of stack below the interrupted `rsp` came back intact (see `libsys/src/regs.rs`
//! for the other codes).

#![no_std]
#![no_main]
#![deny(unsafe_code)]

libsys::regs_probe!();
