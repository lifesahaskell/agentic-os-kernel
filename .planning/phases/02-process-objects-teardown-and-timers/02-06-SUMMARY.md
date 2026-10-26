---
phase: 02-process-objects-teardown-and-timers
plan: 06
subsystem: kernel
tags: [teardown, kill, cow-clone, payer, ledger, ipc, ostd]
requires:
  - phase: 02-process-objects-teardown-and-timers
    provides: Space and Thread objects (02-05), free-frame probe (02-11)
provides:
  - Process-level teardown that refunds the per-Space ledger at the last exit
  - Space::kill, Thread::kill/is_killed/retire, Exit::Killed, IpcError::Dead (ESRCH)
  - Payer trait; fallible cow_clone charging node frames; Space::from_clone
affects: [02-07 agent-level teardown, 02-08, procmgr fork]
tech-stack:
  added: []
  patterns:
    - "Payer trait: one charge/uncharge interface for Agent, Space and the clone Tally"
    - "Kill flag set first, then wake the published waker; cancel closure checks the flag"
key-files:
  created: []
  modified:
    - kernel/src/agent.rs
    - kernel/src/futex.rs
    - kernel/src/ipc.rs
    - kernel/src/space.rs
    - kernel/src/syscall.rs
    - kernel/src/thread.rs
    - kernel/src/user.rs
    - kernel/src/vm.rs
key-decisions:
  - "Two-level teardown (process here, agent in 02-07); image, stack and staging pages are charged; leaks are measured by kernel counters plus the frame probe (Task 0, OD-04/OD-07/OD-08, approved by the user)"
  - "Separate torn guard: kill sets dying early, so dying cannot decide who runs teardown"
  - "ReplyState::Failed(IpcError) answers callers of a dead server; deliver refuses to install caps on a settled wait"
  - "cow_clone is fallible: Range, Unsupported(va), Budget; every exit after the root charge flushes, refunds and drops the child"
requirements-completed: [KERN-08]
status: complete
duration: 54min
completed: 2026-10-26
commits: 5
plan_head_before: cc592a3b99c0bd09dd1a71791806be56375b4978
plan_head_after: bf4ff6b12304365b693d57f0662cd47c94353953
actuals:
  tokens: 12500
  tasks: 4
  commits: 5
---

# Phase 2 Plan 06: Process teardown, kill and fallible CoW clone Summary

A process now dies cleanly: the last exit (or `Space::kill`) unmaps what the space owns and refunds every page it charged, a killed thread leaves user mode and every blocking state, parked callers of a dead server get ESRCH, and `cow_clone` returns `Result` and unwinds a partial clone instead of panicking or leaking.

## Task 0 answer

Decision checkpoint OD-04 / OD-07 / OD-08 was answered **approve** by the user before execution: two-level teardown (process level here, agent level in 02-07), image / stack / staging pages charged to the owner, leaks measured by kernel counters plus the frame probe. 02-11 took the A1 counter path (`SETTLE_ROUNDS = 8`, settle calibration flat at 0 from round 1 in one scenario), so the probe assertions in Task 3 are exact, not widened.

## Tasks

| Task | Name | Commits |
|------|------|---------|
| 0 | Decision checkpoint OD-04/OD-07/OD-08 | none (human stop, approved) |
| 1 | Tracer: teardown at last exit, ledger refund, Payer | ad51d75 |
| 2 | Kill, retire, Dead replies (TDD) | RED 1021a0b, GREEN ab44737 |
| 3 | Fallible cow_clone, from_clone (TDD) | RED e6083d3, GREEN bf4ff6b |

### Task 1 (ad51d75)
`Payer` trait in `agent.rs`; `impl Payer for Space` records pages in `charged_pages`; teardown unmaps owned memory, drops the notify cap and refunds the ledger; ownership modes `new` / `adopt` / `from_image`. `vm::handle_cow_fault` and `copy_to_user` take `&dyn Payer`. `teardown_loop_same_agent_10000x` asserts flat pages and capability slots.

### Task 2 (ab44737)
`Space::kill`, `Thread::{kill, is_killed, retire}`, `block_until_committed -> Option<R>`, `Exit::Killed`, `IpcError::Dead` (ESRCH) with `ReplyWait::fail`, `note_exit` maps `Exit::Killed` to `[NOTE_KIND_KILLED, 0]`. The "strands this caller" comment is deleted: a killed reply wait now returns. Six ktests.

### Task 3 (bf4ff6b)
`CloneError { Range, Unsupported(Vaddr), Budget }`; `cow_clone(parent, range, payer) -> Result<VmSpace, CloneError>` with private `nodes_for`; `handle_cow_fault` has no unwrap; `From<CloneError> for Errno`; `Space::from_clone` with a private `Tally` payer seeding `charged_pages`. Three ktests.

## Evidence (all QEMU TCG; no /dev/kvm on this host, KVM re-run deferred)

- Full unfiltered ktest run after `rm -rf kernel/target/osdk`: **123 passed, 0 failed** (Task 1: 114/0, Task 2: 120/0).
- All thirteen named tests passed in that run (six kill tests, three clone tests, `cow_clone_isolates_writes_and_charges_the_writer`, `teardown_loop_same_agent_10000x`, `teardown_charges_image_and_stack_pages`, and the Task 1 tracer test).
- `teardown_loop: iterations=10000 jiffies=158156` (about 158 s) in the filtered run; 161562 jiffies in the full run. Task 1 measured 151366. Pitfall 22: the outer limit was the Bash tool's `timeout` of 600000 ms (10 minutes) around `cargo osdk test`; `kernel/OSDK.toml` has no timeout key. The loop fits, so the 10,000 run stays the default and no small-N variant was needed. KVM will be faster.
- `cargo fmt --check` exit 0; `cargo osdk clippy` no warning or error; `python3 -I tools/unsafe_check.py` prints `unsafe_check: ok`; `sh scripts/check.sh --no-kvm` last line starts `check: static gates passed`; `docs/architecture/unsafe-ledger.md` is unchanged (no unsafe added, `#![deny(unsafe_code)]` crate-wide).
- Protected test body hash: `e55c4c20f1a28cb2492cf6f70982ac3e210615264d509d879eefa7bb6682ccbe`, identical to the plan's value (D-07).

## Decisions on the plan's reporting asks

- **D-07 under-charge.** Unchanged and recorded in the `cow_clone` and `Space` doc comments: data frames a cow-cloned child shares stay charged to the parent and are uncharged at the parent's teardown even while the child still maps them. The error is an under-charge, never a leak. The child pays for its node frames at clone time and for each copy it makes on a write fault.
- **CloneError to errno.** Range and Unsupported map to EINVAL, Budget to ENOMEM. Asserted in `cow_clone_rejects_bad_ranges`. No syscall reaches it yet; procmgr's fork will.
- **MMIO arm.** Tested, not inspection-only: `IoMem::acquire(0x100_000_000_000..+0x1000)` (the range OSTD's own mm tests use) works in a ktest, and the clone returns `Err(Unsupported(0x40_0000))` with the payer at 0. The huge-page arm shares the `r.len() != PAGE_SIZE` check and is covered by inspection only (no huge-page mapping is constructed in a test).
- **02-11 probe path and node-charge deltas.** A1 counter path. `cow_clone_charge_matches_frames`: the clone charges exactly 10 pages (1 root + 3 + 0 + 1 + 2 + 3) and `b - settled()` is exactly `10 * PAGE_SIZE` with the child alive; after the child drops, `settled() == b`. `cow_clone_partial_failure_unwinds`: the refused clone leaves the payer at 0 and `settled() == b`.
- **Space::adopt in tests.** The 02-05 helpers that run a thread over a borrowed `Image` space (`start`, and the helpers behind `teardown_keeps_an_adopted_space_mapped`) use `Space::adopt`, because an adopted space unmaps nothing and refunds nothing; with `Space::new` a teardown would unmap an image the test still reads. Tests that own their clone use `from_clone` or `Space::new`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Verify commands use the main-checkout path.**
- The plan's `<automated>` commands `cd /home/rakdos/repos/agentic-os-kernel/kernel`. They were run from this worktree's `kernel/` instead, so they test this branch.

**2. [Rule 1 - Plan defect] `cargo osdk test teardown_` matches zero tests.**
- The ktest filter matches the exact bare function name only. Every filtered run used the exact name; the full unfiltered run is the real gate.

**3. [Rule 1 - Plan defect] Stale `target/osdk`.**
- A stale `kernel/target/osdk` hid compile changes; `rm -rf kernel/target/osdk` before each full run.

**4. [Rule 1 - Plan defect] `check.sh --no-kvm` does not compile ktests, and key_links need a manual grep.**
- The static gate passed, but ktest compilation is only exercised by `cargo osdk test`; key_links were verified by grep (below).

**5. [Rule 2 - Missing critical functionality] `Thread::payer()` and process-ledger routing of kernel-side CoW breaks.**
- Kernel copy paths (IPC `write_bytes` / `write_args`, futex `lookup`, syscall `thread_create` publish) broke copy-on-write pages on the agent directly, which would leave pages charged to the owner but absent from the process ledger and never refunded at teardown. They now charge through `&*t.payer()`.
- Files: `agent.rs`, `futex.rs`, `ipc.rs`, `syscall.rs`, `thread.rs`. Commit ad51d75.

**6. [Rule 1 - Bug] Adopted space refunded pages it did not charge.**
- An adopted space (memory kept by the caller) refunded the ledger at teardown while the frames stayed mapped. Fixed: adopted spaces refund nothing, documented on `Space::adopt`. Commit ad51d75.

**7. [Rule 3 - Blocking] `unregister` on the `thread_create` rollback paths** so a failed create does not leave a registered tombstone thread blocking teardown. Commit ad51d75.

**8. [Rule 1 - Test adjustment] `thread_create_limit_is_64` and `space_notify_ring3_self_sets_once` adjusted** for the new teardown behaviour (a space whose threads all exited now tears down, so the tests keep a holder thread or adopt the space). Commit ad51d75.

**9. [Rule 1 - Bug] Separate `torn` guard in `Space`.**
- `kill` sets `dying` before the threads are gone, so `dying` can no longer decide who runs teardown. `torn` is swapped by whoever runs it; `reap`, `kill` and `Drop` all use it. Commit ab44737.

**10. [Rule 2 - Missing critical functionality] `ReplyState::Failed(IpcError)` and settled-wait handling.**
- `ReplyWait::fail` needs a terminal state. `deliver` now refuses to install capabilities into a settled wait and re-checks under the lock, uninstalling on a race, so a late reply to a killed caller leaks neither payload nor capabilities. `ReplyTable::drain` (retire) and a `#[cfg(ktest)]` `ReplyTable::len` accessor were added. Commit ab44737.

**11. [Test design] `recv_parked` stayed private; the kill tests take the call with `ep.recv`.**
- A "running spinner" cannot be interleaved on the FIFO, non-preempting ktest scheduler: a spinner that ever runs hangs the run. `kill_stops_a_running_spinner` therefore kills right after spawn, before the first run. It exercises the same kill flag and wake path as the interrupt tests; the mid-run case in ring 3 is not covered by a ktest.

**12. [Rule 3 - Blocking] `From<CloneError> for Errno` landed in the Task 3 RED commit** (e6083d3), not GREEN, because the RED tests must compile to fail on an assertion. The RED stub is the old walk wrapped in `Ok(...)` behind the new signature, not a `todo!()`.

## TDD Gate Compliance

| Task | RED commit | GREEN commit | RED evidence |
|------|-----------|--------------|--------------|
| 2 | 1021a0b | ab44737 | all six kill tests failed at the `Space::kill` stub |
| 3 | e6083d3 | bf4ff6b | `cow_clone_partial_failure_unwinds` left `None`, right `Some(Budget)`; `cow_clone_charge_matches_frames` left 0, right 10; `cow_clone_rejects_bad_ranges` failed (old walk asserts) |

No REFACTOR commits.

## Key links verified (grep)

| Link | Pattern | Result |
|------|---------|--------|
| space.rs `from_clone` to vm.rs `cow_clone` via `Tally` | `fn from_clone` | present in `kernel/src/space.rs` |
| space.rs `reap` to `teardown` | `fn teardown` | present |
| space.rs `teardown` to agent ledger | `uncharge_pages` | present |
| thread.rs `retire` to ipc.rs Dead | `Dead` | `ipc.rs` has `IpcError::Dead` and `retire` fails each parked reply with it |

Region gates: the `cow_clone` body has no `unwrap`, `assert`, `panic` or `expect`, and its only `?` is the root charge before the walk; `handle_cow_fault` has none either (one comment mentions "panic").

## Threat model handling

| ID | Disposition | How |
|----|-------------|-----|
| T-02-26 pages and budget not returned | mitigated | per-Space ledger refunded at teardown; `teardown_loop_same_agent_10000x` flat |
| T-02-27 thread starts in a dying space | mitigated | `dying` set and checked under the `threads` lock; `killed_space_refuses_thread_create` |
| T-02-28 killed thread stuck in reply wait | mitigated | `block_until_committed` honours kill; `kill_frees_a_thread_in_an_uninterruptible_reply_wait` |
| T-02-29 callers stranded by a dead server | mitigated | `retire` fails parked replies with `Dead`; `dead_server_answers_parked_callers_esrch` |
| T-02-30 frame reuse discloses data | mitigated | OSTD refcounts frames and zeroes on allocation (no code in this plan) |
| T-02-31 process killed unseen | mitigated | `NOTE_KIND_KILLED` from the common exit path; `kill_stops_a_running_spinner` |
| T-02-60 mid-clone charge leak | mitigated | unwind in every exit after the root charge; `cow_clone_partial_failure_unwinds` |
| T-02-61 clone panics on bad range / MMIO / huge page | mitigated | `CloneError`, validation before any charge; `cow_clone_rejects_bad_ranges` |
| T-02-62 child shares parent-charged frames | accepted | D-07 under-charge; node frames charged to the child |
| T-02-63 unwrap in `handle_cow_fault` | mitigated | query declines, re-`jump` refunds and returns `NoMemory`; region grep clean |

## Known Stubs

None.

## Threat Flags

None. `Space::kill` and `from_clone` are kernel-internal; no new syscall, endpoint or trust-boundary schema is added.

## Issues Encountered

- The worktree harness refuses compound or heredoc shell commands and `osdk.sh sh -c` inside loops; every command was run plain and separately.
- Evidence is TCG only; a KVM re-run of the full suite and the 10,000 loop is deferred.

## Self-Check: PASSED

- Commits found in history: ad51d75, 1021a0b, ab44737, e6083d3, bf4ff6b.
- Modified files present: `kernel/src/agent.rs`, `futex.rs`, `ipc.rs`, `space.rs`, `syscall.rs`, `thread.rs`, `user.rs`, `vm.rs`.
