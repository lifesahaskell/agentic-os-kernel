//! Null-IPC server: `recv` once on the endpoint in slot `arg`, then `reply_recv` forever with zero
//! words, no data and no capabilities. Ends when the harness interrupts it (the redirect handler
//! exits with the interrupted pc).

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::IpcArgs;
use libsys::{recv, reply_recv};

fn main(ep: u64) -> u64 {
    let mut args = IpcArgs::default();
    if recv(ep, &mut args).is_err() {
        return 1;
    }
    loop {
        // `recv` left the request in `args`: keep the reply handle, clear everything else.
        args.words = [0; 4];
        args.send_len = 0;
        args.recv_len = 0;
        args.ncaps = 0;
        if reply_recv(ep, &mut args).is_err() {
            return 2;
        }
    }
}

libsys::entry!(main);
