//! Request-ledger server (AGNT-05). `arg` 0: serve the endpoint in slot 0 with a limit of 4
//! requests per tag per window, a window far longer than any test, and the ledger keyed on the
//! kernel-stamped `tag`. An admitted request is answered `words[0] = 0`, an over-limit one
//! `words[0] = -EAGAIN`. A request labelled `STOP` is answered and ends the server with exit 0.
//! `arg` 1: run the ledger against synthetic times and exit 0 only if every check holds; a failed
//! check exits with its number.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{IpcArgs, errno};
use libsys::{
    ledger::{LEDGER_CLIENTS, RequestLedger, Verdict},
    recv, reply_recv, tsc,
};

/// Ends the server. Test control only: a real server would not let a client stop it.
const STOP: u64 = 0xFFFF;

const LIMIT: u32 = 4;
/// Far longer than a test run at any plausible TSC rate.
const LONG_WINDOW: u64 = 1 << 62;

fn serve(ep: u64) -> u64 {
    let mut ledger = RequestLedger::new(LIMIT, LONG_WINDOW);
    let mut args = IpcArgs::default();
    if recv(ep, &mut args).is_err() {
        return 1;
    }
    loop {
        let started = tsc();
        let tag = args.tag;
        let stop = args.label == STOP;
        let verdict = if stop {
            Verdict::Admit
        } else {
            ledger.admit(tag, started)
        };
        args.words = [
            match verdict {
                Verdict::Admit => 0,
                Verdict::Busy => (-errno::EAGAIN) as u64,
            },
            0,
            0,
            0,
        ];
        args.send_len = 0;
        args.recv_len = 0;
        args.ncaps = 0;
        if verdict == Verdict::Admit && !stop {
            ledger.charge(tag, tsc().wrapping_sub(started));
        }
        if stop {
            // Answer, then leave: `reply_recv` would wait for another request.
            return if libsys::reply(&mut args).is_ok() {
                0
            } else {
                2
            };
        }
        if reply_recv(ep, &mut args).is_err() {
            return 3;
        }
    }
}

/// Synthetic-time checks of the ledger. Returns 0, or the number of the first failed check.
fn self_test() -> u64 {
    const W: u64 = 1000;

    // 1. L admits per tag per window; the next is Busy.
    let mut l = RequestLedger::new(LIMIT, W);
    for i in 0..LIMIT as u64 {
        if l.admit(7, i) != Verdict::Admit {
            return 1;
        }
    }
    if l.admit(7, 10) != Verdict::Busy {
        return 2;
    }
    // Another tag has its own count.
    if l.admit(8, 10) != Verdict::Admit {
        return 3;
    }

    // 2. A new window resets the count: tag 7's window began at 0, so it is over at W.
    for _ in 0..LIMIT {
        if l.admit(7, W) != Verdict::Admit {
            return 4;
        }
    }
    if l.admit(7, W + 1) != Verdict::Busy {
        return 5;
    }

    // 3. A (LEDGER_CLIENTS + 1)th distinct tag while every entry is live is Busy; known tags are
    //    still served.
    let mut l = RequestLedger::new(LIMIT, W);
    for tag in 0..LEDGER_CLIENTS as u64 {
        if l.admit(100 + tag, 5) != Verdict::Admit {
            return 6;
        }
    }
    if l.admit(999, 6) != Verdict::Busy {
        return 7;
    }
    if l.admit(100, 6) != Verdict::Admit {
        return 8;
    }

    // 4. Entries idle for a whole window are recycled: every window began at 5, so at 5 + W the
    //    new tag takes one. The recycled entry starts clean.
    if l.admit(999, 5 + W) != Verdict::Admit {
        return 9;
    }
    if l.requests(999) != Some(1) {
        return 10;
    }

    // 5. `charge` accumulates per tag and ignores a tag that is not tracked.
    l.charge(999, 30);
    l.charge(999, 12);
    l.charge(123_456, 5);
    if l.cycles(999) != Some(42) || l.cycles(123_456).is_some() {
        return 11;
    }

    // 6. A counter that goes backwards is zero elapsed time, not a new window.
    let mut l = RequestLedger::new(1, W);
    if l.admit(1, 5000) != Verdict::Admit || l.admit(1, 4000) != Verdict::Busy {
        return 12;
    }
    0
}

fn main(arg: u64) -> u64 {
    if arg == 1 { self_test() } else { serve(0) }
}

libsys::entry!(main);
