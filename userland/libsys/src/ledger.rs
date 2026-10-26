//! Per-client request ledger for ring-3 servers (AGNT-05, ADR 0003).
//!
//! The kernel charges CPU to the agent whose thread is running, so a server's work for a client is
//! charged to the server, not the client. A server therefore bounds each client itself: it counts
//! requests per client in a fixed window and answers `EAGAIN` past a limit. This module is the
//! policy, with no clock and no allocation; the caller passes `now` (cycles, for example from
//! `libsys::tsc`), so the logic runs against synthetic times in tests.
//!
//! **The key is `IpcArgs.tag` as the kernel stamped it on the received message.** Never key on a
//! word of the message body, which a client chooses. Tags are chosen only by a holder of a Space
//! capability with the CREATE right (OD-13), so a client process cannot multiply its own entries
//! by inventing tags.
//!
//! Limits, per server: `limit` requests per tag per window of `window_cycles`. The table holds
//! `LEDGER_CLIENTS` tags. A new tag arriving while every entry is live in its window is answered
//! `Busy`; an entry idle for a whole window is recycled.

/// Entries in the table: distinct tags tracked at once.
pub const LEDGER_CLIENTS: usize = 64;

/// Default requests per tag per window. Unmeasured policy: a server that needs another value
/// passes its own to `RequestLedger::new` and records it in its docs.
pub const DEFAULT_LIMIT: u32 = 128;

/// Default window in TSC cycles: 2^32, about one second at 4 GHz. Unmeasured policy, as above.
pub const DEFAULT_WINDOW_CYCLES: u64 = 1 << 32;

/// What the server does with a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Serve it.
    Admit,
    /// Answer `EAGAIN`: the tag is over its limit, or the table is full of live tags.
    Busy,
}

#[derive(Clone, Copy)]
struct Entry {
    used: bool,
    tag: u64,
    window_start: u64,
    requests: u32,
    cycles: u64,
}

const EMPTY: Entry = Entry {
    used: false,
    tag: 0,
    window_start: 0,
    requests: 0,
    cycles: 0,
};

pub struct RequestLedger {
    limit: u32,
    window_cycles: u64,
    entries: [Entry; LEDGER_CLIENTS],
}

impl RequestLedger {
    /// `limit` requests per tag per window of `window_cycles`.
    pub const fn new(limit: u32, window_cycles: u64) -> Self {
        Self {
            limit,
            window_cycles,
            entries: [EMPTY; LEDGER_CLIENTS],
        }
    }

    /// A window that began at `start` is over at `now`. A `now` before `start` (a counter that
    /// went backwards) counts as zero elapsed time.
    fn expired(&self, start: u64, now: u64) -> bool {
        now.saturating_sub(start) >= self.window_cycles
    }

    /// Counts a request from `tag` at time `now` and says whether to serve it.
    pub fn admit(&mut self, tag: u64, now: u64) -> Verdict {
        let known = self.entries.iter().position(|e| e.used && e.tag == tag);
        let i = match known {
            Some(i) => i,
            None => {
                // A new tag takes a never-used entry, else one whose window has expired.
                let free = self.entries.iter().position(|e| !e.used).or_else(|| {
                    (0..LEDGER_CLIENTS).find(|&i| self.expired(self.entries[i].window_start, now))
                });
                let Some(i) = free else {
                    return Verdict::Busy;
                };
                self.entries[i] = Entry {
                    used: true,
                    tag,
                    window_start: now,
                    requests: 0,
                    cycles: 0,
                };
                i
            }
        };
        if self.expired(self.entries[i].window_start, now) {
            self.entries[i].window_start = now;
            self.entries[i].requests = 0;
        }
        let e = &mut self.entries[i];
        if e.requests >= self.limit {
            return Verdict::Busy;
        }
        e.requests += 1;
        Verdict::Admit
    }

    /// Adds `cycles` of server time spent on a request from `tag`. Does nothing for a tag the
    /// table no longer tracks.
    pub fn charge(&mut self, tag: u64, cycles: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.used && e.tag == tag) {
            e.cycles = e.cycles.saturating_add(cycles);
        }
    }

    /// Requests admitted for `tag` in its current window, if tracked.
    pub fn requests(&self, tag: u64) -> Option<u32> {
        self.find(tag).map(|e| e.requests)
    }

    /// Server cycles charged to `tag` since its entry was created, if tracked. The count restarts
    /// when the entry is recycled.
    pub fn cycles(&self, tag: u64) -> Option<u64> {
        self.find(tag).map(|e| e.cycles)
    }

    fn find(&self, tag: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.used && e.tag == tag)
    }
}
