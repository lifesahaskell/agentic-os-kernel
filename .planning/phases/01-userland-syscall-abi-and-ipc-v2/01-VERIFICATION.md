---
phase: 01-userland-syscall-abi-and-ipc-v2
verified: 2026-10-26T00:00:00Z
status: gaps_found
score: 5/6 must-haves verified
covered_files:
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
covered_digest: "v3:sha256:72252af17fbb33d1b114610d338f6e7755eb15ccb12f14f3dfc086f5916c6328"
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "No entry point reachable from ring 3 can panic the kernel (plan 01-03 threat T-01-12: 'any handler reachable from ring 3', mitigation 'no panic'; intent of SC3 and the capability-confinement core value)"
    status: failed
    reason: "A ring-3 store to a kernel-half address raises a #PF with WRITE set. run_ctx passes pf.addr straight to handle_cow_fault, whose cursor_mut(..).unwrap() gets Err(InvalidVaddrRange) from OSTD and panics. One unprivileged instruction halts the kernel instead of delivering Exit::Fault. Confirmed by reading kernel/src/user.rs:261-268, kernel/src/vm.rs:66 and ostd-0.19.0 page_table/cursor/mod.rs:112-114; NOT executed. No ktest or ring-3 bin does a direct kernel-half store (badsys and the fuzz test only pass kernel-half addresses to syscalls)."
    artifacts:
      - path: "kernel/src/vm.rs"
        issue: "handle_cow_fault unwraps cursor_mut for an unvalidated fault address; page + PAGE_SIZE also overflows for addr >= 0xffff_ffff_ffff_f000"
      - path: "kernel/src/user.rs"
        issue: "run_ctx forwards every write #PF address to handle_cow_fault without checking addr < USER_TOP"
    missing:
      - "In handle_cow_fault, return Ok(false) when addr >= abi::USER_TOP and when cursor_mut returns Err (fix given in 01-REVIEW.md CR-01)"
      - "A ring-3 bin that stores to 0xffff_8000_0000_0000 plus a ktest asserting Exit::Fault (vector 14) is delivered, not a panic"
deferred: []
---

# Phase 1: Userland, syscall ABI and IPC v2 Verification Report

**Phase Goal:** As a server author, I want a Rust ring-3 toolchain, a fuzz-safe syscall ABI and call/reply IPC, so that servers can run and the IPC cost is known.
**Verified:** 2026-10-26
**Status:** gaps_found
**Re-verification:** No, initial verification

## Evidence basis

- I ran `sh scripts/check.sh` once on this host. Exit 0, last line `check: all gates passed`. Output contains `test result: ok. 67 passed; 0 failed`, `aok: fpu preemption check passed`, `aok: cpu budget check passed`, `unsafe_check: ok`, `unsafe_check: self-test ok`. Full log: `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/bddf2157-1df5-4b7e-a6aa-3f8ec74ce447/scratchpad/check.log`.
- **All runtime evidence is QEMU TCG, not KVM.** `/dev/kvm` does not exist on this host (WSL2), OSDK 0.19.0 passes no `-enable-kvm`, and QEMU printed "TCG doesn't support requested feature" warnings (including `tsc-deadline`, `pcid`, `invpcid`) in both the ktest and boot runs. Earlier "QEMU/KVM" labels (b2-spike-report.md, 01-04 SUMMARY, the FPU evidence in unsafe-ledger.md) are unverified. Timing-sensitive items are listed under Provisional Items; the phase is not failed for TCG.
- Truths below were checked against code and tests, not SUMMARY claims. Each ktest named was seen passing in the check log.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: Rust `no_std` ring-3 test crate built for `x86_64-aok-user.json` in Docker, loaded from a flat image, replaces hand-assembled B2 programs; B2 ktests and boot budget check green; clippy and fmt pass | VERIFIED (green gates are TCG, provisional for timing) | `userland/x86_64-aok-user.json` (+sse,+sse2, small code model, static reloc, no redzone). `userland/build.sh` builds 18 bins in `tests/src/bin/` to `AOKIMG01` flat images; kernel loads them via `user_image!` / `load_image` (`include_bytes!`, not an initramfs; CONTEXT allowed this fallback). No `0x0f,0x05` byte programs or asm remain in `kernel/src`. All 27 B2 ktest names (extracted from the pre-01-01 commit `107a6b8`) still exist and pass. check.sh ran userland fmt+clippy `-D warnings`, kernel fmt + `cargo osdk clippy`: clean. `.github/workflows/check.yml` runs check.sh; never run on a hosted runner (branch not shown pushed). |
| 2 | SC2: ring-3 program keeps an XMM value across a yield; route recorded; SAFE-01 ledger | VERIFIED (provisional: TCG) | Route recorded in `unsafe-ledger.md` "FPU/SSE route": safe OSTD route (`FpuContext` save/load in `UserModeHooks::pre_user_run`, `thread.rs:320`), kernel `unsafe` table empty, no OSTD patch, no Redox re-score. Ktests passed: `xmm_survives_yield_between_two_threads` (200 yields x 2 threads, distinct 16-register patterns), `xmm_swap_disabled_is_detected` (negative control: swap off makes the test fail), `xmm_survives_redirect` (handler stub captures xmm0-15 before Rust runs), `fpu_context_default_mxcsr_is_0x1f80`. Boot: `fpu preemption: ticks [26, 27], exits ok true`. Userland `unsafe` is six ledgered rows U-01..U-06; `unsafe_check.py` and its self-test pass; `#![deny(unsafe_code)]` on kernel, abi, libsys roots. Limits: single CPU; SSE only; "signal redirect" is checked to the capture point only, since `sigreturn` is Phase 2 (KERN-11). |
| 3 | SC3: fuzz-style ktest feeds malformed numbers, pointers and lengths, gets errno, never a panic | VERIFIED as written (weak assertions, see WR-06) | `syscall.rs` `fuzz_syscalls_never_panic`: targeted table with exact expected errno per case (ENOSYS, EFAULT, EINVAL, EBADF, EINTR) plus 100,000 seeded random calls across all 7 syscalls, asserting `v >= 0 || KNOWN.contains(&-v)`. Also `decode_rejects_bad_numbers_pointers_and_lengths` (exact), ring-3 `badsys_gets_errno_from_ring3`, `futex_on_a_kernel_address_is_efault_not_a_panic`. Passes. It drives `dispatch` from a kernel thread and checks no post-state, so leaks on the error path are not detected. |
| 4 | SC4: server parks and replies later; interrupted `call` sends Cancel and stays blocked until reply; interrupted `read` loses no data; fail-point ktest on `block_until`; 1000x loop | VERIFIED (provisional: TCG timing) | `server_parks_a_request_and_replies_later` runs ring-3 `park_srv` (answers B before A, `reply_recv` receives C, reused handle gives EBADF). `interrupted_call_waits_for_reply`, `queued_call_interrupt_is_not_started`, `only_the_kernel_sends_cancel`, `reply_to_dead_waiter_is_dropped`, `never_answering_server_keeps_caller_blocked`, `blocked_recv_still_aborts_at_once` pass. `interrupted_read_loses_no_data` runs ring-3 `reader` through both branches (commit: server has the call, Cancel, chunk arrives; not-started: call retaken) and checks all 64 bytes. `block_until_failpoints_resolve_wake_versus_interrupt` forces 3 points x 3 actions; `block_until_interleavings_1000x` is 1000 seeded cases and asserts zero timeouts. KERN-12: `Waiter::new_pair` / `wait_until_or_cancelled` appear once, inside `Thread::block` (`thread.rs:274-283`); every wait site (ipc.rs 265/295/332/352, futex.rs:146, thread.rs:192) goes through `block_until` / `block_until_committed`. |
| 5 | SC5: server tells forked siblings apart by stamped tid and tag; multi-page cross-space copy matches incl. first CoW write; capability cannot exceed attenuated rights; >64 caps install and derive; null-IPC recorded with N, median, p99 | VERIFIED (null-IPC number provisional: TCG) | `siblings_are_distinguished_by_tid_and_tag`: two CoW-cloned images, threads tagged 1 and 2, server records `(credential, tid, tag)` and asserts they differ and match. `multi_page_bulk_round_trip_matches_after_clone`: 3*4096+123 bytes each way in a cow_clone child, parent `.bss` pages asserted still zero (kernel's first write broke CoW); also short-buffer truncation. `vm.rs` ktests cover first write after clone, two CoW pages, unmapped EFAULT, over-budget ENOMEM. KERN-06: no kernel user-memory writer outside `vm.rs` (test code aside). `cap_transfer_attenuates_through_a_chain` (ring-3 `cap_recv`: forwarded mask cannot widen, receiving with no RECV is EPERM, forwarding without GRANT is EPERM, 5 caps is EINVAL), `send_caps_checks_slot_and_grant`, `cap_install_is_all_or_none`. `cspace_grows_past_64_with_install_and_derive`, `revoke_reaches_copies_above_slot_64`, `more_than_64_caps_arrive_by_message` (100 caps received). IPC-06 recorded in `docs/architecture/ipc-measurements.md`: N=10,000, median 5,429,008, p99 7,537,376, max 19,566,536 cycles, labelled TCG/provisional. |
| 6 | Derived: no entry point reachable from ring 3 panics the kernel (T-01-12 mitigation, SC3 intent, core-value confinement) | FAILED (BLOCKER) | CR-01, see Gaps. |

**Score:** 5/6 truths verified (0 behavior-unverified)

### Required Artifacts (spot level; plan `must_haves` not run through `verify.artifacts`)

| Artifact | Status | Details |
|----------|--------|---------|
| `userland/x86_64-aok-user.json`, `userland/build.sh`, `userland/libsys`, `userland/abi`, `userland/tests` (18 bins) | VERIFIED | Exist, substantive, loaded by 40+ `user_image!` call sites in ktests and boot checks |
| `kernel/src/syscall.rs` typed decode (`UserPtr`, `UserSlice`, `Handle`) | VERIFIED | Wired into `run_ctx` (`user.rs`) before `dispatch` |
| `kernel/src/vm.rs` `copy_to_user` / `copy_from_user` | VERIFIED | Sole kernel writer into user memory outside tests |
| `kernel/src/thread.rs` `block_until` + failpoints | VERIFIED | Sole blocking primitive |
| `kernel/src/ipc.rs` reply handles, Cancel, caps, bulk, tid/tag | VERIFIED | Exercised by ring-3 bins |
| `kernel/src/cap.rs` growable CSpace | VERIFIED | Past 64 slots, bounded by `MAX_SLOTS` |
| `tools/unsafe_check.py`, `docs/architecture/unsafe-ledger.md` | VERIFIED (blind spots WR-08) | Run and self-test pass |
| `scripts/check.sh`, `.github/workflows/check.yml` | VERIFIED locally, weak (WR-07) | Local run passed; CI never run |
| `docs/architecture/ipc-measurements.md` | VERIFIED | Exists with N, median, p99 and TCG label |

### Requirements Coverage

| Requirement | Description (short) | Status | Evidence |
|-------------|---------------------|--------|----------|
| KERN-01 | FPU/SSE saved per thread across switches and signal delivery | SATISFIED (TCG; redirect checked to the capture point) | SC2 |
| SAFE-01 | `unsafe` contained, ledgered, CI grep | SATISFIED (checker blind spots WR-08 latent) | Ledger U-01..U-06, checker + self-test green |
| KERN-02 | Typed syscall ABI + fuzz ktest | SATISFIED as written; CR-01 is a non-syscall path | SC3 |
| KERN-03 | `no_std` user crate, custom target, flat image | SATISFIED | SC1 |
| KERN-05 | CSpace past 64 | SATISFIED | SC5 |
| KERN-06 | Single `copy_to_user` breaks CoW | SATISFIED | `vm.rs` ktests |
| KERN-12 | One `block_until`, fail-point ktest, 1000x | SATISFIED | SC4 |
| IPC-01 | call, deferred reply, `reply_recv` | SATISFIED | `park_srv` |
| IPC-02 | Cancel-then-wait; abort-at-once for `recv` | SATISFIED (WR-01/WR-02 edge cases) | SC4 |
| IPC-03 | Stamped tid and tag | SATISFIED (tag set by `Thread::new_tagged`; `thread_create` is Phase 2 KERN-04) | SC5 |
| IPC-04 | Bulk kernel copy, CoW break on destination | SATISFIED | SC5 |
| IPC-05 | Cap transfer with attenuation | SATISFIED for the generic kernel mechanism; the "vfs through exec" scenario cannot exist until Phases 3-4 | SC5 |
| IPC-06 | Null-IPC measured with N, median, p99 | SATISFIED, PROVISIONAL (TCG) | ipc-measurements.md |
| TEST-06 | ktests and boot budget check stay green; clippy/fmt in CI | SATISFIED locally; CI run unverified; gate weak (WR-05, WR-07) | check.sh green |

No orphaned requirements: the 14 IDs in ROADMAP Phase 1 all map above. REQUIREMENTS.md still shows 13 of them unchecked/Pending; the orchestrator should update the checkboxes only after gap closure.

### Review findings weighed against the criteria

| Finding | Bears on | Verdict |
|---------|----------|---------|
| CR-01 | T-01-12, SC3 intent, core value | **Gap (BLOCKER).** SC3 and KERN-02 are literally about syscall numbers, pointers and lengths, and those pass. But plan 01-03 declared T-01-12 ("any handler reachable from ring 3") mitigated with "no panic", and the page-fault path is a handler reachable from ring 3. A one-instruction ring-3 kernel DoS breaks the confinement the project exists to show. Not deferred: no Phase 2 plan touches it. Judgment call: a strict reading of SC3 would pass it; I fail it because the fix is about five lines plus one test and the invariant is the phase's stated purpose. |
| WR-01, WR-02 | SC4 / IPC-02 | Warning. Need two receivers on one endpoint (WR-01) or a server write-back failure racing an interrupt (WR-02). The tests that exist pass; the failure modes are untested. Fix before relying on multi-threaded servers (Phase 3+). |
| WR-03 | SC3 intent (panic from ring-3-reachable path) | Warning. Needs physical memory exhaustion, not a bounded budget; same class as CR-01 but not directly attacker-triggerable while budgets are below RAM. |
| WR-04 | TEST-06 / budget claim | Warning. B2-era tick sampling; the boot check cannot see it. Not a Phase 1 criterion; no later phase claims it. |
| WR-05 | TEST-06 | Warning. The boot check cannot fail on under-charging, so "green" is weaker than it reads. Observed this run: child 61 ticks of an expected ~60, FPU ticks [26, 27]. |
| WR-06 | SC3 | Warning. Literal SC3 holds; leak-on-error is unchecked. |
| WR-07 | TEST-06 | Warning. `|| true` plus no timeout means the boot gate can hide a crash or hang. This run's log did contain both pass strings and no FAILED. |
| WR-08 | SAFE-01 | Warning. Latent blind spots; nothing evades the checker today. |
| IN-01..IN-06 | none | Informational. |

### Anti-Patterns Found

No `TBD`, `FIXME`, `XXX`, `TODO`, `todo!` or `unimplemented!` in phase files. Frame-allocation `unwrap`s are WR-03. No stubs found in the artifacts above.

### Behavioral Spot-Checks

Covered by the single `check.sh` run; no extra named-test runs were needed.

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full gate | `sh scripts/check.sh` | exit 0, 67 ktests passed, both boot checks passed | PASS (TCG) |
| CR-01 panic | not run | confirmed by source reading only | NOT EXECUTED |

### Probe Execution

No probes declared by the phase plans and none found under `scripts/*/tests/probe-*.sh`. SKIPPED.

### Provisional Items (TCG, not hardware)

| Item | Why provisional |
|------|-----------------|
| IPC-06 null-IPC median/p99 | TSC under TCG is not tied to executed work. Figures vary run to run: ipc-measurements.md 5,429,008 / 7,537,376; my run 5,791,017 / 8,373,944 (N=10,000, TSC 4.42 GHz). Both are an order of magnitude or more above what a microkernel should cost on hardware. Ktest asserts only `n == 10_000` and `median > 0`. Re-measure on a KVM host before PERF-02 uses it. |
| CPU budget boot check (33 periods: root 219 ticks, child 61) | Tick-driven; TCG lacks `tsc-deadline`. Upper bounds are real; lower bounds weak (WR-05). |
| FPU timer-preemption boot check (ticks [26, 27]) | Same. The yield-path FPU tests are deterministic and do not depend on timing. |
| Fail-point and 1000x `block_until` tests | Deterministic interleavings; the wall-clock deadlines (+50 jiffies) are the only timing dependence. |
| Hosted CI | No hosted run exists; whether the runner has KVM is unknown, and in `--no-kvm` mode CI skips ktests and boot checks entirely. |

## Gaps Summary

One blocker. CR-01: a ring-3 write to a kernel-half address panics the kernel in `handle_cow_fault`. The five roadmap success criteria are otherwise met by tests that exist and pass in the check run, with the TCG caveat. After the CR-01 fix and its regression test, re-verification should flip this to `passed` (or `human_needed` if the user wants KVM re-measurement of IPC-06 as a gate). Recommended in the same pass because they are cheap and sit on the same boundary: WR-03 (return an error instead of unwrapping frame allocation) and WR-07 (timeout and exit status on the boot gate).

---

_Verified: 2026-10-26_
_Verifier: Claude (gsd-verifier)_
