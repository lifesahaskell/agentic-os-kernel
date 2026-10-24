//! One `call` on the endpoint in slot `arg`, then exit 0. Two clones of this image run as sibling
//! threads of one agent; the server tells them apart by the tid and tag the kernel stamps.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use libsys::call;

fn main(ep: u64) -> u64 {
    let mut args = abi::IpcArgs::default();
    if call(ep, &mut args).is_err() { 1 } else { 0 }
}

libsys::entry!(main);
