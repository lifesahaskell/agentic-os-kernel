---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 07
subsystem: kernel-tests
tags: [ring3-rust-images, null-ipc, rdtsc, unsafe-ledger, IPC-06]
requires: [01-06]
provides:
  - "Every B2 hand-assembled ring-3 program replaced by a Rust bin; kernel `user::prog`, the B2 `load`/`run` and the PROGRAM byte array are gone"
  - "abi::TEST_DATA, libsys::test_page / trap::ud2 / tsc (ledger U-04, U-06, U-05), user::map_stack and user::map_test_page"
  - "Null-IPC round trip measured over 10,000 calls and recorded in docs/architecture/ipc-measurements.md"
affects: []
key-files:
  created:
    - userland/tests/src/bin/yielder.rs
    - userland/tests/src/bin/spinner.rs
    - userland/tests/src/bin/spin.rs
    - userland/tests/src/bin/cow_writer.rs
    - userland/tests/src/bin/ud2.rs
    - userland/tests/src/bin/futex_waiter.rs
    - userland/tests/src/bin/futex_waker.rs
    - userland/tests/src/bin/nullsrv.rs
    - userland/tests/src/bin/nullipc.rs
    - docs/architecture/ipc-measurements.md
  modified:
    - kernel/src/user.rs
    - kernel/src/lib.rs
    - kernel/src/sched.rs
    - kernel/src/ipc.rs
    - userland/abi/src/lib.rs
    - userland/libsys/src/lib.rs
    - docs/architecture/unsafe-ledger.md
decisions:
  - "`core::arch::x86_64::_rdtsc` is `unsafe` on nightly-2026-07-21, so `libsys::tsc()` got ledger row U-05 (the plan allowed no entry only if it were safe)"
  - "Ledger ids follow the order the modules appear in libsys (U-04 test_page, U-05 tsc, U-06 ud2), not the order the plan named them"
  - "Null-IPC figures are reported only; the ktest asserts n == 10_000 and median > 0 and never a ceiling"
status: complete
commits: 3
plan_head_before: cf15e03d7a3d99aa4c6acebad823b4ad65718cc5
plan_head_after: 008e49523d1afc5d659560f43d0db02e06ae6605
requirements-completed: [IPC-06]
actuals:
  tokens: 6600
  tasks: 3
  commits: 3
---

# Phase 1 Plan 07: Port B2 test programs to Rust and measure null IPC Summary

All B2 ring-3 test programs now run as Rust flat images loaded through `user_image!`/`load_image`/`start`, with the same 66 ktest names and assertions (pc ranges became `img.text`), and the null-IPC round trip is measured: median 5,429,008 cycles, p99 7,537,376, max 19,566,536 over N = 10,000 under QEMU TCG (provisional, not a hardware baseline).

## What was done

- **Task 1 (`df379e2`).** Deleted `user::prog`, the B2 `load`/`run`, the test `PROGRAM` array and `testkit::spawn_user`. Added the bins `yielder`, `spinner`, `spin`, `cow_writer`, `ud2`, `futex_waiter`, `futex_waker`; `abi::TEST_DATA` (0x50_0000); `libsys::test_page`, `trap::ud2`, `clock::tsc`; kernel helpers `user::map_stack` (factored out of `load_image`) and `user::map_test_page`. Rewrote the ktests and the boot `cpu_budget_check` on top of them. 66 ktests passed before and after, with identical names (compared against HEAD's `#[ktest]` list). `grep -rn 'prog::' kernel/src` and `grep -rnE 'mod prog|const PROGRAM' kernel/src` print nothing.
- **Task 2 (`16a6f19`).** `nullsrv` and `nullipc` bins, ktest `null_ipc_round_trip_is_recorded` (kernel thread answers the single report call), and `docs/architecture/ipc-measurements.md`.
- **Task 3 (`008e495`).** `sh scripts/check.sh` ran green on the first attempt, ending `check: all gates passed`. Added "Revisit in a hardening pass" to every ledger row, corrected the FPU evidence note, and added the CI-versus-local paragraph to `ipc-measurements.md`.

## Measurement (from the saved ktest log)

```
aok: null-ipc n=10000 median=5429008 p99=7537376 max=19566536 cycles
aok: null-ipc tsc_freq=4426760800 Hz
```

Host: AMD Ryzen 9 7900X3D, WSL2 guest, no `/dev/kvm`. QEMU flags from `kernel/OSDK.toml`: `-cpu Icelake-Server,+x2apic`, `-smp 1`, no accelerator flag, so TCG. The doc labels the figures "QEMU TCG (no KVM; software emulation), provisional: re-measure on a KVM-enabled host" and states they are not a hardware baseline. It also notes that earlier "QEMU/KVM" claims (for example `b2-spike-report.md` line 14) are unverified; those docs were not edited. `b2-spike-report.md` has no IPC timing figure, so there is no B2 comparison line.

The number is large: 5.4 million cycles is about 1.2 ms at the reported TSC frequency. The doc offers two untested explanations marked *(interpretation)*: TCG overhead, or the client waiting for a scheduler tick instead of a direct handoff to the server. Neither was investigated, because the plan says report only. This is worth a look in a later phase.

## Gate results

- `sh scripts/check.sh` (full mode, no `--no-kvm`), 2026-10-26: unsafe_check self-test ok, unsafe_check ok, userland build/fmt/clippy and kernel fmt/clippy clean, 67 ktests passed (66 baseline plus the new one), `aok: fpu preemption check passed`, `aok: cpu budget check passed` (33 periods, root 217 ticks, child 62), final line `check: all gates passed`.
- `aok: cpu budget check` flake: it never failed. Outcomes across the whole plan: Task 1 `cargo osdk run` passed (root 217, child 61), Task 3 `check.sh` boot run passed (root 217, child 62). Zero retries. No code was loosened, skipped or retry-wrapped.
- Task 3's `check.sh` ran before the final docs-only edit; the later commit touched only two `.md` files, and `python3 -I tools/unsafe_check.py` was re-run after it and printed `unsafe_check: ok`.
- Ledger counts: `grep -ciE 'revisit' docs/architecture/unsafe-ledger.md` = 6; rows matching `^\| *[UK]-[0-9]+` = 6.
- CI versus local: `.github/workflows/check.yml` runs the full script only if `/dev/kvm` is writable on the runner, otherwise `--no-kvm` (static gates plus `cargo osdk build`; ktests and boot checks local only). The branch is not pushed, so no CI run exists; which mode the hosted runner takes is not verified. Everything in this plan ran locally in full mode under TCG. Recorded in `ipc-measurements.md` with the date.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `_rdtsc` is `unsafe`, so ledger row U-05 was required**
- **Found during:** Task 2 (first compile of `libsys`; error E0133)
- **Fix:** `libsys::clock::tsc()` with `#![allow(unsafe_code)]` and `// SAFETY: U-05`; ledger row added. The plan anticipated this branch. OSTD never sets CR4.TSD (grep found nothing), so ring-3 `rdtsc` is allowed.
- **Files modified:** `userland/libsys/src/lib.rs`, `docs/architecture/unsafe-ledger.md`

**2. [Rule 3 - Blocking] `unsafe_check` rejected the U-04 SAFETY comment**
- **Issue:** the `// SAFETY:` comment sat more than 3 lines above the `unsafe` token.
- **Fix:** shortened the comment so it fits the 3-line window; the fuller explanation is in the ledger row.

**3. [Rule 3 - Blocking] Compile errors after the port**
- Missing `Exit` import in `sched.rs`, unused `CachePolicy`/`PageProperty` imports in `user.rs`, and `println!` cannot capture inline variables (`{n}`) in this setup. All fixed in the same tasks.

### Design deviations from the plan text (not rule-triggered)

- **Ledger id order.** U-04 is `test_page`, U-05 is `tsc`, U-06 is `ud2`. The plan's acceptance only requires U-04 to exist.
- **Samples on the stack.** The plan said a `static` `.bss` `[u64; 10_000]`. A `static mut` would need another `unsafe` block; a stack array of 80 KB fits the 32-page (128 KB) user stack and sorts as a plain `&mut [u64]`, with no new ledger row.
- **Same-space futex test.** Uses a kernel-side waker instead of a second ring-3 program in the same space (all images link at the same address, so two cannot share a space).
- **Address-space switch proof.** The old "different code address per space" check does not exist for same-address Rust images. `spinner` and `yielder` instead count on their own test page and re-read an id; a thread resumed under the wrong space sees a foreign id and exits 1. The boot check and `each_task_resumes_in_its_own_address_space` assert on that.
- **Exact pc assertions.** Replaced by `img.text.contains(pc)` and rip equality between the fault message and the exit code, as the plan allowed.
- **`cow_writer` stores a u32** through `test_page()` (an `AtomicU32` slice) rather than a byte.
- **Extra kernel helpers.** `user::map_stack`, `user::map_test_page`, and test helper `cow_child` (clone `IMAGE_BASE..TEST_DATA+PAGE_SIZE` plus a fresh stack so `mem_used_pages() == 1` still holds). All in `kernel/src/user.rs`, which Task 1 lists; the port needed them.
- **`tsc` and ledger row U-05 landed in the Task 1 commit**, not Task 2, because the ledger rows U-04 to U-06 were added together and `libsys` was edited once.
- **Two root agents, no child.** The ktest uses separate root agents with cspaces filled directly instead of `spawn_child`, so the CPU quota of a child (1 tick) cannot interact with the run.

## Not verified

- **No KVM anywhere.** Every ktest and boot result in this plan is from QEMU TCG. Whether the same code passes under KVM is untested.
- **No hardware baseline.** The null-IPC cycle counts are a TSC read inside an emulated guest. They say nothing about bare metal or KVM latency.
- **`rdtsc` is not serialised** (no `lfence`/`rdtscp`), so single samples carry skew. Not quantified; negligible against millions of cycles, but unmeasured.
- **The cause of the 5.4M-cycle round trip** (TCG overhead versus scheduler tick wait) is a hypothesis only.
- **CI.** No CI run for this branch exists. Which `check.sh` mode the hosted runner takes is unknown.
- **The `cpu budget check` flake rate** is not characterised: two runs, both passed.
- **Flat-image and stack behaviour on the boot path under repeated runs** was not stress-tested beyond one `check.sh` pass.

## Known Stubs

None. No TODO, FIXME or placeholder text in the files created or modified.

## Threat Flags

None. No new network endpoint, auth path or trust-boundary schema. `tsc()` exposes the TSC to ring 3, which is already the hardware default (CR4.TSD clear).

## Self-Check: PASSED

- Files exist: all ten created files above (checked with `git status` and `git diff --numstat` against the base).
- Commits reachable from HEAD: `df379e2`, `16a6f19`, `008e495`.
- `git rev-list --count cf15e03..HEAD` = 3 at the time of writing.
