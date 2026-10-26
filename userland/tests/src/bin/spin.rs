//! `jmp .`: never traps on its own, so only a timer redirect can end it. The redirect handler
//! exits with the interrupted pc.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

fn main(_: u64) -> u64 {
    loop {
        core::hint::spin_loop();
    }
}

libsys::entry!(main);
