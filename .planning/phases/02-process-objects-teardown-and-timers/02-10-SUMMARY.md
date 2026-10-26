---
phase: 02-process-objects-teardown-and-timers
plan: 10
subsystem: kernel-thread-lifecycle
tags: [arch_prctl, fs-base, exit-unmap, clear_tid, futex, thread-exit, ostd]

requires:
  - phase: 02-process-objects-teardown-and-timers
    provides: "ThreadCtx with a clear_tid field and Thread::set_fs_base (02-04), Space teardown and the mm lock (02-06 to 02-08), self-memory syscalls and munmap (02-09), process lookup from a thread (02-11)"
provides:
  - "arch_prctl (158) ARCH_SET_FS and ARCH_GET_FS on the caller's own FS base, kept across context switches"
  - "SYS_EXIT_UNMAP (0x109): unmap a page range of the caller's own space and exit the thread in one call, so a detached thread frees its own stack"
  - "The clear_tid word a thread was created with is zeroed and futex-woken once at its exit, whatever the exit kind; unmapped, read-only, in-range-of-exit_unmap and dying-space cases are skipped"
  - "thread_create accepts a 0 or 4-aligned user clear_tid (previously any non-zero value was EINVAL)"
affects: [02-12, LIBC-04, libaok, musl-patch-series, procmgr]

actuals:
  tokens: 8666
  tasks: 3
  commits: 5

commits: 5
plan_head_before: 6914e8c17c7fc6351f23b29a46a4afe17fd85773
plan_head_after: eb3d219249462e973cabaf1f529545c41467ce3c

tech-stack:
  added: []
  patterns:
    - "The exit-time clear runs first at both thread exit points in user.rs, before set_exit, note_exit and reap, under the space's mm lock so it cannot race teardown"
    - "Thread::take_clear_tid swaps the address to 0, so the clear runs at most once per thread"
    - "A skip is decided where the knowledge is: ExitUnmap drops the word if it lies in the range it is about to unmap"

key-files:
  created:
    - userland/tests/src/bin/lifecycle.rs
  modified:
    - userland/abi/src/lib.rs
    - kernel/src/syscall.rs
    - kernel/src/thread.rs
    - kernel/src/space.rs
    - kernel/src/user.rs

key-decisions:
  - "The GS codes (ARCH_SET_GS, ARCH_GET_GS) return EINVAL although Linux accepts them (OD-03); nothing in musl or BusyBox calls them"
  - "ARCH_SET_FS with a base at or above USER_TOP is EPERM, decided in decode"
  - "ExitUnmap clears the thread's clear_tid (set_clear_tid(0)) when the word lies in [addr, addr + page_up(len)), so the exit never writes into pages the call just freed"
  - "Space::clear_child_tid takes the mm lock, checks is_dying under it, and ignores every error: the thread is exiting either way"

requirements-completed: [KERN-04]

coverage:
  - id: D1
    description: "ARCH_SET_FS / ARCH_GET_FS round trip survives 100 context switches; bad codes, a USER_TOP base and bad GET pointers return the documented errno"
    requirement: KERN-04
    verification:
      - kind: integration
        ref: "kernel/src/space.rs#lifecycle_arch_prctl_round_trip, kernel/src/syscall.rs#decode_arch_prctl"
        status: pass
    human_judgment: false
  - id: D2
    description: "A thread unmaps its own stack and exits in one call; the pages are back in the owner's budget before a sibling sees the stack gone; bad ranges are EINVAL with the thread still running"
    requirement: KERN-04
    verification:
      - kind: integration
        ref: "kernel/src/space.rs#lifecycle_exit_unmap_frees_the_stack, lifecycle_exit_unmap_rejects_bad_ranges, lifecycle_exit_unmap_without_a_process_just_exits; kernel/src/syscall.rs#decode_exit_unmap"
        status: pass
    human_judgment: false
  - id: D3
    description: "The clear_tid word is zeroed and a futex waiter on it is woken (not timed out) when the thread exits"
    requirement: KERN-04
    verification:
      - kind: integration
        ref: "kernel/src/space.rs#lifecycle_clear_tid_zeroed_and_woken_at_exit"
        status: pass
    human_judgment: false
  - id: D4
    description: "A clear_tid that is unmapped, inside the exit_unmap range, or in a dying space is skipped without panic or charge"
    requirement: KERN-04
    verification:
      - kind: integration
        ref: "kernel/src/space.rs#lifecycle_clear_tid_bad_pointer_is_ignored_at_exit, lifecycle_clear_tid_skipped_inside_exit_unmap_range, lifecycle_clear_tid_skipped_on_a_dying_space"
        status: pass
    human_judgment: false
  - id: D5
    description: "thread_create takes a 0 or 4-aligned user clear_tid and refuses unaligned, page-0, kernel-half and USER_TOP-straddling values; the syscall fuzz covers both new numbers"
    requirement: KERN-04
    verification:
      - kind: unit
        ref: "kernel/src/space.rs#thread_create_refuses_bad_ctx; kernel/src/syscall.rs#fuzz_syscalls_never_panic"
        status: pass
    human_judgment: false

duration: "about 9 min from the first task commit (10:55) to the last (11:04), plus the full-suite run; setup before the first commit and the suite wait were not timed"
completed: 2026-10-26
status: complete
---

# Phase 2 Plan 10: Thread Lifecycle Syscalls Summary

**arch_prctl FS base, a one-call unmap-and-exit, and a clear_tid word that is zeroed and futex-woken at every thread exit, completing the thread_create(space, ctx, tag) contract that 02-04 left half open.**

## Performance

- **Duration:** about 9 min between first and last task commit, plus the full run (setup and waiting not timed)
- **Tasks:** 3 of 3
- **Files created:** 1, **modified:** 5

## Accomplishments

- Task 1 (tracer, `ea5d9b6`): `abi` numbers `SYS_ARCH_PRCTL` 158, `ARCH_SET_FS` 0x1002, `ARCH_GET_FS` 0x1003, `SYS_EXIT_UNMAP` 0x109; decode and dispatch for `arch_prctl`; the `lifecycle` ring-3 bin with modes 0 and 1 (FS base across 100 yields plus four error checks). `lifecycle_arch_prctl_round_trip` passed end to end before any expansion.
- Task 2 (`3fcad69` RED, `439ed9d` GREEN): `SYS_EXIT_UNMAP` decode (EINVAL for an unaligned address, a zero length or a range leaving user space, all in decode so the thread keeps running) and dispatch (munmap on the caller's process, then `Outcome::Exit(code)`); bin modes 2 and 3.
- Task 3 (`244c886` RED, `eb3d219` GREEN): `Thread::clear_tid`, `set_clear_tid`, `take_clear_tid`; `thread_create` accepts 0 or a 4-aligned user word in `[PAGE_SIZE, USER_TOP - 4]` and stores it; `Space::clear_child_tid`; calls at both exit points in `user.rs`; the ExitUnmap skip; bin mode 4; four `lifecycle_clear_tid_*` ktests and the extended `thread_create_refuses_bad_ctx`.

## Required records

**GS codes: EINVAL deviation (OD-03).** Linux accepts `ARCH_SET_GS` (0x1001) and `ARCH_GET_GS` (0x1004). This kernel returns `-EINVAL` for them, and for every code other than `ARCH_SET_FS` and `ARCH_GET_FS`. Nothing in musl or BusyBox calls them. `ARCH_SET_FS` with a base at or above `USER_TOP` is `-EPERM`, an amendment to OD-03's table made in this plan; `ARCH_GET_FS` through an unmapped or kernel-half pointer is `-EFAULT`. Checked by bin mode 1 steps 2 to 5.

**ExitUnmap clear_tid skip.** When a thread's `clear_tid` word lies in `[addr, addr + page_up(len))`, the `ExitUnmap` dispatch sets the thread's `clear_tid` to 0 before calling `munmap`, so the exit-time clear never writes into pages the call is about to free (or into whatever is mapped there next). The arithmetic saturates and uses `checked_next_multiple_of`, so a constructed `ExitUnmap` with a huge length cannot panic. The skip is a `Thread` field reset, not a flag on the space. `lifecycle_clear_tid_skipped_inside_exit_unmap_range` checks four cases: inside, one page past the end (kept), the last aligned word of the range (skipped) and the word just before it (kept).

**Did mode 4's wait return 0 on the first run?** Yes. `lifecycle_clear_tid_zeroed_and_woken_at_exit` passed on the first GREEN run with the bin exiting `Code(0)`, which requires the `FUTEX_WAIT` to have returned exactly 0 and the word to read 0. The RED run of the same test returned `Code(3)` (the wait timed out after 1 s), so the test distinguishes a wake from a timeout. The bin also separates `-EAGAIN` (the child cleared the word before the wait began, exit 4) from a real wake; that race did not occur under the FIFO ktest scheduler, where main runs to its wait before the child's 50 yields finish. It is scheduler-dependent, so the KVM re-run could in principle see exit 4.

## Verification

All runtime evidence here is QEMU TCG. There is no `/dev/kvm` on this host, so nothing was run with KVM.

- Full unfiltered `cargo osdk test` (after `rm -rf kernel/target/osdk`): `test result: ok. 181 passed; 0 failed; 0 filtered out.` Log: `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/551cda72-80af-415e-95df-8a64bb7a2418/scratchpad/full.log`. The log names, all passing: the eight `lifecycle_*` tests (`lifecycle_arch_prctl_round_trip`, `lifecycle_exit_unmap_frees_the_stack`, `lifecycle_exit_unmap_rejects_bad_ranges`, `lifecycle_exit_unmap_without_a_process_just_exits`, `lifecycle_clear_tid_zeroed_and_woken_at_exit`, `lifecycle_clear_tid_bad_pointer_is_ignored_at_exit`, `lifecycle_clear_tid_skipped_inside_exit_unmap_range`, `lifecycle_clear_tid_skipped_on_a_dying_space`), `thread_create_refuses_bad_ctx`, `fuzz_syscalls_never_panic` (100,000 cases), `teardown_loop_same_agent_10000x`. No `FAILED` line.
- `bash scripts/check.sh --no-kvm` ends `check: static gates passed; LOCAL-ONLY (needs /dev/kvm): cargo osdk test, cargo osdk run boot checks`. This script runs static gates only (it does not compile `cfg(ktest)` code); the ktests are the runtime evidence.
- Mutation check on the dying-space guard: with the `is_dying()` early return in `clear_child_tid` removed, `lifecycle_clear_tid_skipped_on_a_dying_space` failed (`left: 0, right: 1`: the word was zeroed). The guard was restored before the GREEN commit.
- No QEMU or Docker container is left running (`docker ps -q` is empty).

Needs the deferred KVM re-run (02-CONTEXT.md Deferred Ideas): the whole suite, in particular `lifecycle_exit_unmap_frees_the_stack` (probe loop of up to 100,000 `FUTEX_WAIT` probes with yields), `lifecycle_clear_tid_zeroed_and_woken_at_exit` (the wait-before-clear ordering and its 1 s timeout), `lifecycle_arch_prctl_round_trip` (FS base survives real context switches under hardware virtualisation), `fuzz_syscalls_never_panic` and `teardown_loop_same_agent_10000x`.

Not verified: real preemption (the ktest scheduler is FIFO and never preempts), an exit racing `Space::kill` from another CPU (single CPU here: the `mm` lock plus the `is_dying` check under it is the argument, not a test), and any musl or libaok caller (LIBC-04 is Phase 6).

## key_links (greps)

- `grep -v '^\s*//' kernel/src/user.rs | grep -c clear_child_tid` prints 2 (both exit points: `run_registered` and the `spawn_thread` task).
- `munmap` in the `Syscall::ExitUnmap` dispatch of `kernel/src/syscall.rs`: `let _ = p.munmap(addr, len);` (line 719).
- `set_clear_tid` in `thread_create` (`kernel/src/space.rs` line 604, before `user::spawn_thread`) and in the ExitUnmap dispatch (`kernel/src/syscall.rs` line 716, non-comment count 1).

## TDD Gate Compliance

- Task 1 is a tracer: committed as `feat` (`ea5d9b6`) after its tests passed end to end; the tracer gate (re-run of `<verify>`) passed before expansion.
- Task 2: RED `3fcad69` (four tests failed on their target assertions against an ENOSYS stub), GREEN `439ed9d`.
- Task 3: RED `244c886`, GREEN `eb3d219`. At RED, `lifecycle_clear_tid_zeroed_and_woken_at_exit` failed with `Code(3)` against `Code(0)` and `lifecycle_clear_tid_skipped_inside_exit_unmap_range` failed with `left: 4294971392, right: 0` (the word was not dropped). Two tests passed at RED because they assert that something does not happen, which a no-op stub satisfies by construction: `lifecycle_clear_tid_bad_pointer_is_ignored_at_exit` and `lifecycle_clear_tid_skipped_on_a_dying_space`. They guard the GREEN code from overreach. The second was proven live by the mutation check above; the first is a no-panic guard whose value is the absence of a fault in the exit path, and no mutation was run on it. `thread_create_refuses_bad_ctx` also passed at RED because the RED commit carried the `thread_create` rule (a stub was needed to compile the tests).
- No `refactor` commit.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ktest filters are exact test names**
- **Found during:** Task 1
- **Issue:** the plan's `cargo osdk test lifecycle_` filter matches 0 tests (`0 passed`); OSTD's filter takes an exact name and one test per invocation.
- **Fix:** ran each test by exact name, and the unfiltered suite for the final gate, which covers every `lifecycle_*` test.

**2. [Rule 3 - Blocking] `<automated>` commands name the main checkout**
- **Issue:** the plan's `<automated>` blocks `cd` into `/home/rakdos/repos/agentic-os-kernel`, not this worktree.
- **Fix:** ran the same commands from the worktree.

**3. [Rule 3 - Blocking] stale `target/osdk` bundle**
- **Issue:** a leftover `kernel/target/osdk` can make `cargo osdk test` run an old bundle.
- **Fix:** `rm -rf kernel/target/osdk` before each build that mattered, including the full run.

**4. [Rule 1 - Plan inaccuracy] `check.sh --no-kvm` does not compile cfg(ktest)**
- **Issue:** the plan treats it as a gate for the new ktests; it runs fmt, clippy and `cargo osdk build` only, so a ktest that does not compile passes it.
- **Fix:** the ktests were compiled and run by `cargo osdk test` by name after each step.

**5. [Rule 3 - Placement] lifecycle ktests live in `space.rs`'s test module**
- **Issue:** the plan says syscall.rs's or user.rs's test module. `wait_reaped` is `pub(super)` to `space`, and the clear_tid tests need `holder_of`, `thread_create` and `Space` internals.
- **Fix:** put the eight `lifecycle_*` ktests at the end of `space.rs`'s test module. `decode_arch_prctl`, `decode_exit_unmap` and the fuzz additions are in `syscall.rs`.

**6. [Rule 1 - Plan inaccuracy] test setups simplified**
- `lifecycle_clear_tid_skipped_inside_exit_unmap_range`: the threads have no process (`Thread::new`, no `set_process`). The skip is decided before `munmap` and does not depend on a process, so a processless thread exercises it.
- `lifecycle_clear_tid_skipped_on_a_dying_space`: the word sits in the last 8 bytes of the image's stack, not a 1-page mmap, and the check is that the word reads 1 and `space.charged()` is unchanged (not the owner's `mem_used_pages`). The space then tears down on `reap`, after which `mem_used_pages()` is 0.
- `lifecycle_clear_tid_bad_pointer_is_ignored_at_exit`: asserts `Exit::Code(0)` and the space reaped with `mem_used_pages()` 0 afterwards. The plan's "unchanged until teardown" is not asserted: `join_thread` returns after `set_exit`, before `reap`, and nothing yields in between on one CPU, so a mid-window read would assert nothing real.

**7. [Rule 1 - Plan inaccuracy] ExitUnmap length arithmetic**
- **Issue:** `len.next_multiple_of(PAGE_SIZE)` panics on overflow in debug builds, and `dispatch` can be handed a `Syscall` that did not come through `decode`.
- **Fix:** `checked_next_multiple_of(...).unwrap_or(usize::MAX)` and a saturating add. Commit `eb3d219`.

**8. [Rule 3 - Tooling] clippy `function_casts_as_integer`**
- **Fix:** `unmap_own_stack as *const () as usize` in the bin, and the same form for the mode 4 child. Found at the Task 2 gate; the fix is in the Task 2 commits.

### Not changed

- `userland/libsys` was not modified: the bin uses raw `syscall6` for `SYS_FUTEX`, `SYS_MMAP`, `SYS_THREAD_CREATE`, `SYS_ARCH_PRCTL` and `SYS_EXIT_UNMAP`, as the `mmap` bin does.

## Known Stubs

None.

## Threat Flags

None. `arch_prctl` and `SYS_EXIT_UNMAP` act on the caller's own thread and space with no capability argument. The one new kernel write to user memory (`clear_child_tid`) goes through `copy_to_user`, which breaks copy-on-write on the payer's budget; an unmapped, read-only or kernel-half address is an ignored error, and a dying space is not touched.

## Phase 2 close-out

This is the last plan of Phase 2. What the phase leaves, as far as this plan can see it:

- KERN-04 is complete: Space and Thread capabilities, `thread_create(space, ctx, tag)` with the `clear_tid` contract whole, gated by the Space capability. Marked in REQUIREMENTS.md by this plan.
- OD-03's three kernel calls exist: FS base (158), `thread_create` on `SPACE_SELF` (02-04) and exit-and-unmap (0x109). `set_tid_address` and `exit_group` stay libaok's. They are verified with musl under LIBC-04 in Phase 6, which this plan does not claim.
- Not done and owned elsewhere: the SIGXCPU leg of AGNT-02 (OD-12, SIG-03); KVM runs of everything in Phase 2 (all evidence from this wave is TCG).
- Process-wide `exit_group` behaviour (killing sibling threads at the first thread's exit) is libaok's per OD-03; the kernel's `Space::kill` is what it will call.
- `unsafe`: none added by this plan; no ledger row needed.

## Issues Encountered

- A shell guard in this environment refuses compound `git` commands and some `sh -c` forms; commands were split into plain ones, and file rewrites went through small `python3 -I` scripts. No effect on the result.

## Self-Check: PASSED

- Created and modified files exist: `userland/tests/src/bin/lifecycle.rs`, `kernel/src/space.rs`, `kernel/src/syscall.rs`, `kernel/src/thread.rs`, `kernel/src/user.rs`, `userland/abi/src/lib.rs`, this SUMMARY.
- Commits are ancestors of HEAD: `ea5d9b6`, `3fcad69`, `439ed9d`, `244c886`, `eb3d219`.
- `git rev-list --count 6914e8c17c7fc6351f23b29a46a4afe17fd85773..HEAD` is 5 (taken before the SUMMARY commit), matching `commits: 5`.
