# ADR 0003: Server CPU is exempt from the client's kernel budget, bounded by a request ledger

- Status: **Proposed** (option (a) chosen by Corey 2026-10-26 at the Phase 2 plan 03 checkpoint; to be Accepted after Phase 4 servers adopt it)
- Date: 2026-10-26
- Requirement: AGNT-05 (Phase 2, success criterion 5)
- Open decision: OD-05 in `.planning/phases/02-process-objects-teardown-and-timers/02-OPEN-DECISIONS.md`
- Related: [ADR 0002](0002-rust-microkernel.md)

## Context

The kernel charges CPU to the agent whose thread is running: the scheduler's tick handler calls
`charge_tick` on the running thread's agent (`kernel/src/sched.rs`, `RunQueue::update`). Servers
(procmgr, vfs, exec) run on their own agents. When a client `call`s a server, the server's thread
runs, so the server's agent pays; the client's agent pays nothing for the work it asked for.
*(interpretation: this follows from the charging rule above; no test measures it yet.)*

Without a bound, one client can send requests as fast as it can and make a server spend its whole
budget on that client, while the client's own kernel CPU budget stays untouched.

## Decision

**Option (a): exempt, with a per-client request ledger.** Work a server does for a client is
not charged to the client's kernel CPU budget. Each server bounds its clients itself, with a
ledger from `libsys::ledger`. No kernel change.

## The ledger

- **Key:** the tag the kernel stamped on the received message (`IpcArgs.tag`), never a word of
  the message body, which the client chooses. Tags are chosen only by a holder of a Space
  capability with the CREATE right, and a thread created with `SPACE_SELF` inherits its creator's
  tag (OD-13, built in plan 02-04, not yet in the tree). Once it lands, a client process cannot
  multiply its own ledger entries. Until then the kernel's tag assignment is `Thread::new_tagged`
  only, which no ring-3 caller reaches.
- **Record per key:** the start of the current window, requests admitted in it, and server cycles
  charged (`charge(tag, cycles)`), so the ledger reports requests and CPU.
- **Limit:** at most `L` requests per tag per window of `W` cycles. Each server sets `L` and `W`
  and records them in its own documentation. `libsys::ledger` defaults, which are policy and not
  measured: `DEFAULT_LIMIT = 128` requests and `DEFAULT_WINDOW_CYCLES = 2^32` cycles (about one
  second at 4 GHz).
- **Table:** `LEDGER_CLIENTS = 64` tags tracked at once. An entry idle for a whole window is
  recycled.
- **Over the limit, or table full:** the server answers `EAGAIN` (`words[0] = -EAGAIN`). A known
  tag over its limit is refused; a new tag is refused while every entry is live in its window.
  `EAGAIN` is the standard errno for "try again later", so POSIX semantics are not changed.
- **Time:** `admit(tag, now)` takes `now` as an argument. The test server passes the ring-3 TSC
  read (`libsys::tsc`, ledger row U-05).

Evidence: ktest `request_ledger_limit_answers_eagain_per_tag` (a ring-3 server, limit 4: the fifth
request of tag 0xA gets `EAGAIN`, all four of tag 0xB are served) and `request_ledger_self_test`
(window reset, table full at 64 tags, recycling, backwards counter), run under QEMU TCG.

## Consequences

**Kernel CPU budgets do not cover server-side work done for an agent.** A kernel CPU budget
bounds the CPU of an agent's own threads. Work a server does on an agent's behalf is bounded by
the server's own budget and by the server's ledger, not by the client's budget.

- A client can make a server spend up to `L` requests' worth of CPU per window at no cost to its
  own budget.
- A server that does not use the ledger has no bound on any one client other than its own budget.
- Phase 4 procmgr and vfs adopt the ledger and record their `L` and `W`.
- A request's cost varies: `L` bounds requests, not cycles. The cycles the ledger records are
  reported, not enforced.
- T-02-12 accepted: 64 clients with live windows lock out a 65th until an entry expires.
- PROJECT.md states the same limit in its core value section and in its CPU-budget line.

## Alternative considered

**Option (b): lend the client's budget through a reply-handle credit.** The kernel would charge the
server's CPU for a request to the client's agent, through a credit carried on the reply handle.
That would let kernel CPU limits cover server-side work, a stronger core-value claim. Not now:

- It changes the scheduler's charging path (`sched.rs`, `agent.rs`) and the reply ABI, which
  collide with the Phase 2 plans that already change those files.
- There is no design yet for charging a server that is preempted in the middle of a request, or
  for a server that serves many clients in one `reply_recv` loop.
- The scheduler samples at the tick (`agent.rs`, "sampled at the tick"), so per-request charging
  needs sub-tick accounting first.

Option (b) stays open: nothing here prevents adding the credit later, and the ledger would remain
as the per-client policy on top. *(interpretation)*

## What would show this wrong

- A Phase 4 server is starved by one client that stays inside the ledger limit (limit too high,
  or requests too uneven in cost for a request count to bound).
- A POSIX test needs CPU-time accounting that includes server work (for example `getrusage` or
  `times` on a process whose work is mostly in vfs).
- The 64-entry table proves too small for a real workload, so honest clients get `EAGAIN`.

## Not verified

- The defaults `L = 128` and `W = 2^32` cycles are not derived from a measurement.
- All runtime evidence so far is QEMU TCG (no `/dev/kvm` on the development host). TSC rate and
  request cost under KVM are untested.
