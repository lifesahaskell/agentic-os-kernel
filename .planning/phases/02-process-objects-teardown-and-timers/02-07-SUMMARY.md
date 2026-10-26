---
phase: 02-process-objects-teardown-and-timers
plan: 07
subsystem: kernel
tags: [agent-retirement, staging-charge, futex, timers, leak-counters, frame-probe, ostd]
requires:
  - phase: 02-process-objects-teardown-and-timers
    provides: process teardown, Payer trait, fallible cow_clone (02-06); free-frame probe (02-11)
provides:
  - Agent retirement: a child agent retires with its last counted process and returns its carve to the parent
  - Charged IPC staging: a message's byte pages are charged to the sender before allocation and uncharged once, on drop
  - FutexError::NoMemory and an unwrap-free futex lookup
  - Teardown drops the space's armed timers
  - Leak counters (Counts, live_threads, live_reply_waits, deadline_count, armed_count, table_len) and a probe gate on both 10,000x loops
affects: [02-08, 02-09, 02-10, procmgr exec and fork]
tech-stack:
  added: []
  patterns:
    - "Life/Stage retirement state: enter_space and leave_space count spaces, the last leave retires, the last child finishes a retired agent"
    - "Staged owns an Arc<StagingCharge> whose Drop uncharges once, so a clone of a queued message cannot double-refund"
    - "Counters are the primary leak check; the frame probe is a one-sided gate with tolerance F"
key-files:
  created: []
  modified:
    - kernel/src/agent.rs
    - kernel/src/cap.rs
    - kernel/src/futex.rs
    - kernel/src/ipc.rs
    - kernel/src/ipc/timer.rs
    - kernel/src/space.rs
    - kernel/src/syscall.rs
    - kernel/src/thread.rs
    - kernel/src/user.rs
key-decisions:
  - "Retirement is driven by the count of non-adopted spaces; adopted spaces do not count"
  - "Staging pages are charged before allocation; a refusal is ENOMEM and zero bytes charge nothing"
  - "Payer is Send + Sync so a charge can sit inside a queued message"
  - "Timer ownership is the arming thread's space pointer; teardown removes by pointer and drops entries after the lock is released"
  - "Probe tolerance F is measured per process (688128 bytes), not guessed; the probe ran on the counter path with exact equality"
requirements-completed: [KERN-08]
status: complete
duration: 2h
completed: 2026-10-26
commits: 4
plan_head_before: baf14c69d565fca9d54854f8091d9efff2c20c21
plan_head_after: 506c5582ecccf376090e0170362790dd82733973
actuals:
  tokens: 18300
  tasks: 3
  commits: 4
---

# Phase 2 Plan 07: Agent retirement, charged IPC staging and probe-gated teardown loops Summary

A child agent now retires when its last counted process goes and hands its carve back to its parent, IPC staging pages are charged to the sender and refunded exactly once, a dying space drops its armed timers, and both 10,000x loops assert flat kernel counters plus a free-frame probe on the counter path.

## Tasks

| Task | Name | Commits |
|------|------|---------|
| 1 | Agent retirement and carve return | e859de3 |
| 2 | Charged IPC staging and futex copy-on-write mapping (TDD) | RED d636d7d, GREEN d7ec14d |
| 3 | Timer drop at teardown, counters, probe-gated loops | 506c558 |

### Task 1 (e859de3)
`Life` and `Stage` state on `Agent`; `enter_space`, `leave_space`, `retire`, `finish_if_childless`, `return_carve`, `child_gone`. `CSpace::{live, drain, close}` break a capability-space cycle at retirement. `AgentError::Retired` maps to ESRCH. `Space.counted` records whether a space counts toward its agent; `Space::build` is the single funnel for every constructor. Eleven ktests: `retire_keeps_derived_grants`, `retire_returns_carve`, `root_never_retires`, `retire_waits_for_children`, `retire_after_the_last_counted_space`, `retire_breaks_a_cspace_cycle`, `retire_refuses_new_threads`, `retire_on_drop_returns_the_carve`, `retire_adopted_spaces_do_not_count`, `retire_unrun_space_uncharges_its_image`, `retire_loop_child_agent_10000x`.

### Task 2 (d636d7d RED, d7ec14d GREEN)
`StagingCharge` with `Drop`; `Staged { bytes, _charge }` with `Deref<[u8]>`, `From<Vec<u8>>`, `Clone`, `Default`, `to_vec`. `Message.data` and `Answer.data` are `Staged`. `read_bytes(space, src, payer)` charges `len.div_ceil(PAGE_SIZE)` before allocating; refusal is ENOMEM; zero bytes charge nothing. `sys_call` and `answer` pass `t.payer()`. `FutexError::NoMemory` (errno 12) with `From<AgentError>`; `lookup` no longer unwraps (`handle_cow_fault(...)?`, cursor failure maps to Fault). The shortcut comment in the staging path is deleted.

### Task 3 (506c558)
`ipc::timer::drop_space(vm)` removes every armed timer whose owner is the dying space and drops the entries after the lock is released; `armed_count()` replaces the ktest-only `armed_len`. `Thread` gains `LIVE_THREADS`, `Drop`, `live_threads()`, `deadline_count()` (replaces `pending_deadlines`), `fpu_owner_count()`. `ReplyWait` gains `LIVE_REPLY_WAITS`, `Drop`, `live_reply_waits()`. `futex::table_len()`. `Space` gains `LIVE_SPACES`, `DROPPED_SPACES`, and `Counts` (9 fields) via `counts()`. `teardown` calls `drop_space`. Test helpers `footprint`, `assert_probe_flat`, `counts_flat`, `assert_counts_flat`, `teardown_turn`; new tests `counts_drop_the_space_armed_timers` and `frame_probe_one_process_returns_to_baseline`; both 10,000x loops gained the counter and probe gate.

## Evidence (all QEMU TCG; no /dev/kvm on this host, KVM re-run deferred)

Verified:
- Full unfiltered ktest run after `rm -rf kernel/target/osdk`: **144 passed, 0 failed** (Task 2 run: 142/0).
- Task 2: each of the 8 tests passed by exact name.
- `cargo fmt --check` exit 0; `cargo osdk clippy` clean; `python3 -I tools/unsafe_check.py` prints `unsafe_check: ok`; `bash scripts/check.sh --no-kvm` prints `check: static gates passed`; `docs/architecture/unsafe-ledger.md` is unchanged (`git diff --quiet` passes; `#![deny(unsafe_code)]` stays crate-wide).
- F, the measured per-process footprint: `frame_probe: footprint=688128` bytes.
- Retire loop: `before=2090512384 after=2090512384 drop=0 bound=688128`, `iterations=10000 jiffies=162239`.
- Teardown loop: `before=2090512384 after=2090512384 drop=0 bound=688128`, `iterations=10000 jiffies=159841`.
- `frame_probe_one_process_returns_to_baseline`: `residual=0`; settle curve delta=0 at rounds 1, 2, 4, 8 and 16.
- Probe path: the counter path (frame plus slab counters) with exact equality, not the exhaustion fallback. D-06: the probe reads allocator state, not the teardown tallies, so it is independent of the code it checks. `SETTLE_ROUNDS = 8`.
- Loop duration: about 160 s each, inside one 590 s foreground wait. The outer limit is the 600 s tool cap, since `kernel/OSDK.toml` has no timeout key. `QUICK_ITERATIONS` was not used; both loops ran the full 10,000.
- Acceptance greps for Task 3 all passed (counts 1, settled 8, fp 1, loop 1, DROPPED_SPACES 3, drop_space 1, table_len 1, thread readers 3, live_reply_waits 1, Drop for ReplyWait 1).
- No commit message carries a `Co-authored` or `Claude-session` trailer. No unexpected deletions and no untracked files after any commit.

Not verified:
- Anything on KVM. The 10,000x loops and the full suite need a re-run on a host with `/dev/kvm`.
- Mid-run kill and timer behaviour of a ring-3 spinner (the non-preempting ktest scheduler cannot host one).

## Decisions on the plan's reporting asks

- **Known gaps (recorded on the `retire` doc comment in `agent.rs`).**
  1. Pages still staged in messages a retired agent sent are counted against nobody until those messages are consumed.
  2. procmgr's exec must build the new space before the old one is torn down, or the agent retires between the two.
  3. An agent dropped while it still has children never returns its carve: a leak toward its parent, never a double spend.
  4. A retiring owner refused by `Space::from_clone` is reported as `CloneError::Budget`.
- **D-02.** No earlier-plan test needed a semantic change. The only edit to an earlier test is the `_carved` rename in `agent.rs` `child_gets_attenuated_subset_and_carved_budget`.
- **D-09 limitation.** The `Exit::Budget` shortcut, now at `kernel/src/user.rs` lines 347-349 (the plan said 279-282), still covers both a spent budget and a failed allocation. Not split here.
- **`Space::from_clone` and retirement.** A retiring owner is reported as `CloneError::Budget` (ENOMEM), the closest existing variant. A dedicated variant is a later choice.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Plan defect] The plan's ktest filters match nothing or are unreliable.**
- `retire`, `staging_`, `futex_cow_`, `frame_probe_`, `counts_` are not exact function names. Exact names or a full run were used.

**2. [Rule 3 - Blocking] The `<automated>` commands point at the main checkout.**
- They were run from this worktree's `kernel/` instead, so they test this branch.

**3. [Rule 1 - Plan defect] Stale `target/osdk`.**
- `rm -rf kernel/target/osdk` before every run that must include new tests.

**4. [Rule 1 - Plan defect] `check.sh --no-kvm` does not compile ktests.**
- `scripts/check.sh` has no exec bit, so it is run as `bash scripts/check.sh --no-kvm`. Ktest compilation is exercised only by `cargo osdk test`.

**5. [Rule 1 - Plan defect] key_links are not machine-checkable as written.**
- All patterns match except the literal `FutexError::from`: futex `lookup` converts with `?` (implicit `From`), and only the tests call `FutexError::from` explicitly.

**6. [Rule 3 - Blocking] `live_agents` is `pub`, not `pub(crate)`,** to avoid a dead-code warning.

**7. [Rule 1 - Test adjustment] `child_gets_attenuated_subset_and_carved_budget`** binds the carve as `_carved` after the signature change. Commit e859de3.

**8. [Design] `retire` is private.** Only `leave_space`, `finish_if_childless` and `Drop` reach it.

**9. [TDD] No separate RED commit for Tasks 1 and 3.** Their tests need the new API to compile, so test and implementation landed together. Task 2 has a real RED commit.

**10. [Rule 1 - Plan defect] Stale Task 2 baselines.** The plan's counts for `payer()` and the copy-out call sites no longer matched the tree; the call sites were found by reading the code.

**11. [Rule 1 - Plan defect] `bulk` sends 12,411 bytes, which is 4 pages, not the plan's 3.** `BULK_PAGES = 4`.

**12. [Test design] The staging tests run the ring-3 `bulk` image directly, not a clone.**

**13. [TDD] `futex_cow_no_memory_maps_through_from` landed in the GREEN commit,** because it needs `FutexError::NoMemory` to compile.

**14. [Rule 3 - Blocking] `Payer` gained `Send + Sync`.** `Arc<dyn Payer>` could not be task data or sit in a queued message otherwise.

**15. [Rule 3 - Blocking] `armed_len` and `pending_deadlines` were replaced** by non-ktest `armed_count` and `deadline_count`, since the counters now feed `Counts`. Callers in `ipc.rs` updated.

**16. [Test design] Two Task 3 tests avoid a running spinner.** The ktest scheduler is FIFO and does not preempt, and `probe::settled()` yields, so any runnable spinner hangs the run (two hung containers, see Issues). `footprint()` uses a `futex_waiter` clone with its test page set to 0x11 and waits until the thread `is_blocked()`. `counts_drop_the_space_armed_timers` calls `th.set_space(space.vm().clone())` instead of waiting for the thread to run, because a timer's owner is the space a thread sets on its first run.

**17. [Rule 3 - Blocking] `Staged::to_vec` clones the bytes** for `call_bulk`, which needs an owned `Vec`.

## TDD Gate Compliance

| Task | RED commit | GREEN commit | RED evidence |
|------|-----------|--------------|--------------|
| 2 | d636d7d | d7ec14d | three staging tests failed, charging absent: staging pages were never charged |

Tasks 1 and 3 have no separate RED commit (deviation 9). No REFACTOR commits.

## Key links verified (grep)

| Link | Result |
|------|--------|
| space.rs `teardown` to timer.rs `drop_space` | present, called once |
| ipc.rs `read_bytes` to agent.rs `Payer` via `t.payer()` | present at both call sites |
| futex.rs `lookup` to `handle_cow_fault` | present, `?` conversion |
| futex.rs `From<AgentError> for FutexError` | present; literal `FutexError::from` appears only in tests (deviation 5) |
| agent.rs `finish_if_childless` to `return_carve` | present |

## Threat model handling

| ID | Disposition | How |
|----|-------------|-----|
| T-02-32..35 carve, grants, cycles and thread creation at retirement | mitigated | `retire_returns_carve`, `retire_keeps_derived_grants`, `retire_breaks_a_cspace_cycle`, `retire_refuses_new_threads`, `retire_waits_for_children` |
| T-02-36, T-02-37 uncharged staging pages | mitigated | `StagingCharge` charged before allocation, uncharged once in `Drop`; staging tests |
| T-02-64, T-02-65 armed timers outliving a space | mitigated | `drop_space` at teardown; `counts_drop_the_space_armed_timers` |
| T-02-66 pages staged by a retired agent's pending messages | accepted | known gap 1 above |
| T-02-67..69 leaks across 10,000 cycles | mitigated | counters plus probe gate in both loops, flat at `drop=0` |

## Known Stubs

None.

## Threat Flags

None. No new syscall, endpoint or trust-boundary schema; the changes are kernel-internal accounting.

## Issues Encountered

- Two QEMU containers hung during Task 3 development and were left running: `64ba6de222c0` (the counts timer test) and `b97788525d3a` (the frame probe test). Each uses about 100% of one CPU. `docker stop` and `pkill` were denied in this session, so the user must stop them.
- The harness refuses compound and loop git commands and `cd ..`; every command was plain and separate.
- Evidence is TCG only.

## Self-Check: PASSED

- Commits found in history: e859de3, d636d7d, d7ec14d, 506c558.
- Modified files present: `kernel/src/agent.rs`, `cap.rs`, `futex.rs`, `ipc.rs`, `ipc/timer.rs`, `space.rs`, `syscall.rs`, `thread.rs`, `user.rs`.
