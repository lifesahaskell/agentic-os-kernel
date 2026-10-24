---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 03
subsystem: syscall-boundary
tags: [rust, syscall, decode, copy-on-write, user-copy, fuzz, ostd, errno]

requires:
  - phase: 01-01
    provides: userland workspace, flat image loader, ring-3 test programs, user::testkit
  - phase: 01-02
    provides: unsafe ledger, scripts/check.sh gates, per-thread FpuContext
provides:
  - kernel/src/syscall.rs typed decode layer (Syscall, decode, dispatch, Errno, UserPtr, UserSlice, Handle)
  - vm::copy_to_user / copy_from_user, the only kernel path into user memory (CoW break and retry)
  - 100,000-case seeded syscall fuzz ktest and a ring-3 hostile-argument program
affects: [01-04, 01-05, 01-06, 01-07, phase-2, phase-3]

plan_head_before: 18db3cdb79bedc3fe21a8ff7c6f2ddff8d0ae519
plan_head_after: d516936f3e8d3e19ef8efff5b3267515cdf9957c

actuals:
  tokens: 9700
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - ring-3 registers are decoded once into a typed Syscall; handlers never see raw values
    - kernel writes to user memory go through vm::copy_to_user, which resolves CoW itself and retries
    - IpcArgs crosses the boundary as 168 little-endian bytes (to_bytes / from_bytes), no unsafe, no zerocopy

key-files:
  created:
    - kernel/src/syscall.rs
    - userland/tests/src/bin/badsys.rs
  modified:
    - userland/abi/src/lib.rs
    - kernel/src/lib.rs
    - kernel/src/user.rs
    - kernel/src/ipc.rs
    - kernel/src/vm.rs

key-decisions:
  - "Errno holds a positive value; dispatch negates it. Handle bound is a local const (65,536) so cap.rs stays untouched for plan 01-04"
  - "OSTD needs a registered user page-fault handler or any kernel fault at a user address panics; vm registers one that declines, and the copy loop resolves CoW itself with the agent to charge"
  - "A futex wait with a timeout (val2 != 0) stays ENOSYS, as before"

requirements-completed: [KERN-02, KERN-06]

coverage:
  - id: D1
    description: "Every ring-3 syscall is decoded into a typed Syscall before a handler runs; unknown number or futex op is ENOSYS, bad pointer EFAULT, oversize length EINVAL, out-of-range slot EBADF"
    requirement: "KERN-02"
    verification:
      - kind: unit
        ref: "aok::syscall::test::decode_rejects_bad_numbers_pointers_and_lengths"
        status: pass
    human_judgment: false
  - id: D2
    description: "The first kernel write into a CoW-shared page succeeds, lands only in the target space, charges the target agent one page; spanning two CoW pages breaks both; unmapped and read-only non-CoW pages give EFAULT; an over-budget break gives ENOMEM"
    requirement: "KERN-06"
    verification:
      - kind: integration
        ref: "aok::vm::test::{first_kernel_write_after_cow_clone_breaks_cow, copy_to_user_spanning_two_cow_pages_breaks_both, copy_to_user_unmapped_is_efault, copy_to_user_over_budget_is_enomem}"
        status: pass
    human_judgment: false
  - id: D3
    description: "100,000 seeded random syscalls plus a targeted malformed-input table go through decode and dispatch without a kernel panic; every result is an exit, non-negative, or a negative errno from the abi list"
    requirement: "KERN-02"
    verification:
      - kind: integration
        ref: "aok::syscall::test::fuzz_syscalls_never_panic -> 'aok: fuzz n=100000 enosys=40671 efault=8940 einval=1707 ebadf=13404 eintr=49'"
        status: pass
    human_judgment: false
  - id: D4
    description: "From ring 3, a kernel-address pointer, an unmapped pointer, a pointer straddling the user top, an unknown number, an unknown futex op, a kernel-address futex and an out-of-range slot each return the expected errno"
    requirement: "KERN-02"
    verification:
      - kind: integration
        ref: "aok::user::test::badsys_gets_errno_from_ring3 (Exit::Code(0))"
        status: pass
    human_judgment: false

duration: 70min
completed: 2026-10-24
status: complete
---

# Phase 1 Plan 03: Syscall decode layer and user-copy primitives Summary

**Every ring-3 syscall is decoded into a typed `Syscall` in `kernel/src/syscall.rs` before any handler runs, and every kernel write into user memory goes through `vm::copy_to_user`, which breaks copy-on-write itself and retries.**

## Accomplishments

- `syscall.rs`: `Errno` (with `From<CapError | IpcError | FutexError | AgentError>`), `UserPtr<T>`, `UserSlice`, `Handle`, `Syscall`, `decode`, `Outcome`, `dispatch`. The futex op parsing moved here from `user.rs`; `sys_futex` and the `FUTEX_*` consts are gone, and the `run_ctx` loop reads `rax` plus six registers, calls `decode`, and dispatches. `ipc::sys_call` now takes `Handle` and `UserPtr<IpcArgs>` and returns `Result<(), Errno>`; the private `errno_of` became `From<IpcError> for Errno`.
- `vm.rs`: `copy_to_user` writes in a loop; on a fault it calls `handle_cow_fault` at the fault address and retries, at most 2 faults per page, then `EFAULT`. `Ok(false)` from `handle_cow_fault` (not a CoW page) is `EFAULT`; a refused budget is `ENOMEM`. `copy_from_user` is a single fallible read.
- `IpcArgs` is encoded for the copies as 21 little-endian u64 (`to_bytes` / `from_bytes` in `abi`, 168 bytes, field order equals the `repr(C)` layout). The reply write-back copies only the 32 bytes at `offset_of!(IpcArgs, words)`. No `unsafe`, no new crates.
- 40 ktests pass (33 prior plus 7: decode, fuzz, badsys, four in `vm`). Fuzz line: `aok: fuzz n=100000 enosys=40671 efault=8940 einval=1707 ebadf=13404 eintr=49`. Boot checks still print both `aok: fpu preemption check passed` and `aok: cpu budget check passed`.

## TDD record (task 1)

- **RED:** `decode_rejects_bad_numbers_pointers_and_lengths` with `decode` temporarily returning `ENOSYS` for every input. The run printed `test aok::syscall::test::decode_rejects_bad_numbers_pointers_and_lengths ... FAILED`. The unrelated tests that depend on real syscalls then hung (user programs could not `exit`), so that run produced no `test result:` summary and no machine-readable record; `gsd_run check tdd-red-evidence` was not run. Semantic assessment: the target test executed and failed, the first assertion (`0xFFFF` is `ENOSYS`) passes under the stub and the later ones fail, which is the intended reason. Not a build or setup failure.
- **GREEN:** the real `decode`, and the test passed with all 40 ktests.
- RED was not committed separately (the stub needed the whole layer to compile), so the per-task commits are cut by file group, below.

## Task Commits

1. **Tasks 1 and 2 (decode layer, user-copy primitives, `ipc` migration)** - `e8423be` (feat). Both tasks edit `ipc.rs` and the copy ktests live in `vm.rs`, so they landed together.
2. **Task 3 (fuzz ktest, `badsys` program and its ktest)** - `f8a85fa` (test)
3. **Userland clippy fix** - `d516936` (fix): `chunks_exact(8)` in `IpcArgs` encoding replaced with `as_chunks::<8>()`.

## OSTD behaviour that differed from RESEARCH

RESEARCH assumed the fallible `VmWriter`/`VmReader` return `Err((PageFault, copied_len))` on their own. They do, but only after the kernel registers a user page-fault handler: `ostd::mm::fault::handle_user_page_fault` does `USER_PAGE_FAULT_HANDLER.get().expect("a page fault handler is missing")`, so the first fault at an unmapped user address from kernel mode (`SYS_CALL` with pointer `0x1000`) panicked and the VM triple-faulted. QEMU under `--no-reboot` exited 0 with no `test result:` line, which is how the fuzz test first showed it. Fix: `vm::decline_kernel_user_faults()` calls `inject_user_page_fault_handler` with a handler that returns `Err(())`, from both copy functions (idempotent `call_once`, so no init-order dependency). The handler declines on purpose: the fault carries no agent, so the CoW break and its charge happen in the copy loop. With a declining handler, `rep movsb` faults report an exact copied length, which the loop uses as the fault address.

## Deviations from Plan

**1. [Rule 1 - Bug] Missing OSTD user page-fault handler** (above). Found during Task 3 fuzzing; the fix is in `vm.rs` and shipped in `e8423be`.

**2. [Rule 3 - Blocking] Plan `<automated>` commands name the main checkout.** They `cd /home/rakdos/repos/agentic-os-kernel/kernel`, which would verify the wrong tree from a worktree. I ran the equivalent worktree-relative commands (`cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'`) and did not rewrite the plan file.

**3. [Rule 3 - Blocking] `sh scripts/check.sh` is refused by the worktree sandbox** (its `cd "$(dirname "$0")/.."` reads as an escape). I ran its gates one by one from the worktree root with the same commands: `unsafe_check --self-test`, `unsafe_check`, userland build/fmt/clippy, kernel fmt/clippy, `cargo osdk test`, `cargo osdk run` with the boot-line greps. All passed, but the script itself was not run, so its final line `check: all gates passed` was not observed.

**4. [Rule 1 - Bug] Userland clippy** rejected `chunks_exact(8)` in the new `IpcArgs::to_bytes/from_bytes`; fixed in `d516936`.

**5. Commit split.** Tasks 1 and 2 are one commit (shared `ipc.rs`), and the RED state was not committed. The intermediate commit `e8423be` contains `syscall.rs` without the fuzz ktest, and was not built on its own; only the final tree and `f8a85fa`'s predecessor state were built and tested.

## Issues Encountered

- A stubbed `decode` makes every ring-3 test hang, so the RED run had to be killed (stopped its own container by id) after the target test had reported FAILED.
- `grep -rn 'writer(' kernel/src | grep -v '^kernel/src/vm.rs:'` still prints one line: the pre-existing test name `cow_clone_isolates_writes_and_charges_the_writer()` in `user.rs`. No `.writer(` call is outside `vm.rs`.
- `cargo osdk test <name>` filters on the bare function name (`fuzz_syscalls_never_panic`), not a substring or module path.

## Verified versus not verified

- Verified (run here): 40 ktests, boot checks, `unsafe_check` self-test and tree check, userland build/fmt/clippy, kernel fmt/clippy, all plan acceptance greps (decode 1, `sys_futex` 0, `syscall::decode` 1, `FUTEX_PRIVATE_FLAG` 0, `copy_to_user` defs 1, ipc use 2, seed 1, fuzz fn 1, no `unsafe` in `userland/tests/src`).
- Not verified: `scripts/check.sh` as one script (see deviation 3); multi-CPU behaviour; the "max 2 faults per page then EFAULT" bound is never reached by a test (it needs a racing `cow_clone`); only 49 of 100,000 fuzz cases reach a blocking path (`eintr`), so IPC and futex blocking are lightly fuzzed; the intermediate commit `e8423be` in isolation.

## Known Stubs

None. The `FutexWait` timeout stays `ENOSYS` by design (`shortcut:` comment in `decode`, as in the code it replaced).

## Threat Flags

None new. Pointer, length and slot validation now happens at one boundary (`decode`) and in `copy_*`; no new endpoints or trust boundaries. The `unsafe` count is unchanged (kernel table empty).

## Self-Check: PASSED

- Files present: `kernel/src/syscall.rs`, `userland/tests/src/bin/badsys.rs`, `userland/out/badsys.bin` (built, untracked output).
- Commits `e8423be`, `f8a85fa`, `d516936` are ancestors of HEAD (`git rev-list --count` from the base is 3).
- `grep -rlE '\bunsafe\b|allow\(unsafe_code\)' kernel/src` prints nothing.
