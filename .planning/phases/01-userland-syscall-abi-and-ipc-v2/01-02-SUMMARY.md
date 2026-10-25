---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 02
subsystem: safety-gates-and-fpu
tags: [rust, unsafe-ledger, ci, fpu, sse, xsave, ostd, ring3]

requires:
  - phase: 01-01
    provides: userland workspace, flat image loader, ring-3 test programs, user::testkit
provides:
  - SAFE-01 ledger, tools/unsafe_check.py (with --self-test), scripts/check.sh, .github/workflows/check.yml
  - Per-thread FpuContext swap through the safe OSTD route (no kernel unsafe)
  - userland fpu test program, four FPU ktests, boot-time timer-preemption check
affects: [01-03, 01-04, 01-05, 01-06, 01-07, phase-2, phase-3, phase-4]

plan_head_before: 433ae60c5a4089bd0d4292db378ba8e3a1b00bd5
plan_head_after: d6ddb8ac74447c92f94f8990e0265740be986837

actuals:
  tokens: 14000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - every unsafe site is ledgered (K-nn kernel, U-nn userland) and tagged `// SAFETY: <ID>`; checker is the first gate in scripts/check.sh
    - FPU state is swapped lazily by owner tracking in UserModeHooks::pre_user_run
    - ring-3 xmm probes use one asm block with no Rust between load and check

key-files:
  created:
    - docs/architecture/unsafe-ledger.md
    - tools/unsafe_check.py
    - scripts/check.sh
    - .github/workflows/check.yml
    - userland/libsys/src/fpu.rs
    - userland/tests/src/bin/fpu.rs
  modified:
    - kernel/osdk.sh
    - kernel/src/thread.rs
    - kernel/src/lib.rs
    - kernel/src/user.rs
    - userland/libsys/src/lib.rs
    - userland/libsys/src/rt.rs

key-decisions:
  - "KERN-01 route A (safe): FpuContext per Thread plus FPU_OWNER swap in pre_user_run; D-02 module and D-04 halt not needed"
  - "Default FpuContext already has MXCSR 0x1F80 and FCW 0x037F; no explicit initialisation in Thread::new"
  - "scripts/check.sh clears kernel/target/osdk before cargo osdk run, because OSDK otherwise reuses the ktest bundle"

requirements-completed: [SAFE-01, TEST-06, KERN-01]

coverage:
  - id: D1
    description: "unsafe_check.py exits 0 on the tree; its self-test fails on an unlisted unsafe site, a missing SAFETY comment, a ledger row with no block, an unledgered SAFETY ID and a crate root without deny(unsafe_code)"
    requirement: "SAFE-01"
    verification:
      - kind: unit
        ref: "python3 -I tools/unsafe_check.py --self-test; probe file kernel/src/probe.rs made the checker exit 1 naming it"
        status: pass
    human_judgment: false
  - id: D2
    description: "scripts/check.sh runs unsafe check, userland build/fmt/clippy, kernel fmt/clippy, ktests and boot checks; --no-kvm runs the static gates and names the local-only ones"
    requirement: "TEST-06"
    verification:
      - kind: integration
        ref: "sh scripts/check.sh -> 'check: all gates passed'; sh scripts/check.sh --no-kvm -> 'check: static gates passed; LOCAL-ONLY ...'"
        status: pass
    human_judgment: false
  - id: D3
    description: "XMM state is per thread: two ring-3 threads with different 16-register patterns keep their own values across yields, across timer preemption and across a signal redirect; with the swap disabled the yield test detects corruption"
    requirement: "KERN-01"
    verification:
      - kind: integration
        ref: "aok::thread::test::{fpu_context_default_mxcsr_is_0x1f80, xmm_survives_yield_between_two_threads, xmm_swap_disabled_is_detected, xmm_survives_redirect} (33 ktests passed); boot line 'aok: fpu preemption check passed'"
        status: pass
    human_judgment: false
  - id: D4
    description: "CI workflow runs scripts/check.sh and reports its mode in the job summary"
    requirement: "TEST-06"
    verification:
      - kind: manual
        ref: ".github/workflows/check.yml written; never executed on GitHub from this worktree"
        status: unrun
    human_judgment: true

duration: 25min
completed: 2026-10-24
status: complete
---

# Phase 1 Plan 02: Unsafe ledger, check gates and FPU/SSE switch Summary

**SAFE-01 is enforced by a self-tested ledger checker over both source trees, one script runs every gate, and per-thread XMM state works through the safe OSTD route (`FpuContext` plus owner tracking in `pre_user_run`) with no kernel `unsafe`.**

## Accomplishments

- `tools/unsafe_check.py` blanks comments, strings, raw strings and char literals, then fails on rules (a)-(e) from the plan; its self-test covers each rule plus comment/string false positives. A probe `kernel/src/probe.rs` with `unsafe {}` made it exit 1 naming the file.
- `docs/architecture/unsafe-ledger.md` lists U-01 (`syscall6`), U-02 (`entry!` stubs, extended for the xmm capture) and U-03 (`hold`). The Kernel table is empty.
- `scripts/check.sh` gates: checker self-test, checker, userland build+fmt+clippy, kernel fmt+clippy, `cargo osdk test`, `cargo osdk run` (requires both boot lines, rejects `FAILED`). `--no-kvm` swaps the last two for `cargo osdk build` and ends with a LOCAL-ONLY line.
- `kernel/osdk.sh` passes `--device /dev/kvm` only when the node exists.
- KERN-01: `Thread.fpu`, `FPU_OWNER`, `pre_user_run`, ktest-only `FPU_SWAP_DISABLED`; `libsys::fpu::{pattern, hold}`; two-argument `entry!(main, handler)` whose stub stores xmm0-xmm15 into `HANDLER_XMM` before any Rust runs.
- 33 ktests pass (29 prior plus the 4 FPU ktests). Boot output: `aok: fpu preemption: ticks [26, 26], exits ok true`, `aok: fpu preemption check passed`, `aok: cpu budget check passed`.

## Task Commits

1. **Task 1: ledger, checker, check script, CI, osdk.sh** - `e3dfe91` (feat)
2. **Task 2: per-thread FpuContext swap, fpu program, ktests, boot check** - `0a9be5d` (feat)
3. **Task 3: record KERN-01 route** - `d6ddb8a` (docs)

## FPU route

Route A (safe). The D-01 spike passed on the first attempt, so the D-02 contained-unsafe module `kernel/src/fpu.rs` was not created and D-04 did not fire. The default `FpuContext` already reports MXCSR 0x1F80 and FCW 0x037F (ktest `fpu_context_default_mxcsr_is_0x1f80`), so `Thread::new` needs no explicit initialisation.

## Recorded outcomes requested by the plan

- **Pinned checkout SHA:** `11d5960a326750d5838078e36cf38b85af677262` (`actions/checkout` v4.4.0, newest v4 tag from `git ls-remote`; lightweight tag, so the SHA is the commit).
- **CI mode:** not run. The workflow chooses full mode when `/dev/kvm` is writable and `--no-kvm` otherwise, and writes the mode to `$GITHUB_STEP_SUMMARY`. Locally both modes were run and passed.

## Deviations from Plan

**1. [Rule 3 - Blocking] OSDK `run` reuses the ktest bundle after `test`.**
- **Found during:** Task 1 verification (`sh scripts/check.sh`).
- **Issue:** `cargo osdk run` right after `cargo osdk test` re-ran the ktests instead of booting `kernel_main`, so the boot check line was absent. `rm -rf target/osdk/iso_root` was not enough.
- **Fix:** the run gate in `scripts/check.sh` clears `target/osdk` first.
- **Commit:** `e3dfe91`

**2. [Rule 1 - Bug] Userland clippy failed on `loop {}` in `libsys::exit`.** 01-01 never ran userland clippy. Replaced with `loop { core::hint::spin_loop() }`. Commit `e3dfe91`.

**3. [Rule 3 - Blocking] `/dev/kvm` is not writable by the user on this host** (root:kvm 0660; Docker passes it through). `osdk.sh` therefore tests `-e`, not `-w`. The CI step still tests `-w`, which is correct after the udev rule sets 0666. Commit `e3dfe91`.

**4. Plan sequencing.** The U-03 ledger row and the `aok: fpu preemption check passed` requirement in `check.sh` were added in Task 2, not Task 1, so Task 1's `check.sh` run passes before the FPU code exists. Also removed the now-obsolete `allow(unused_macros)` and `allow(unused_imports)` on `user_image!`, since `lib.rs` uses it in non-test code. The ktests live in a new `thread.rs` test module, as the plan allowed.

## Issues Encountered

- A bare `userland/` `cargo fmt --check` passes over the long `asm!` operand list; rustfmt does not reformat inside `asm!`.
- OSDK emits `unstable feature specified for -Ctarget-feature: ermsb`. Pre-existing, not touched.

## Verified versus not verified

- Verified (run here): checker self-test and tree check, probe file, `scripts/check.sh` in both modes, 33 ktests, boot checks, fmt and clippy for both trees, the plan's acceptance greps.
- Not verified: the CI workflow on GitHub (never pushed); any SMP behaviour; AVX and x87 state (out of scope per D-03). The negative control covers the yield path only. The preemption check has no swap-off counterpart because the BudgetScheduler cannot be injected in a ktest, so it shows the swap holds under timer interleaving but not that the check would fail without it.
- The `scripts/check.sh` `FAILED` grep matches any line containing that word in the boot log, including debug output.

## Known Stubs

None.

## Threat Flags

None. `pre_user_run` closes T-01-06 (cross-thread XMM disclosure) and is exercised by the negative control. The CI action is pinned to a full SHA (T-01-09), and no new crates were added (T-01-SC).

## Self-Check: PASSED

- Files present: ledger, checker, check.sh, check.yml, libsys fpu.rs, tests fpu.rs, `userland/out/fpu.bin` (built).
- Commits `e3dfe91`, `0a9be5d`, `d6ddb8a` are ancestors of HEAD.
- `grep -rlE '\bunsafe\b|allow\(unsafe_code\)' kernel/src` prints nothing.
