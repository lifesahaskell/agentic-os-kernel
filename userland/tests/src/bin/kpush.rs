//! `main` returns 0x5afe. The CR-01 ktest starts this image with `rsp` in the kernel half, so
//! `_start`'s `call` faults on its return-address push before `main` runs; started normally, the
//! image exits 0x5afe, which shows the kernel survived the faults and the image itself is good.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

fn main(_: u64) -> u64 {
    0x5afe
}

libsys::entry!(main);
