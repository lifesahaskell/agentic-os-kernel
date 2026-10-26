---
phase: 01-userland-syscall-abi-and-ipc-v2
verified: 2026-10-26T07:00:00Z
status: passed
score: 6/6 must-haves verified
covered_files:
  - .github/workflows/check.yml
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-01-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-01-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-02-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-02-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-03-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-03-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-04-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-04-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-05-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-05-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-06-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-06-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-07-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-07-SUMMARY.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-08-PLAN.md
  - .planning/phases/01-userland-syscall-abi-and-ipc-v2/01-08-SUMMARY.md
  - docs/architecture/ipc-measurements.md
  - docs/architecture/unsafe-ledger.md
  - kernel/src/agent.rs
  - kernel/src/cap.rs
  - kernel/src/futex.rs
  - kernel/src/ipc.rs
  - kernel/src/lib.rs
  - kernel/src/sched.rs
  - kernel/src/syscall.rs
  - kernel/src/thread.rs
  - kernel/src/user.rs
  - kernel/src/vm.rs
  - scripts/check.sh
  - tools/unsafe_check.py
  - userland/abi/src/lib.rs
  - userland/build.sh
  - userland/libsys/src/fpu.rs
  - userland/libsys/src/lib.rs
  - userland/libsys/src/raw.rs
  - userland/libsys/src/rt.rs
  - userland/tests/src/bin/kpush.rs
covered_digest: "v3:sha256:933cf9b179d9dcecacd2db00cfe2fb753d75d89533bb6e68dc4d602cdfd7936c"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 5/6
  gaps_closed:
    - "No entry point reachable from ring 3 can panic the kernel (CR-01: ring-3 store to a kernel-half address) - closed by 40a9da1, regression ktests run green"
  gaps_remaining: []
  regressions: []
deferred: []
---

# Phase 1: Userland, syscall ABI and IPC v2 Verification Report

**Phase Goal:** As a server author, I want a Rust ring-3 toolchain, a fuzz-safe syscall ABI and call/reply IPC, so that servers can run and the IPC cost is known.
**Verified:** 2026-10-26
**Status:** passed (all runtime evidence is QEMU TCG; see Provisional Items)
**Re-verification:** Yes, after gap closure plan 01-08 (commits `40a9da1`, `2ee2116`)

## Evidence basis

- I ran `sh scripts/check.sh` once against the current tree (HEAD `776de9e`; no uncommitted changes under `kernel/` or `userland/`). Exit 0, last line `check: all gates passed`. Log: `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/073e3793-02ab-45d9-bb4b-dd228dd9d4f0/scratchpad/check.log`. It contains `test result: ok. 69 passed; 0 failed; 0 filtered out` (was 67), zero `Panicked at` lines, no `FAILED`, no `Non-resettable panic`, `unsafe_check: self-test ok`, `unsafe_check: ok`, `aok: fpu preemption check passed` (ticks [26, 28]), `aok: cpu budget check passed` (child 60 ticks of quota 2, root 218).
- **All runtime evidence is QEMU TCG, not KVM.** `/dev/kvm` does not exist on this host; QEMU prints "TCG doesn't support requested feature" warnings (`tsc-deadline`, `pcid`, `invpcid`) in both runs. Timing-sensitive items are under Provisional Items; the phase is not failed for TCG.
- Gap-closure claims were checked against the code and the log, not the 01-08 SUMMARY. The red-before-fix claim in the SUMMARY (both new tests panic at `vm.rs:66` on HEAD before the fix) is taken from the SUMMARY's pasted log lines and the commit history: `git rev-list --count dd9ce23..HEAD -- kernel userland` is 2, and `40a9da1` contains tests and fix together, so no red commit exists. I did not re-run the pre-fix tree.

## Gap closure check (CR-01, WR-03)

| Item | Evidence in the current tree | Status |
|------|------------------------------|--------|
| CR-01 guard | `kernel/src/vm.rs:71` `if addr >= abi::USER_TOP { return Ok(false); }` before `page + PAGE_SIZE`, so no overflow for the top page. `USER_TOP = 0x0000_8000_0000_0000` (`userland/abi/src/lib.rs:120`). | VERIFIED |
| CR-01 fallible cursor | `vm.rs:76-78` `let Ok(mut c) = space.cursor_mut(..) else { return Ok(false) }`. The remaining `query().unwrap()` (vm.rs:81) and `jump().unwrap()` (vm.rs:107) run only after the guard and an accepted cursor, so the page is inside the user page table. | VERIFIED |
| CR-01 ring-3 path | `user.rs:273` still forwards raw `pf.addr`; `Ok(false)` falls to `deliver` (`user.rs:~215-240`), which with no exception handler returns `Exit::Fault(pf.addr)`. | VERIFIED |
| CR-01 ring-3 regression test | `userland/tests/src/bin/kpush.rs` (sentinel image, exits 0x5afe) plus `user.rs` `ring3_store_to_kernel_half_is_a_fault`: rsp in the kernel half gives `Exit::Fault(0xffff_8000_0000_0008)`, rsp 0 gives `Exit::Fault(0xffff_ffff_ffff_fff8)`, then the same agent runs kpush to `Exit::Code(0x5afe)`. Seen `ok` in the check log. No `unsafe` used. | VERIFIED |
| CR-01 unit test | `vm.rs` `cow_fault_outside_user_half_is_declined`: `USER_TOP - 1` (control), `0xffff_8000_0000_0000`, `0xffff_ffff_ffff_f000`, `usize::MAX`, `USER_TOP` all `Ok(false)`, `mem_used_pages() == 0`. Seen `ok`. | VERIFIED |
| WR-03 | `vm.rs:101-103` alloc-then-charge with `AgentError::NoMemory`; `user.rs` `load_image` and `map_stack` return `LoadError::NoMemory`; `syscall.rs` maps `AgentError::NoMemory` to `ENOMEM`. Remaining non-test `alloc_frame().unwrap()` is `lib.rs:104` in the boot budget check (kernel-initiated, not ring-3 reachable). | VERIFIED by inspection. The three failure branches never execute in a test (OSTD 0.19.0 has no allocation-failure injection; RR-02, accepted). |
| No new `unsafe` | `grep -rwn unsafe kernel/src` prints nothing; `unsafe_check: ok`. | VERIFIED |

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: Rust `no_std` ring-3 test crate built for `x86_64-aok-user.json` in Docker, loaded from a flat image, replaces hand-assembled B2 programs; B2 ktests and boot budget check green; clippy and fmt pass | VERIFIED (TCG; hosted CI run unobserved) | `userland/x86_64-aok-user.json`, `userland/build.sh`, 19 bins under `userland/tests/src/bin/` (18 plus `kpush`), loaded via `user_image!`/`load_image`. Check run: userland fmt + `clippy -D warnings`, kernel fmt + `cargo osdk clippy` clean; 69 ktests pass; budget check passes. Regression: the 27 B2 ktest names verified in the prior report still pass (69 total, 0 failed). `.github/workflows/check.yml` runs `check.sh` (`--no-kvm` mode when the runner lacks `/dev/kvm`, which skips ktests and boot checks); branch `research/recursion-spikes` is on origin but I did not observe a hosted run. |
| 2 | SC2: ring-3 program keeps XMM value across a yield; route recorded; SAFE-01 ledger | VERIFIED (TCG) | Regression check: `xmm_survives_yield_between_two_threads`, `xmm_swap_disabled_is_detected`, `xmm_survives_redirect`, `fpu_context_default_mxcsr_is_0x1f80` all `ok` in this run. Route recorded in `unsafe-ledger.md`; kernel has zero `unsafe`; `unsafe_check.py` and self-test pass. Limits unchanged: single CPU, SSE only, redirect checked to the capture point (`sigreturn` is Phase 2, KERN-11). |
| 3 | SC3: fuzz-style ktest feeds malformed numbers, pointers, lengths; errno, never a panic | VERIFIED (weak assertions, WR-06 open) | `fuzz_syscalls_never_panic` ok: `n=100000 enosys=37355 efault=14082 einval=993 ebadf=24872 eintr=472`; `decode_rejects_bad_numbers_pointers_and_lengths`, `badsys_gets_errno_from_ring3`, `futex_on_a_kernel_address_is_efault_not_a_panic` ok. |
| 4 | SC4: park/reply later; interrupted `call` sends Cancel and stays blocked; interrupted `read` loses no data; fail-point ktest on `block_until`; 1000x | VERIFIED (TCG timing) | Regression, all `ok` this run: `server_parks_a_request_and_replies_later`, `interrupted_call_waits_for_reply`, `queued_call_interrupt_is_not_started`, `only_the_kernel_sends_cancel`, `reply_to_dead_waiter_is_dropped`, `never_answering_server_keeps_caller_blocked`, `blocked_recv_still_aborts_at_once`, `interrupted_read_loses_no_data`, `block_until_failpoints_resolve_wake_versus_interrupt`, `block_until_interleavings_1000x` (`ok=679 interrupted=321 timed_out=0`). |
| 5 | SC5: tid+tag distinguish siblings; multi-page cross-space copy incl. first CoW write; capability attenuation; >64 caps; null-IPC with N, median, p99 | VERIFIED (null-IPC provisional: TCG) | All `ok` this run: `siblings_are_distinguished_by_tid_and_tag`, `multi_page_bulk_round_trip_matches_after_clone`, `cap_transfer_attenuates_through_a_chain`, `send_caps_checks_slot_and_grant`, `cap_install_is_all_or_none`, `cspace_grows_past_64_with_install_and_derive`, `revoke_reaches_copies_above_slot_64`, `more_than_64_caps_arrive_by_message`, `null_ipc_round_trip_is_recorded` (this run: `n=10000 median=5472104 p99=7407897 max=18081777`, TSC 4.4276 GHz). Recorded in `docs/architecture/ipc-measurements.md` (N=10,000, median 5,429,008, p99 7,537,376) with TCG label. |
| 6 | Derived: no entry point reachable from ring 3 panics the kernel (T-01-12, SC3 intent, core-value confinement) | VERIFIED for the page-fault path and the audited sites below; not a proof of absence | Was FAILED (CR-01). Now closed by the guard and ring-3 regression test above. I grepped non-test kernel code for `unwrap()`, `expect(`, `panic!`, `assert*`, `unreachable!`: `futex.rs:91-92` is guarded by `is_in_user_space(va, 4)` at `futex.rs:83` (and by the futex kernel-address ktest); `user.rs:290` (`panic!` on an exception without exception info) is an OSTD invariant, not ring-3 controlled; `thread.rs:263` and `sched.rs:98` are kernel-internal; `vm.rs:30-48` `cow_clone` unwraps are not ring-3 reachable at this HEAD (no syscall clones a space) and become reachable in Phase 2 (RR-01, recorded). The syscall dispatch surface is covered by the 100,000-call fuzz ktest. |

**Score:** 6/6 truths verified (0 behavior-unverified)

### Required Artifacts

`verify.artifacts` on `01-08-PLAN.md`: 5/5 passed (`vm.rs`, `user.rs`, `kpush.rs`, `agent.rs`, `syscall.rs`, each containing the declared pattern). Artifacts of plans 01-01..01-07 were verified in the prior report and re-checked here by regression: all named ktests pass, `check.sh` builds all images, `unsafe_check` passes.

| Artifact | Status | Details |
|----------|--------|---------|
| `userland/x86_64-aok-user.json`, `userland/build.sh`, `userland/libsys`, `userland/abi`, `userland/tests` | VERIFIED | Built and loaded in this run |
| `kernel/src/vm.rs` `copy_to_user`/`copy_from_user`/`handle_cow_fault` | VERIFIED | Guarded; `copy_to_user` propagates `AgentError` via `?` to `Errno` (`From<AgentError>`, `NoMemory` and `Budget` both ENOMEM) |
| `kernel/src/thread.rs` `block_until` + failpoints | VERIFIED | Ktests ok |
| `kernel/src/ipc.rs`, `cap.rs` | VERIFIED | Ktests ok |
| `tools/unsafe_check.py`, `docs/architecture/unsafe-ledger.md` | VERIFIED (blind spots WR-08 latent) | Run and self-test pass |
| `scripts/check.sh`, `.github/workflows/check.yml` | VERIFIED locally, weak (WR-07) | Local run passed |
| `docs/architecture/ipc-measurements.md` | VERIFIED | N, median, p99, TCG label |

### Key Link Verification

`verify.key-links` on `01-08-PLAN.md` returned 0/3, but only because the plan's `from` fields are prose, not file paths ("Source file not found (from: must be a relative file path...)"). That is a plan-format defect, not a wiring result. I verified the three links by hand:

| From | To | Via | Status |
|------|----|-----|--------|
| `user.rs` `run_ctx` WRITE-fault arm (`user.rs:273`) | `vm.rs` `handle_cow_fault` | raw `pf.addr`; `Ok(false)` falls through to `deliver` | WIRED |
| `user.rs` `deliver` | `Exit::Fault` | no exception handler gives `fatal()`, `Exit::Fault(pf.addr)` | WIRED (exercised by `ring3_store_to_kernel_half_is_a_fault`) |
| `vm.rs` `copy_to_user` (`vm.rs:152`) | `syscall.rs` `From<AgentError> for Errno` | `handle_cow_fault(..)?` | WIRED |

### Data-Flow Trace (Level 4)

Not applicable beyond the null-IPC figures, which come from a TSC read inside the ring-3 `nullipc` image and are printed by the ktest, not hardcoded.

### Behavioral Spot-Checks

Covered by the single `check.sh` run; no extra named-test runs needed.

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full gate | `sh scripts/check.sh` | exit 0, 69 ktests passed, both boot checks passed | PASS (TCG) |
| CR-01 ring-3 kernel-half store | ktest `ring3_store_to_kernel_half_is_a_fault` (inside the gate) | ok, kernel survives, kpush then exits 0x5afe | PASS (TCG) |
| CR-01 unit | ktest `cow_fault_outside_user_half_is_declined` | ok | PASS (TCG) |

### Probe Execution

No probes declared by the phase plans; none under `scripts/*/tests/probe-*.sh` (`scripts/` holds only `check.sh`). SKIPPED.

### Requirements Coverage

All 14 IDs in ROADMAP Phase 1 appear in plan `requirements` frontmatter and in REQUIREMENTS.md. No orphans.

| Requirement | Status | Evidence |
|-------------|--------|----------|
| KERN-01 | SATISFIED (TCG; redirect checked to capture point) | SC2 |
| SAFE-01 | SATISFIED (WR-08 latent) | Ledger, checker and self-test green, kernel `unsafe` count 0 |
| KERN-02 | SATISFIED | SC3 plus CR-01 closure (ring-3 hostile address no longer panics) |
| KERN-03 | SATISFIED | SC1 |
| KERN-05 | SATISFIED | SC5 |
| KERN-06 | SATISFIED | `copy_to_user` ktests, `vm.rs` guards |
| KERN-12 | SATISFIED | SC4 |
| IPC-01 | SATISFIED | `park_srv` |
| IPC-02 | SATISFIED (WR-01/WR-02 edge cases untested) | SC4 |
| IPC-03 | SATISFIED (tag set by `Thread::new_tagged`; `thread_create` is Phase 2 KERN-04) | SC5 |
| IPC-04 | SATISFIED | SC5 |
| IPC-05 | SATISFIED for the generic kernel mechanism; the vfs-through-exec scenario cannot exist until Phases 3-4 | SC5 |
| IPC-06 | SATISFIED, PROVISIONAL (TCG) | ipc-measurements.md |
| TEST-06 | SATISFIED locally; hosted CI run unobserved; gate weak (WR-05, WR-07) | check.sh green |

REQUIREMENTS.md still shows KERN-01, SAFE-01, KERN-02, KERN-05, KERN-06, KERN-12, IPC-02..IPC-06, TEST-06 unchecked and Pending (KERN-03, IPC-01 are checked). The orchestrator should update the checkboxes and traceability table; I did not edit REQUIREMENTS.md.

### Review findings status

| Finding | Disposition | Bears on | Verdict |
|---------|-------------|----------|---------|
| CR-01 | fixed (`40a9da1`) | T-01-12, SC3 intent | Closed; verified above |
| WR-03 | fixed (`2ee2116`) | SC3 intent | Closed by inspection; failure branches untested (RR-02) |
| WR-01, WR-02 | open | IPC-02 | Warning. Needs two receivers on one endpoint (WR-01) or a server write-back failure racing an interrupt (WR-02). Fix before multi-threaded servers (Phase 3+). |
| WR-04, WR-05 | open | TEST-06 | Warning. CPU budget sampled at tick; boot check cannot fail on under-charging. Observed this run: child 60 ticks, FPU ticks [26, 28]. |
| WR-06 | open | SC3 | Warning. Literal SC3 holds; leak-on-error unchecked. |
| WR-07 | open | TEST-06 | Warning. `|| true` and no timeout on the boot gate; the log is grep-checked for both pass strings and `FAILED`, which this run satisfied. |
| WR-08 | open | SAFE-01 | Warning. Latent checker blind spots. |
| IN-01..IN-06 | open | none | Informational |

None of the open items fails a roadmap success criterion. They are warnings, recorded in `01-REVIEW-DISPOSITION.md`.

### Anti-Patterns Found

No `TBD`, `FIXME`, `XXX`, `TODO`, `todo!` or `unimplemented!` in `kernel/src`, `userland/{abi,libsys,tests}`, `scripts`, `tools`, or the two phase docs. The new `// shortcut:` comment in `user.rs` (CoW allocator failure ends as `Exit::Budget`) is a documented deviation, not a stub. No stubs found.

### Human Verification Required

None required by the roadmap criteria. Two optional re-checks are carried as Provisional Items below, not as gates.

### Provisional Items (TCG, not hardware)

| Item | Why provisional |
|------|-----------------|
| IPC-06 null-IPC median/p99 | TSC under TCG is not tied to executed work. Three TCG runs gave 5.43M/7.54M (doc), 5.47M/7.41M (this run), 5.79M/8.37M (earlier verification), N=10,000. Ktest asserts only `n == 10_000` and `median > 0`. Re-measure on a KVM host before PERF-02 uses it (STATE.md blocker, open). |
| CPU budget boot check (33 periods: root 218, child 60) and FPU preemption (ticks [26, 28]) | Tick-driven; TCG lacks `tsc-deadline`. Upper bounds real; lower bounds weak (WR-05). |
| Fail-point and 1000x `block_until` tests | Deterministic interleavings; wall-clock deadlines are the only timing dependence. |
| Hosted CI | SC1 and TEST-06 say "in CI". The workflow exists and runs `check.sh`, but I observed no hosted run, and in `--no-kvm` mode CI skips ktests and boot checks. The static gates (fmt, clippy, unsafe_check, build) run identically in the same image locally and passed. |
| Pre-fix red run of the CR-01 tests | Taken from the 01-08 SUMMARY log excerpt; not re-run by me. The post-fix green is verified. |

Judgment call for the owner: I set `status: passed` because the written criteria name no hardware, and every criterion is met by tests that exist and pass in this run. If you want KVM re-measurement of IPC-06, or one observed green hosted CI run, as a gate for closing Phase 1, change this to `human_needed` and add those as items. STATE.md already tracks the KVM re-run.

## Gaps Summary

No gaps. The sole blocker from the previous report (CR-01) is closed: the guard, the fallible cursor and the ring-3 regression test are present in the tree, wired, and passing in a fresh full gate run (69 passed, 0 failed, no panics). Open warnings WR-01, WR-02, WR-04..WR-08 and IN-01..IN-06 do not fail any success criterion. Recorded residuals: RR-01 (`cow_clone` unwraps become ring-3 reachable once Phase 2 calls it from procmgr fork/spawn; Phase 2 should make it fallible or prove the range), RR-02 (allocation-failure branches untested, no OSTD injection), RR-03 (`futex.rs:88` maps every `handle_cow_fault` error to `FutexError::Budget`).

---

_Verified: 2026-10-26_
_Verifier: Claude (gsd-verifier)_
