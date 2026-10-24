//! Receives capabilities and passes them on. `arg` low byte is the RECV endpoint slot; the endpoint
//! it forwards to is in the next slot (SEND). Rights bits are SEND 1, RECV 2, GRANT 4.
//!
//! Chain mode: (1) the first message carries one capability with SEND|GRANT; (2) forward it with
//! mask 7; (3) receiving on it fails with EPERM, it has no RECV; (4) the second message carries
//! one capability with SEND only; (5) forwarding it fails with EPERM, it has no GRANT; (6) a call
//! naming five capabilities fails with EINVAL. The exit code is the first failing step, 0 if none.
//!
//! Drain mode (bit 8 of `arg`): receives 25 messages of four capabilities, each with SEND only,
//! and exits 0 if all arrived and slots above 63 were used, else 1 or 2.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{IpcArgs, errno};
use libsys::{call, recv};

fn chain(r: u64) -> u64 {
    let fwd = r + 1;
    let mut a = IpcArgs::default();
    if recv(r, &mut a).is_err() || a.ncaps != 1 || a.cap_rights[0] != 5 {
        return 1;
    }
    let held = a.caps[0];

    let mut f = IpcArgs {
        label: 1,
        ncaps: 1,
        ..IpcArgs::default()
    };
    f.caps[0] = held;
    f.cap_rights[0] = 7;
    if call(fwd, &mut f).is_err() {
        return 2;
    }
    if recv(held, &mut IpcArgs::default()) != Err(errno::EPERM) {
        return 3;
    }

    let mut b = IpcArgs::default();
    if recv(r, &mut b).is_err() || b.ncaps != 1 || b.cap_rights[0] != 1 {
        return 4;
    }
    // The reply to step 2 wrote its own (empty) capability list over `f`.
    f.ncaps = 1;
    f.caps[0] = b.caps[0];
    f.cap_rights[0] = 7;
    if call(fwd, &mut f) != Err(errno::EPERM) {
        return 5;
    }

    f.ncaps = 5;
    if call(fwd, &mut f) != Err(errno::EINVAL) {
        return 6;
    }
    0
}

fn drain(r: u64) -> u64 {
    let mut a = IpcArgs::default();
    let mut top = 0;
    for _ in 0..25 {
        if recv(r, &mut a).is_err() || a.ncaps != 4 || a.cap_rights != [1; 4] {
            return 1;
        }
        top = top.max(a.caps.into_iter().max().unwrap());
    }
    if top >= 64 { 0 } else { 2 }
}

fn main(arg: u64) -> u64 {
    if arg & 0x100 != 0 {
        drain(arg & 0xff)
    } else {
        chain(arg & 0xff)
    }
}

libsys::entry!(main);
