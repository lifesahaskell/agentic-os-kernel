---
phase: 02-process-objects-teardown-and-timers
plan: 08
subsystem: kernel
tags: [sigreturn, redirect, signal-frame, fpu, red-zone, unsafe-ledger, ostd]
requires:
  - phase: 02-process-objects-teardown-and-timers
    provides: Thread capability and thread_interrupt (02-04), kill and retire (02-06), leak counters (02-07)
provides:
  - A per-thread kernel-held frame stack (depth 8) holding {UserContext, FpuContext, FS base} per redirect
  - SYS_SIGRETURN (0x108): pops a frame and restores it; EINVAL on an empty stack
  - Handler rsp moved below the red zone by a register change only; the kernel writes no user memory
  - EAGAIN from SYS_THREAD_INTERRUPT when the frame stack is full
  - libsys::regs_probe! all-register probe (ledger row U-07) with two negative controls
affects: [02-09, 02-10, procmgr signal delivery (SIG-01/02), musl signal tests (Phase 6)]
tech-stack:
  added: []
  patterns:
    - "Redirect frames live on the kernel heap in Thread, not on the user stack: user code cannot forge or read them"
    - "Ktest-only CORRUPT_FRAME fault injection in restore_frame, reset by a drop guard, proves the probe can fail"
    - "A register-exact probe interrupts from a timer callback, because the scheduler does not preempt ring 3"
key-files:
  created:
    - userland/libsys/src/regs.rs
    - userland/tests/src/bin/sigreturn.rs
    - userland/tests/src/bin/sigframe.rs
  modified:
    - kernel/src/thread.rs
    - kernel/src/user.rs
    - kernel/src/syscall.rs
    - kernel/src/space.rs
    - userland/abi/src/lib.rs
    - userland/libsys/src/lib.rs
    - docs/architecture/unsafe-ledger.md
key-decisions:
  - "Frame = {UserContext, FpuContext, FS base}, stack depth MAX_FRAMES = 8 (OD-01); a ninth SYS_THREAD_INTERRUPT is SpaceError::Limit, which maps to EAGAIN"
  - "Handler rsp = ((rsp - 128) & !15) - 8, computed with wrapping arithmetic and no user write (OD-16)"
  - "SYS_SIGRETURN takes no pointer: nothing user-supplied selects the frame, so there is nothing to forge"
  - "RedirectFrame has a manual Debug and pointer-identity PartialEq/Eq so Outcome keeps its derives"
  - "Frames are cleared in Thread::retire, so a dead thread holds no FPU snapshots"
  - "Ledger id U-07 taken for regs.rs"
requirements-completed: [KERN-11]
status: complete
commits: 5
plan_head_before: 1fd04ddf14fb04b03ec38fcf34b1036580e1b8cb
plan_head_after: 04ca38beb5705eceef888a69c5fab0dfd58d112a
actuals:
  tokens: 8100
  tasks: 3
  commits: 5
duration: 35min
completed: 2026-10-26
---

# Phase 2 Plan 08: Kernel-held signal frame stack and SYS_SIGRETURN Summary

**A redirect now pushes {UserContext, FpuContext, FS base} onto a per-thread kernel stack (depth 8), moves the handler's rsp below the red zone by a register change, and SYS_SIGRETURN restores all of it, proven by an all-register probe with a 4 KiB stack canary and two negative controls.**

## Performance

- **Duration:** about 35 min wall clock (most of it QEMU TCG runs; the full suite takes over 10 min)
- **Started:** 2026-10-26T14:40:17Z
- **Completed:** 2026-10-26T15:15Z
- **Tasks:** 3 (Task 1 tracer, Tasks 2 and 3 TDD)
- **Files modified:** 10 (3 created)

## Accomplishments

- **Frame stack (Task 1, tracer).** `Thread::{try_interrupt, apply_redirect, pop_frame, restore_frame, frame_depth}`; `apply_redirect` saves the live FPU first when this thread owns it, so the frame holds the interrupted xmm state. `restore_frame` replaces the whole context (rax included), sets the FS base, writes the FPU state and clears the FPU owner slot so `pre_user_run` reloads it.
- **SYS_SIGRETURN.** `Syscall::Sigreturn`, `Outcome::Sigreturn(Box<RedirectFrame>)`, `run_ctx` restores on it; an empty stack is `EINVAL` and changes nothing.
- **All-register probe (Task 2).** `libsys::regs_probe!` loads a distinct pattern into every general register except rsp and into xmm0-xmm15, fills the 4 KiB below rsp with a canary, spins; the handler records rsp and pc, zeroes every register, and returns through SYS_SIGRETURN. `regs::check` returns 0 only when every register, the canary and the handler rsp match.
- **Negative controls.** `CORRUPT_FRAME=1` (rbx + 1) gives `Code(2)`; `CORRUPT_FRAME=2` (skip the FPU restore and leave the owner slot alone) gives `Code(20)`.
- **Bounded nesting (Task 3).** Eight nested redirects work; the ninth `thread_interrupt` is `Limit` (EAGAIN) and pushes nothing. `retire` clears the frames. The fuzz test gained the empty-stack SYS_SIGRETURN case (EINVAL) and the number in its pool.

## Task Commits

1. **Task 1: frame stack and SYS_SIGRETURN (tracer)** - `16b5125` (feat)
2. **Task 2 RED: probe tests and controls** - `efb3dbe` (test)
3. **Task 2 GREEN: regs_probe!, ledger U-07, CORRUPT_FRAME read** - `14a7fcd` (feat)
4. **Task 3 RED: depth, EINTR, drop-at-kill tests; sigreturn modes 2 and 4; FUTEX_WAIT in abi** - `ea08f0e` (test)
5. **Task 3 GREEN: try_interrupt in space.rs, retire clears frames, fuzz additions** - `04ca38b` (feat)

Docs commit for this SUMMARY follows (not counted in `commits:`).

## Plan outputs

- **Ledger id taken:** U-07 (`userland/libsys/src/regs.rs`). `python3 -I tools/unsafe_check.py` prints `unsafe_check: ok`.
- **`FUTEX_WAIT` added to the abi:** yes, `abi::FUTEX_WAIT = 0` beside `SYS_FUTEX` (the abi had none), so `userland/abi/src/lib.rs` is in the file list. The kernel's private `FUTEX_WAIT` const in `syscall.rs` is untouched.
- **No libsys futex wrapper existed**, so mode 2 uses the raw `syscall6` call.

## Evidence (all runtime evidence is QEMU TCG; this host has no /dev/kvm)

Verified here:
- `sh scripts/check.sh --no-kvm` ends `check: static gates passed` (after the rustfmt fix folded into `04ca38b`).
- The full unfiltered `cargo osdk test` under TCG: `test result: ok. 153 passed; 0 failed`, zero `FAILED` lines, exit 0. It includes all nine `sigreturn_*` tests, `fuzz_syscalls_never_panic` (100,000 iterations, no unknown errno), `xmm_survives_yield_between_two_threads`, `xmm_survives_redirect` and `timer_interrupt_redirects_a_running_thread`.
- RED was observed before each GREEN: Task 2's three tests failed against the placeholder probe; Task 3's depth and kill tests failed against the unchanged kernel.
- Acceptance greps: `try_interrupt` in space.rs 1, `MAX_FRAMES` in thread.rs 3, `SYS_SIGRETURN` in syscall.rs 4, `CORRUPT_FRAME` in thread.rs 2, `regs.rs` in the ledger at least 1.

Not verified:
- Nothing ran on KVM. The KVM re-run of the ktests is deferred (02-CONTEXT.md Deferred Ideas). TCG does not reproduce hardware timing, so the timer-interrupt probe delay (30 ticks) is calibrated for TCG only.
- x87 and MXCSR state are not probed; the probe covers the general registers (rsp excluded, checked separately through the handler rsp), xmm0-xmm15 and the stack canary.
- AVX state is out of scope, as it is for the existing FPU swap.

## Decisions Made

See `key-decisions` above. One addition: the probe's `check` runs on a stack 16 KiB below the entry rsp, so its own frames cannot disturb the canary range.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Plan verify command matches no tests**
- **Found during:** Task 1
- **Issue:** `cargo osdk test sigreturn_` (and `sigreturn`) filter to 0 tests; the ktest runner here matches the exact function name, one per invocation.
- **Fix:** Ran each test by exact name in a loop inside one `./osdk.sh sh -c`, and ran the full unfiltered suite at the end, which is the plan's actual completion gate.
- **Files modified:** none

**2. [Rule 1 - Bug] Ring-3 loops that wait for an interrupt must yield**
- **Found during:** Task 1 (a ktest run hung for over 10 minutes)
- **Issue:** The scheduler does not preempt user threads, so a bare spin in `sigreturn` mode 1 starved the test task that delivers the interrupt.
- **Fix:** Mode 1 calls `sched_yield` in its loop. Mode 4 (plan text: "spins forever") yields in both main and handler for the same reason. The hung container was stopped.
- **Files modified:** `userland/tests/src/bin/sigreturn.rs`
- **Committed in:** `16b5125`, `ea08f0e`

**3. [Rule 1 - Bug] Task 2's "yield until the probe is spinning, then interrupt" cannot work**
- **Found during:** Task 2
- **Issue:** The probe makes no syscalls, so no yield can land while it spins, and a bare-spinning probe starves the test task.
- **Fix:** The probe is interrupted from a timer callback (`register_callback_on_cpu` + `Jiffies` deadline, 30 ticks after start), the same mechanism as `timer_interrupt_redirects_a_running_thread`. A tick that landed before the probe finished loading its registers would fail the probe; 30 ticks gives TCG ample margin, and the probe passed on every run here.
- **Files modified:** `kernel/src/user.rs`
- **Committed in:** `efb3dbe`

**4. [Rule 1 - Bug] The skip-FPU control also had to leave the FPU owner slot alone**
- **Found during:** Task 2
- **Issue:** Skipping only the `*self.fpu.lock() = fpu` write still cleared `FPU_OWNER`, so `pre_user_run` reloaded the saved state and the control would have passed the probe.
- **Fix:** Under `CORRUPT_FRAME == 2`, `restore_frame` skips both the write and the owner clear, so the handler's zeroed xmm registers stay live and the probe reports xmm0.
- **Files modified:** `kernel/src/thread.rs`
- **Committed in:** `14a7fcd`

**5. [Rule 1 - Bug] `vec_box` clippy warning on the frame stack**
- **Found during:** Task 1
- **Issue:** `Vec<Box<RedirectFrame>>` trips `clippy::vec_box`. The Box keeps `Outcome` small and moves a frame by pointer.
- **Fix:** `#[allow(clippy::vec_box)]` with a justifying comment.
- **Committed in:** `16b5125`

### Other notes

- `sigreturn_after_a_blocked_call_returns_eintr` passed at its RED step: the behaviour (rax saved in the frame holds the `-EINTR` the interrupted futex wait produced) was already delivered by Task 1. It is a characterization test and guards that behaviour from here on; it was not seen to fail.
- The Task 3 RED commit carried one rustfmt-unclean assert; the fix is folded into the GREEN commit `04ca38b`.
- `SpaceError::Limit`'s doc comment now also names the full frame stack.

## Issues Encountered

- A stale `target/osdk` left root-owned files after the interrupted run; the host `rm` failed, and `./osdk.sh rm -rf target/osdk` from `kernel/` cleared it.
- Two Docker containers from plan 02-07 (`silly_satoshi`, `great_booth`) were running when I started. They were not mine and I left them; `docker ps` was empty at the end of this plan, and none of mine is running.
- The pre-existing `ermsb` target-feature warning in `check.sh` output is unrelated and untouched.

## Known Stubs

None.

## Threat Flags

None beyond the plan's `<threat_model>`: the new syscall takes no pointer and no user-chosen frame index, and the per-thread frames are bounded at 8 kernel-heap boxes and freed at retire.

## Self-Check: PASSED

Files created exist (`regs.rs`, `sigreturn.rs`, `sigframe.rs`); commits `16b5125`, `efb3dbe`, `14a7fcd`, `ea08f0e`, `04ca38b` are ancestors of HEAD.
