---
phase: 02-process-objects-teardown-and-timers
plan: 02
subsystem: kernel-ipc
tags: [timeouts, futex, recv, reply_recv, timer, SYS_TIMER_ARM, KERN-09]
requires: [01-05]
provides:
  - "FUTEX_WAIT with a relative timespec: ETIMEDOUT at now + ms + 1 ticks, never early; a zero timespec probes and never blocks"
  - "SYS_RECV and SYS_REPLY_RECV take a millisecond timeout in a[2] (0 = none, above 600000 = EINVAL, expiry = ETIMEDOUT)"
  - "SYS_TIMER_ARM 0x106 (ep, delay_ms, cookie): one-shot LABEL_TIMER message to an endpoint, words [cookie, due, sent, id]"
  - "LABEL_KERNEL_BASE reserved range: push refuses every label at or above u64::MAX - 15"
  - "Endpoint::push_kernel and ipc::timer (the timer task, ARMED table with owner: Option<Weak<VmSpace>> for 02-06)"
affects: [02-05, 02-06]
key-files:
  created:
    - kernel/src/ipc/timer.rs
  modified:
    - kernel/src/futex.rs
    - kernel/src/ipc.rs
    - kernel/src/syscall.rs
    - kernel/src/thread.rs
    - userland/abi/src/lib.rs
    - userland/tests/src/bin/timeout.rs
decisions:
  - "deadline = now + ms + 1 ticks for every timed wait (futex, recv, timer): the extra tick covers the elapsed part of the current tick, so a wait is never early and ends in [D+1, D+3]"
  - "A recv timeout above MAX_TIMEOUT_MS is refused at decode, before any slot lookup; a timer delay is range-checked in timer::arm, after the slot resolves"
  - "Only the receive half of reply_recv is timed; a timeout after a successful reply returns ETIMEDOUT with the reply already delivered"
  - "One timer task sleeps in block_until with the earliest due tick as its deadline: no second register_callback, nothing in IRQ context but the existing DEADLINES wake"
  - "fire_due holds ARMED for the whole pass, so an entry is always either in the table or in the endpoint queue; a Full queue re-keys it to now + 1 and keeps its original due tick"
  - "push_kernel takes no tid: a kernel-made message always has tid 0"
status: complete
commits: 6
plan_head_before: 09b184a508a905325305892bb997022bd9f665e8
plan_head_after: 194d09b89b59de52a167ec32d1fa2160f0e853b2
requirements-completed: [KERN-09]
actuals:
  tokens: 13000
  tasks: 3
  commits: 6
---

# Phase 2 Plan 02: Timeouts and timer messages Summary

Timed waits for futex and ipc `recv`/`reply_recv` on one `block_until` deadline, plus `SYS_TIMER_ARM`, a one-shot timer that makes the kernel send a `LABEL_TIMER` message to an endpoint, owned by a single kernel task so no endpoint lock or allocation runs in interrupt context.

## Evidence class: TCG only

This host has no /dev/kvm. Every ktest and every tick measurement below ran under QEMU TCG (`-smp 1`), not KVM. Tolerances were set and verified against TCG behaviour. The deferred KVM re-run (02-CONTEXT.md Deferred Ideas) must repeat the three timing families: `timeout_futex_*`, `timeout_recv_*`, `timer_msg_*`. Nothing here is KVM evidence.

## Tasks and commits

| Task | Name | Type | Commit |
|------|------|------|--------|
| 1 | Futex wait timeout from a timespec, ring 3 to ETIMEDOUT | tracer | `ffe4c1d` feat |
| 2 | Timed `recv` and `reply_recv` | tdd | `1ad3b34` test (RED), `203af36` feat (GREEN) |
| 3 | Timer that delivers a message to an endpoint | tdd | `984921d` test (RED), `9085ee8` feat (GREEN), `194d09b` refactor |

## TDD Gate Compliance

Task 1 is a tracer: one production-quality `feat` commit, verified end to end before Tasks 2 and 3 expanded on it.

Tasks 2 and 3 each have a `test(02-02)` commit before their `feat(02-02)` commit.

RED evidence. `gsd_run check tdd-red-evidence` does not parse the ktest report format, so the evidence and its semantic assessment are recorded here by hand. Every RED test compiled, ran, and failed on a behavioural assertion or on the stub's `Invalid`; none failed on a compile error or a hang.

Task 2 (stubs `recv_until` and `recv_parked_until` returned `Invalid`; decode carried an unbounded `timeout_ms`):

| Test | RED failure |
|------|-------------|
| `timeout_recv_expires_within_tolerance` | `assertion left == right failed`, left `Some(Invalid)`, right `Some(TimedOut)` |
| `timeout_recv_message_beats_deadline` | `called Result::unwrap() on an Err value: Invalid` at the first receive |
| `timeout_recv_ring3_never_early` | left `Code(4194600)`, right `Code(0)`; the 200-tick guard tripped (elapsed=200) because the deadline was ignored, and the program had to be interrupted |
| `decode_recv_timeout_is_bounded` | left `Ok(Recv { .. timeout_ms: 600001 })`, right `Err(Errno(22))` |

Task 3 (stub `timer::arm` returned `Invalid`; `push` still accepted labels other than `LABEL_CANCEL`):

| Test | RED failure |
|------|-------------|
| `timer_msg_label_is_reserved` | left `Ok(())`, right `Err(Invalid)` for label `0xfffffffffffffff0` |
| `timer_msg_delivers_to_endpoint` | `unwrap()` on `Err(Invalid)` from the stub `arm` |
| `timer_msg_limits` | same |
| `timer_msg_full_queue_retries` | same |
| `timer_msg_ring3_arm_and_recv` | left `Code(122)`, right `Code(0)` (100 + EINVAL) |

Semantic assessment: for the unwrap-style failures the test never reached its timing or content assertions, so the RED proves the feature is absent, not that the assertions are right. Those assertions were exercised by the GREEN run (below). Two targeted fuzz cases added in the Task 3 RED commit (delay 0 is EINVAL, empty slot 5 is EBADF) already hold with the stub: they guard decode and slot resolution and are not RED tests.

GREEN: all of the above pass. REFACTOR: `194d09b` only, no behaviour change.

## Verification (all TCG)

- `sh scripts/check.sh --no-kvm`: `check: static gates passed; LOCAL-ONLY (needs /dev/kvm): cargo osdk test, cargo osdk run boot checks`, no `aok` warnings (the `ermsb` rustc warning is pre-existing and unrelated).
- One unfiltered `cargo osdk test`: `test result: ok. 80 passed; 0 failed; 0 filtered out.` (75 tests at the start of this plan, 5 `timer_msg_*` added; the others were added within tasks 1 and 2).
- Named runs, each `ok`: `timeout_futex_expires_within_tolerance`, `timeout_futex_ring3_never_early`, `timeout_recv_expires_within_tolerance`, `timeout_recv_message_beats_deadline`, `timeout_recv_ring3_never_early`, `decode_recv_timeout_is_bounded`, `timer_msg_delivers_to_endpoint`, `timer_msg_ring3_arm_and_recv`, `timer_msg_limits`, `timer_msg_full_queue_retries`, `timer_msg_label_is_reserved`, `fuzz_syscalls_never_panic`.
- Acceptance greps: `pub mod timer` count in `kernel/src/ipc.rs` is 1; `register_callback` in `kernel/src/ipc/timer.rs` is 0; `LABEL_KERNEL_BASE` is used in `ipc.rs`; `SYS_TIMER_ARM` is defined once in the abi.

### Measured ticks (1 tick = 1 ms)

| What | Bound asserted | Measured |
|------|----------------|----------|
| futex wait, kernel, D=10 (after warm-up) | [11, 13] | 11 (the cold first run, before the warm-up was added: 15) |
| futex wait, ring 3, 20 ms | [20, 40] | 34-35 |
| recv and recv_parked, kernel, D=10 | [11, 13] | 11 |
| recv, ring 3, 20 ms | [20, 40] | 28 alone, 26 in the full run |
| timer message, kernel, 10 ms | sent - due in [0, 2] | 0 |
| timer message, ring 3, 20 ms | [20, 40] | 37 alone, 25 in the full run |
| timer behind a full queue | within 3 ticks of a slot freeing | 1 alone, 0 in the full run |

The ring 3 numbers carry the process spawn and first-run cost of TCG, so they sit well above the kernel-side D+1. One run measured 37 against a bound of 40: little margin. That is an observation from three runs, not a distribution. The bound exists to catch "never early" and "not absurdly late"; on KVM the spawn cost should shrink, which is an assumption until the re-run.

Fuzz (`fuzz_syscalls_never_panic`, 100,000 iterations, errnos limited to `KNOWN`): last full run `enosys=35258 efault=13700 einval=4155 ebadf=28038 eintr=76`. Targeted cases added: `SYS_RECV` with timeout 600001 is EINVAL, `SYS_TIMER_ARM` delay 0 is EINVAL, slot 5 is EBADF; `SYS_TIMER_ARM` joined the random number list; `timer::disarm_all()` runs after the loop because random arms that succeed would otherwise outlive the test.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Test filter in the plan's verify commands matches nothing**
- **Found during:** Task 1
- **Issue:** `cargo osdk test timeout_futex` and `... timer_msg` ran 0 tests: this OSDK build matches the exact bare test function name, not a substring or path.
- **Fix:** ran every test by its exact name, and ran one unfiltered `cargo osdk test` as the catch-all.
- **Files modified:** none (verification only)

**2. [Rule 1 - Bug] Cold-start tolerance miss under TCG**
- **Found during:** Task 1
- **Issue:** the first timed wait of a boot measured 15 ticks for D=10 (bound [11, 13]): the first deadline wait registers the timer callback and runs every path cold, and TCG translates each block on first use.
- **Fix:** an unmeasured warm-up wait before the measured one, in `timeout_futex_expires_within_tolerance`, `timeout_recv_expires_within_tolerance` and `timer_msg_delivers_to_endpoint` (the timer task is spawned on first arm). The measurements are in a comment next to the first one. This weakens nothing about the bound; it removes a one-time cost. Whether KVM needs the warm-up is unknown.
- **Files modified:** `kernel/src/futex.rs`, `kernel/src/ipc.rs`, `kernel/src/ipc/timer.rs`
- **Commits:** `ffe4c1d`, `203af36`, `9085ee8`

**3. [Rule 1 - Bug] `recv_parked` became dead code in the kernel build**
- **Found during:** Task 3 static gates
- **Issue:** Task 2 moved the syscalls to `recv_parked_until`, leaving `recv_parked` used only by ktests: a `dead_code` warning in the non-test build.
- **Fix:** gated it with `#[cfg(ktest)]`.
- **Commit:** `194d09b`

**4. [Rule 2 - Missing critical] Test isolation for the global timer table**
- **Found during:** Task 3
- **Issue:** `ARMED` and the timer task are global. A timer left armed would hold a `DEADLINES` registration, and `block_until_times_out` and `timeout_recv_expires_within_tolerance` assert that registry is empty. The fuzz test arms timers at random.
- **Fix:** ktest-only `timer::armed_len()` and `timer::disarm_all()`; every `timer_msg_*` test drains its timers, the ring-3 test and the fuzz test call `disarm_all()`.
- **Files modified:** `kernel/src/ipc/timer.rs`, `kernel/src/syscall.rs`

### Departures from the plan text (design, not defects)

- **`push_kernel` has no `tid` parameter.** The plan listed `(label, sender, tid, tag, words)`; a kernel-made message has tid 0 by contract, so the argument could only be wrong. Both `push` and `push_kernel` now share a private `enqueue`.
- **The timer task fires entries under the `ARMED` lock** instead of popping, pushing outside the lock, and re-inserting on `Full`. Outside the lock there is a window where a timer is neither in the table nor in the queue, which would make `armed_len()` and a future 02-06 teardown miss it. Cost: preemption is off while it fires due entries. Recorded as a `shortcut:` comment (one global lock, fine for 16 timers per agent on one CPU).
- **`timer_msg_label_is_reserved` calls `Endpoint::push` directly** instead of `call`: `send`, `call` and the syscalls all go through `push`, and a wrongly accepted `call` would block the test forever instead of failing it.
- **The plan's `scripts/check.sh` verify commands name the main checkout path.** Run from this worktree instead, so they check this worktree's tree.

## Auth gates

None.

## Known Stubs

None. `Armed.owner` is written and never read; it is the field plan 02-06 uses to drop a dying space's timers, and carries `#[allow(dead_code, reason = ...)]` naming that plan. Not a stub: nothing in this plan's goal depends on it.

## Threat Flags

| Flag | File | Description |
|------|------------|-------------|
| threat_flag: resource-retention | kernel/src/ipc/timer.rs | An armed timer holds a strong `Arc<Endpoint>` for up to 600 s, so a dead agent's endpoint outlives it until the timer fires. Bounded by 16 timers per agent (T-02-06); plan 02-06 is meant to drop entries by `owner`. |

Threat register: T-02-05 (forged timer message) is mitigated by the `LABEL_KERNEL_BASE` check in `push` and `timer_msg_label_is_reserved`; T-02-06 by the 16-per-agent and 600,000 ms limits and `timer_msg_limits`; T-02-07 by the single timer task and the absence of a second `register_callback`; T-02-08 by saturating arithmetic and `timespec_ms` range checks; T-02-09 by the next-tick retry and `timer_msg_full_queue_retries`.

## Needs the deferred KVM re-run

- All tick measurements and bounds in the table above.
- Whether the warm-up steps are still needed, and whether the ring 3 upper bound of 40 can be tightened.
- The unfiltered `cargo osdk test` (80 tests) and `scripts/check.sh` without `--no-kvm`.

## Self-Check: PASSED

- FOUND: `kernel/src/ipc/timer.rs`
- FOUND commits: `ffe4c1d`, `1ad3b34`, `203af36`, `984921d`, `9085ee8`, `194d09b` (each an ancestor of HEAD)
- No file deletions in the final task commit.
- `commits: 6` is measured from the ledger: `git rev-list --count 09b184a..HEAD` at `194d09b`.
