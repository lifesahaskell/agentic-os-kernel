//! All-register probe (placeholder until `regs_probe!` lands): exits 98.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

fn main(_arg: u64) -> u64 {
    98
}

libsys::entry!(main);
