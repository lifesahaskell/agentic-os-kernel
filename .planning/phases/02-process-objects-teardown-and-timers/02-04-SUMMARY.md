---
phase: 02-process-objects-teardown-and-timers
plan: 04
subsystem: kernel
tags: [capabilities, space, thread, syscall, ostd, ktest]
requires:
  - phase: 02-01
    provides: Agent budget and Thread run loop
  - phase: 02-02
    provides: capability space with O(1) insert and free list
provides:
  - Object::Space and Object::Thread capabilities, Rights CREATE and SIGNAL
  - space::thread_create (CREATE-gated) and space::thread_interrupt (SIGNAL-gated)
  - SYS_THREAD_CREATE 0x104, SYS_THREAD_INTERRUPT 0x105, ThreadCtx ABI
affects: [02-05, 02-06, procmgr, exec]
tech-stack:
  added: []
  patterns:
    - "Space caps hold Arc<Space>, Thread caps hold Weak<Thread> (OD-14)"
    - "A thread's agent is its Space owner (credential invariant)"
    - "Publish-then-spawn: the tid is written to user memory before the thread starts, and a failed write unwinds the cap and the registration"
key-files:
  created:
    - kernel/src/space.rs
    - userland/tests/src/bin/thread_create.rs
  modified:
    - kernel/src/cap.rs
    - kernel/src/thread.rs
    - kernel/src/user.rs
    - kernel/src/lib.rs
    - kernel/src/syscall.rs
    - userland/abi/src/lib.rs
key-decisions:
  - "SPACE_SELF (u64::MAX) names the caller's own space and inherits the caller's tag; a non-zero tag with SPACE_SELF is EINVAL (OD-13)"
  - "thread_create check order: start fields, tag rule, target (CREATE; WrongType; Dead), register (Limit), cap insert (NoRoom), publish tid, spawn"
  - "thread_interrupt check order: entry range (Invalid), resolve(SIGNAL) (Rights/NoSlot), WrongType, Dead"
status: complete
requirements-completed: [KERN-04]
commits: 4
plan_head_before: 5a5a15814165886db610ad0667248439c041ef87
plan_head_after: 9aeb1219b16a05e591ffc393e0b269568563e501
actuals:
  tokens: 13000
  tasks: 3
  commits: 4
duration: multi-session (long; dominated by 10-minute TCG ktest runs and one hang diagnosis)
completed: 2026-10-26
---

# Phase 2 Plan 04: Space and Thread capabilities, thread_create, thread_interrupt Summary

The process is now a kernel object: a holder of a Space capability with CREATE starts a thread in another address space running as that space's owner agent, and a holder of the returned Thread capability with SIGNAL redirects it to a handler entry.

## What was built

- **Task 1 (tracer, c0bccce)**: `Object::Space(Arc<Space>)` and `Object::Thread(Weak<Thread>)`, rights `CREATE=8`, `SIGNAL=16` (`ALL=31`), `kernel/src/space.rs` with `Space`, `ThreadStart`, `Target`, `SpaceError`, `thread_create`, `thread_interrupt`. Supporting hooks in `thread.rs` (`set_process`, `process`, `fs_base`, `exited`/`set_exit`/`exit_status`, `spawn_thread`, `run_ctx`) and `user.rs` (`start_ctx`, testkit `join_thread`). Kernel-path ktest proves the thread runs in the other space as its owner and is interrupted to its handler.
- **Task 2 (TDD, ea3f627 RED, 1169741 GREEN)**: syscalls 0x104 and 0x105, `Errno::ESRCH`, `From<SpaceError> for Errno`, `ThreadCtx` (48 bytes, `to_bytes`/`from_bytes`, `THREAD_CTX_TID_OFFSET=40`), ring-3 fixture `thread_create.rs`. Ring-3 holder creates and interrupts a thread in another space; a read-only ctx page gives EFAULT and starts nothing.
- **Task 3 (9aeb121)**: ktests for the 64-thread limit, full cspace (EMFILE), refusals without CREATE or with a bad ctx, `thread_interrupt` refusals, credential-equals-owner, `rights_include_create_and_signal`; both syscalls added to the syscall fuzz and decode tests, `KNOWN` grows to 12 entries with ESRCH.

## Verification (all runtime evidence is QEMU TCG)

There is no `/dev/kvm` on this host. Every ktest ran under QEMU TCG, `-smp 1`. The KVM re-run is deferred to a host with `/dev/kvm`.

- Full unfiltered `./osdk.sh cargo osdk test`: **97 passed, 0 failed** (90 after Task 2, plus 7 in Task 3).
- `sh scripts/check.sh --no-kvm`: static gates passed (fmt, clippy, build). Note this mode does not compile ktests.
- Fuzz scratch offset: `mapped + 128` (the scratch address used for the `[SPACE_SELF, mapped+128]` THREAD_CREATE case, which gives EINVAL because a non-zero tag cannot go with SPACE_SELF).
- Spinner exit code: the handler exits with the interrupted pc. The kernel-path ktest asserts `Exit::Code(pc)` lies inside `img.text`; the numeric pc was not captured in the log. The ring-3 test cannot read the `Exit::Code` of a thread it started (the Weak upgrade races thread exit), so it asserts `live_threads() == 0` instead.
- FSGSBASE: OSTD enables CR4.FSGSBASE unconditionally when the CPU exposes it; the OSDK QEMU CPU `Icelake-Server` has it. `fs_base` in `ThreadCtx` is therefore applied through the normal path. Not separately exercised beyond `fs_base: 0` in the ktests.

## TDD Gate Compliance

- **Task 1** was a tracer (`type="tracer"`), committed as `feat` with its own kernel-path ktest.
- **Task 2 RED (ea3f627)**: run recorded in the scratchpad log `t2red.log`: 88 passed, 2 failed. The two failures were the ring-3 thread_create and thread_interrupt ktests returning ENOSYS from the unwired syscalls. The RED commit includes the fixture binary and ABI types as scaffolding, since the test cannot compile without them. **GREEN (1169741)**: 90 passed.
- **Task 3**: no RED failure was observed. The behaviour under test already existed from Tasks 1 and 2, so the new tests passed on first successful run (after the test-side hang fix below). No mutation check was run (a ktest cycle costs about 10 minutes under TCG). The tests are therefore regression coverage, not evidence that they would catch a regression. This is a gap.

## Deviations from Plan

**1. [Rule 1 - Bug, test] Hang in `thread_create_limit_is_64` and `space_credential_matches_owner`**
- **Found during:** Task 3
- **Issue:** ktests run on OSTD's default FIFO scheduler, which does not preempt on a tick. A spinner left alive ahead of the main ktest task in the queue keeps the CPU forever. Stopping threads one at a time (interrupt, join, next) hung as soon as a second spinner was ahead of main.
- **Fix:** `stop_all` interrupts every thread first and joins after; `space_credential_matches_owner` does the same per creator. The limit test no longer observes "room after one exit": it stops all 64, asserts `live_threads() == 0`, then creates one more thread and stops it. This tests that exits return room, not that a single exit frees exactly one slot.
- **Files modified:** `kernel/src/space.rs`
- **Commit:** 9aeb121

**2. [Plan expectation wrong] `thread_interrupt [0, 0x40_0000]` returns EPERM, not EINVAL.** Slot 0 holds a Space cap with rights that lack SIGNAL in the fuzz setup, and the rights check precedes the WrongType check. The test asserts EPERM.

**3. [Rule 3 - Blocking] Plan `<automated>` commands name the main checkout.** Ran the equivalent commands from the worktree `kernel/` directory instead.

**4. [Rule 3 - Blocking] Name filters match zero tests** with `cargo osdk test`; the full unfiltered run was used throughout.

**5. [Rule 3 - Blocking] Stale `target/osdk`** (root-owned after a killed container) had to be removed through the container: `./osdk.sh rm -rf target/osdk`.

**6. [Plan detail] `register` takes `&Thread`**, and `set_exit` ordering is reap-before-`set_exit`; a `clear_exit` was added so a failed publish can unwind.

**7. [Plan detail] Refusal slot is 5, not 2.** Slot 2 would hit the new Thread cap in the ring-3 fixture; the fixture refuses against empty slot 5 (EBADF).

**8. [Process] Task 2 RED includes the fixture bin and ABI types** as compile scaffolding.

## Authentication gates

None.

## Known Stubs

None. Scanned the changed files for empty-value, placeholder and TODO patterns; nothing flows to a UI or leaves a goal unmet.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: new-syscall-surface | kernel/src/syscall.rs | `thread_create` (0x104) and `thread_interrupt` (0x105) accept a user pointer (`ctx`) that the kernel both reads and writes (tid write-back); both are covered by the fuzz and the EFAULT and read-only-page ktests |
| threat_flag: new-capability-types | kernel/src/cap.rs | `Space` and `Thread` capabilities carry creation and signalling authority; mitigated by CREATE and SIGNAL rights and by the credential invariant (thread agent is Space owner) |

## Self-Check: PASSED

- Files exist: `kernel/src/space.rs`, `userland/tests/src/bin/thread_create.rs`, this SUMMARY.
- Commits are ancestors of HEAD: c0bccce, ea3f627, 1169741, 9aeb121.
- Acceptance greps: `const KNOWN: [isize; 12]` 1, `fn rights_include_create_and_signal` 1, `SYS_THREAD_INTERRUPT` 8 in syscall.rs, no `DIAG` left in `kernel/src`.
