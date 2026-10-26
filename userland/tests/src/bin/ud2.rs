//! Executes `ud2` (invalid opcode, vector 6).

#![no_std]
#![no_main]
#![deny(unsafe_code)]

fn main(_: u64) -> u64 {
    libsys::trap::ud2()
}

libsys::entry!(main);
