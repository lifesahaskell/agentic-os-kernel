---
phase: 02-process-objects-teardown-and-timers
plan: 01
subsystem: kernel-scheduler
tags: [scheduler, idle, halt_cpu, ktest, version-pin, KERN-07]
requires:
  - phase: 01
    provides: BudgetScheduler, Thread::block_until with deadline timer, boot checks in kernel/src/lib.rs
provides:
  - idle task in BudgetScheduler that halts the CPU when nothing can run
  - boot-time idle halt check (cpu_budget_check runs it first)
  - four sched_* ktests pinning the update_current / pick / enqueue protocol
  - exact OSTD pin and tools/pin_check.sh static gate
affects: [02-02, 02-03, later phases that block kernel tasks]
tech-stack:
  added: []
  patterns:
    - "idle lives in RunQueue.idle or in current, never in queue, so it is never counted as others"
    - "boot tasks that wait use sleep_ticks (blocked) instead of yield loops, so idle can run"
key-files:
  created: [tools/pin_check.sh]
  modified: [kernel/src/sched.rs, kernel/src/lib.rs, kernel/Cargo.toml, scripts/check.sh]
key-decisions:
  - "Idle halt check bounds: halt_cpu returns over N=50 ticks must fall in [N/2, 4N]"
  - "Idle is excluded from the others count and is requeued into its own slot, never the queue"
status: complete
actuals:
  tokens: 4500
  tasks: 3
  commits: 3
plan_head_before: 09b184a508a905325305892bb997022bd9f665e8
plan_head_after: ba8a04d3e74a4b5973835c1b12fbc9e095a5d51a
---

# Phase 2 Plan 01: Idle task, sched ktests and version pin Summary

BudgetScheduler gains an idle task that calls `ostd::task::halt_cpu`, four `sched_*` ktests pin the scheduler protocol, and OSTD plus the toolchain are pinned behind `tools/pin_check.sh`.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 1 | Idle task and boot idle-halt check (tracer) | a8d1360 |
| 2 | Four sched_* ktests | b483735 |
| 3 | Exact OSTD pin and pin_check.sh gate | ba8a04d |

## Evidence

**All runtime evidence is QEMU under TCG. This host has no /dev/kvm.** The KVM re-run of `cargo osdk test` and the boot checks is deferred to a KVM host.

- Boot (task 1): `aok: idle halts 44 over 53 ticks`, `aok: idle halt check passed`, fpu preemption and cpu budget checks pass. 44 halts over 53 ticks sits inside the [25, 200] band; a spinning idle would count millions.
- Budget line, before (Phase 1 TCG baseline, from 01-08-SUMMARY.md, not re-run by me): `33 periods: root agent 218 ticks (quota 8), child 61`.
- Budget line, after: `35 periods: root agent 231 ticks (quota 8), child 65 (quota 2)`. The upper bounds (`ticks <= quota * periods`) hold. Both agents get slightly more CPU than before because the boot task is now blocked instead of yield-spinning.
- Ktests (task 2): full `cargo osdk test` is 73 passed, 0 failed, and includes `sched_idle_is_picked_only_when_nothing_is_eligible`, `sched_update_current_truth_table`, `sched_enqueue_requests_preempt_when_idle_runs`, `sched_idle_is_never_counted_as_others`.
- Static (task 3): `sh tools/pin_check.sh` prints `pin_check: ok`; `--self-test` breaks one pin per round (six rounds over five files) and prints `pin_check: self-test ok`. `sh scripts/check.sh --no-kvm` ends `check: static gates passed; LOCAL-ONLY (needs /dev/kvm): ...`. `kernel/Cargo.lock` is unchanged.

## Deviations from Plan

**1. [Plan defect - step 0c] Absolute verify paths.** The plan's `<automated>` commands begin `cd /home/rakdos/repos/agentic-os-kernel && ...`, which is the main checkout, not this worktree. I ran the root-relative equivalents inside the worktree. The plan should use relative paths.

**2. [Plan defect - ktest filter] `cargo osdk test sched_` matches nothing.** The OSDK/ktest filter is a path-prefix match, not a substring match, so `sched_` and `aok::sched` filtered out all 73 tests (`0 passed; 73 filtered out`). The tests do run in the unfiltered `cargo osdk test`. The plan's filtered verify command should use the full path prefix of a test, or the unfiltered run.

**3. [Rule 3 - Blocking] Stale ktest bundle.** OSDK reuses the bundle in `kernel/target/osdk` unless it is cleared; an early run did not pick up my new tests. Cleared with `rm -rf target/osdk` and rebuilt. `scripts/check.sh` already does this for the boot run.

No other deviations.

## Known Stubs

None.

## Threat Flags

None.

## Self-Check: PASSED

- tools/pin_check.sh, kernel/src/sched.rs, kernel/src/lib.rs, scripts/check.sh present.
- Commits a8d1360, b483735, ba8a04d are ancestors of HEAD.
