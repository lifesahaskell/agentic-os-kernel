//! Server that parks requests: it receives A, then B, answers B, then answers A while receiving C
//! in the same `reply_recv`, then answers C. `arg` is the RECV endpoint slot. A reply handle must
//! work once, so the last step reuses B's and requires EBADF. The exit code is the number of the
//! first failing step, 0 if none failed.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{IpcArgs, errno};
use libsys::{recv, reply, reply_recv};

fn main(ep: u64) -> u64 {
    let mut a = IpcArgs::default();
    if recv(ep, &mut a).is_err() || a.reply == 0 {
        return 1;
    }
    let (a_label, a_handle) = (a.label, a.reply);

    let mut b = IpcArgs::default();
    if recv(ep, &mut b).is_err() || b.reply == 0 || b.reply == a_handle {
        return 2;
    }
    let b_handle = b.reply;
    b.words = [b.label * 100, 0, 0, 0];
    if reply(&mut b).is_err() {
        return 3;
    }

    // Answers A and receives C into the block that held A's request.
    a.reply = a_handle;
    a.words = [a_label * 100, 0, 0, 0];
    if reply_recv(ep, &mut a).is_err() || a.reply == 0 {
        return 4;
    }
    a.words = [a.label * 100, 0, 0, 0];
    if reply(&mut a).is_err() {
        return 5;
    }

    b.reply = b_handle;
    if reply(&mut b) != Err(errno::EBADF) {
        return 6;
    }
    0
}

libsys::entry!(main);
