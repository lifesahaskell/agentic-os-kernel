---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 08
subsystem: kernel-vm
tags: [cow, page-fault, ring3, ktest, gap-closure, rust, ostd]

requires:
  - phase: 01-userland-syscall-abi-and-ipc-v2
    provides: handle_cow_fault, run_ctx fault path, ktest harness and check.sh gate (01-07)
provides:
  - handle_cow_fault declines addresses at or above USER_TOP and a rejected cursor instead of panicking (CR-01)
  - AgentError::NoMemory (-> ENOMEM) and LoadError::NoMemory; no non-test frame-allocation unwrap left in vm.rs or user.rs (WR-03)
  - kpush sentinel ring-3 image and two regression ktests
affects: [02-procmgr, 01-verification]

actuals:
  tokens: 2174
  tasks: 3
  commits: 2
plan_head_before: dd9ce23a2148165af352faebd4002ecd2822b246
plan_head_after: 2ee2116f60dbc95163672536eec202a086640276

tech-stack:
  added: []
  patterns:
    - "Ring-3 store without unsafe: start an ordinary image with rsp in the kernel half, so _start's call-push is the faulting store"
    - "Allocate before charge in the CoW break, so a failed allocation charges nothing and a refused charge drops the frame"

key-files:
  created:
    - userland/tests/src/bin/kpush.rs
  modified:
    - kernel/src/vm.rs
    - kernel/src/user.rs
    - kernel/src/agent.rs
    - kernel/src/syscall.rs

key-decisions:
  - "CR-01 is closed by a USER_TOP guard plus a fallible cursor_mut in handle_cow_fault; the remaining query and jump unwraps stay because the guard keeps the page inside the user page table"
  - "Exit::Budget stays the ring-3 outcome for an allocator failure in the CoW break (shortcut comment added); futex.rs:88 keeps its blanket FutexError::Budget mapping (RR-03)"

requirements-completed: [KERN-02, SAFE-01, KERN-06, TEST-06]

duration: not recorded (about one hour, estimate; dominated by three ktest builds and one full gate under TCG)
completed: 2026-10-26
status: complete
---

# Phase 1 Plan 08: Gap closure for CR-01 and WR-03 Summary

**A ring-3 write fault at or above `USER_TOP` is now a declined CoW fault that ends as `Exit::Fault` for that thread only, and the three frame-allocation unwraps in vm.rs and user.rs return `NoMemory` errors instead of panicking.**

## Plan base and commits

- Plan base: `dd9ce23a2148165af352faebd4002ecd2822b246` (`git rev-parse HEAD` before Task 1; Task 1 made no commit).
- `40a9da1` fix(01-08): decline CoW faults outside the user half (Task 2: tests from Task 1 plus the fix, one commit)
- `2ee2116` fix(01-08): propagate frame allocation failure instead of panicking (Task 3)
- `git rev-list --count dd9ce23..HEAD -- kernel userland` is 2 (measured), so no red commit exists. No Co-Authored-By or Claude-Session trailers on either commit.

## Red evidence (Task 1, never committed)

The two new tests were written and formatted, then run against HEAD code before the fix. Log: `aok-01-08-red.log` in the session scratchpad (not in the repo). The ktest runner caught both panics, so the run did not abort: `67 passed; 2 failed`.

```
---- /work/kernel/src/user.rs:454:4 - ring3_store_to_kernel_half_is_a_fault ----
[caught panic] Panicked at /work/kernel/src/vm.rs:66:69
called `Result::unwrap()` on an `Err` value: AccessDenied

---- /work/kernel/src/vm.rs:269:4 - cow_fault_outside_user_half_is_declined ----
[caught panic] Panicked at /work/kernel/src/vm.rs:66:69
called `Result::unwrap()` on an `Err` value: AccessDenied
```

vm.rs:66 is the `cursor_mut(..).unwrap()` in `handle_cow_fault`. The rsp route worked as planned: the ring-3 push reached `handle_cow_fault` with the raw `pf.addr` (no Exit::Exception, no OSTD "Cannot handle user CPU exception", no hang), so the Stop-and-ask condition did not trigger.

Before Task 2 committed, the snapshot checks passed: `sha256sum -c` reported OK for `kernel/src/user.rs` and `kpush.rs`, and the hash of the vm.rs ktest module matched the red run. The tests that ran red are byte-identical to the committed ones.

## Green evidence

- ktest count: 67 passed before, 69 passed after (`69 passed; 0 failed`), measured at the Task 2 commit (`aok-01-08-green.log`) and again on the Task 3 tree (`aok-01-08-wr03.log`). Zero `Panicked at` lines in either log. Both new tests report ok.
- Full gate, `sh scripts/check.sh` (no flag), `timeout 3600`, run 2026-10-26 (finished about 06:46 UTC), attempt 1 of at most 2:
  - `gate.status` 0; last line `check: all gates passed`; `unsafe_check: ok` present.
  - Boot log: `aok: fpu preemption check passed`, `aok: cpu budget check passed`, with `aok: 33 periods: root agent 218 ticks (quota 8), child 61 (quota 2)`.
  - No `Non-resettable panic` or `The panic handler panicked` line in `kernel/target/boot-check.log`; no hang; no cpu-budget flake; one attempt.
  - The gate's ktest step reported `69 passed; 0 failed`, no `Panicked at`.
- All runtime evidence is QEMU TCG: the host is a WSL2 guest with no `/dev/kvm`. This is not a KVM baseline, and the STATE.md blocker on re-running Phase 1 under KVM stays open.

## What was verified and what was not

Verified: the two CR-01 tests fail at HEAD with a vm.rs panic and pass after the fix; 67 prior ktests still pass; the full gate passes (fmt, clippy, unsafe_check, ktests, both boot checks); the region-scoped greps and source assertions in the Task 3 acceptance criteria.

Not verified:
- The three allocation-failure branches (CoW break, `load_image`, `map_stack`) never execute in any test. OSTD 0.19.0 has no frame-allocation failure injection (RR-02, accepted). They are closed by inspection plus the source assertions.
- The alloc-before-charge order has no runtime test: on a refused charge both orders leave `mem_used_pages() == 0`, so the new quota assertions pin the outcome, and the awk source gate (`alloc-first`) pins the order.
- Ring-3-reachable unwraps outside the CR-01 page-fault path were not audited. This plan's claim is the page-fault path only.
- Nothing ran under KVM.

## Deviations from Plan

**1. [Rule 3 - Blocking] Plan commands target the main checkout; run against the worktree instead**
- **Found during:** Task 1 onward
- **Issue:** The `<automated>` verify commands, and the gate start command, begin with `cd /home/rakdos/repos/agentic-os-kernel/...`, which is the main checkout, not this worktree. Run as written they would verify the main tree (worktree-path-safety step 0c). The plan itself allows "the executor's worktree root" for the snapshot steps.
- **Fix:** Ran the same commands root-relative from the worktree (`kernel/osdk.sh ...`, `sh scripts/check.sh`). `osdk.sh` and `check.sh` both derive their root from the script path, so they mounted and built the worktree. The gate was started through a one-line wrapper script in the session scratchpad that `cd`s to the worktree root, because the harness refused the braced `cd && { ...; }` form. Logs were written to the session scratchpad (`$TMPDIR` was not set) except `gate.log`/`gate.status`/`boot-check.log`, which sit in the worktree's gitignored `kernel/target`.
- **Files modified:** none
- **Commit:** n/a
- Plan defect, for the planner: write verify commands root-relative.

**2. [Rule 1 - Style] Wrapped the `// shortcut:` comment in `run_ctx`**
- **Issue:** The plan gives the comment as one long line; existing `// shortcut:` comments in user.rs wrap at the file's width.
- **Fix:** Same text, wrapped over three lines. The `// shortcut:` marker is unchanged.
- **Commit:** `2ee2116`

**Total deviations:** 2 (one blocking path issue, one cosmetic). **Impact:** none on behavior or evidence.

Also noted: the Task 1 verify's "no compile error" and "Panicked at ... vm.rs" checks were done by grepping the saved red log (`grep -n -E "Panicked at|^error|build\.sh:"` returned only the two vm.rs panics), equivalent to the plan's command.

## Residual risks and dispositions (for the orchestrator)

- **RR-01 (deferred to Phase 2, orchestrator should report):** the `cow_clone` unwraps at vm.rs (`cursor_mut`, `query`, `jump`) plus the `panic!` and `assert_eq!` on MMIO and huge pages are not reachable from ring 3 at this HEAD: no syscall clones a space and every caller is a ktest. They become reachable once Phase 2 plans 02-06 and 02-07 call `cow_clone` from procmgr spawn and fork. Phase 2 should either establish that the range stays inside the user page table or make `cow_clone` fallible.
- **RR-02 (accepted):** the three WR-03 failure branches are untested at runtime (no failure injection in OSTD 0.19.0).
- **RR-03 (accepted, unchanged from HEAD):** `kernel/src/futex.rs:88` still maps every `handle_cow_fault` error to `FutexError::Budget` (errno 12). It was not edited. An allocator failure during a futex CoW break therefore reports as `Budget`, the same merge `run_ctx` makes with `Exit::Budget`.
- **Dispositions to record in 01-REVIEW-DISPOSITION.md (not edited here):** CR-01 `fixed` in `40a9da1`; WR-03 `fixed` in `2ee2116`. WR-01, WR-02, WR-04..WR-08 and IN-01..IN-06 stay `open`.
- The 01-VERIFICATION.md truth "No entry point reachable from ring 3 can panic the kernel" is closed for the CR-01 page-fault path only. Other ring-3-reachable unwraps outside vm.rs were not audited.

## Prohibitions check

- No new `unsafe`: `grep -rwn unsafe kernel/src` prints nothing; `grep -rnE '\bunsafe\b|allow\(unsafe_code\)' userland/tests/src` prints nothing; `unsafe_check: ok`; `git diff HEAD -- userland/libsys docs/architecture/unsafe-ledger.md scripts/check.sh` is empty.
- No commit left a ktest failing; the red run was evidence only.
- check.sh and the cpu-budget gate were not edited, loosened, wrapped or retried.
- No edits to `userland/out`, `wiki/graph.json` or `raw/`. No fix for WR-01, WR-02, WR-04..WR-08 or IN-01..IN-06. STATE.md and ROADMAP.md untouched.

## Known Stubs

None.

## Threat Flags

None: no new endpoint, auth path, file access or schema surface. T-01-32 through T-01-35 mitigations are in place; T-01-36 stays transferred as RR-01.

## Self-Check: PASSED

- Files exist: `userland/tests/src/bin/kpush.rs`, `kernel/src/vm.rs`, `kernel/src/user.rs`, `kernel/src/agent.rs`, `kernel/src/syscall.rs`.
- Commits `40a9da1` and `2ee2116` are ancestors of HEAD (`git merge-base --is-ancestor`).
- `git rev-list --count dd9ce23..HEAD` is 2 before this SUMMARY commit, matching `commits: 2`.
- Working tree before the SUMMARY commit: only this file staged.
