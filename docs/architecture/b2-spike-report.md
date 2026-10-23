# B2 spike report: capability-microkernel primitives on Asterinas OSTD

Status: complete, 2026-10-23. Milestone: B2 in [scoping.md](scoping.md). Design:
[option-b-microkernel.md](option-b-microkernel.md) section 2. Code: [`kernel/`](../../kernel/).

## Verdict

**OSTD does not fight the object model, at least for the primitives B2 covers.** All five were
built on OSTD 0.19.0's public API in ~640 lines of safe Rust (`#![deny(unsafe_code)]`), with
no OSTD patches. All 15 kernel tests pass in QEMU/KVM on x86-64. The Redox-fork fallback is not
triggered. The recommendation is to continue on OSTD into B3, carrying the friction list below.

Time box: no number was set. The spike took one working session.

## What was built

| Primitive | Module | OSTD API it rests on | Tests (ktest, QEMU) |
|---|---|---|---|
| Boots in QEMU, x86-64 | `lib.rs` | `#[ostd::main]`, `power::poweroff` | `cargo osdk run` prints and exits 0 |
| Capability space: rights masks, attenuating derive, per-grant revoke via chained forwarders | `cap.rs` | none (plain Rust, `SpinLock`) | `derive_never_amplifies`, `derive_requires_grant`, `revoke_kills_one_grant_and_its_descendants`, `cspace_slots` |
| Agent object: credential (`agent_id`, `on_behalf_of`, parent), root cspace, memory budget carved from parent | `agent.rs` | none | `child_gets_attenuated_subset_and_carved_budget`, `child_cannot_regrant_without_grant_right` |
| IPC endpoints: kernel-stamped credential, rights-checked through the cspace, bounded queue | `ipc.rs` | `WaitQueue` | `kernel_stamps_sender_credential`, `rights_are_enforced_and_queue_is_bounded`, `send_wakes_blocked_receiver` |
| Thread interrupt and redirect: abort a blocked IPC with `Interrupted` (EINTR), then rewrite rip/rdi before user code resumes | `thread.rs`, `user.rs` | `Waiter::wait_until_or_cancelled`, `Waker::wake_up`, `UserModeHooks::has_kernel_event` | `interrupt_aborts_blocked_recv`, `pending_interrupt_fails_recv_without_blocking`, `timer_interrupt_redirects_a_running_thread` |
| Lazy CoW address-space clone plus write-fault handler that charges the faulting agent | `vm.rs`, `user.rs` | `VmSpace::cursor_mut`, `CursorMut::{query, protect_next, map, unmap}`, `PageFlags::AVAIL2`, `Frame::reference_count`, `UserContext::take_exception` | `cow_clone_isolates_writes_and_charges_the_writer`, `cow_fault_over_budget_stops_the_thread`, `write_to_read_only_page_is_a_fault_not_cow` |

The user-mode tests run real ring-3 code: a 50-byte hand-assembled program, checked with
`objdump`. The redirect test spins in `jmp .` until a timer callback interrupts the thread. Its
exit code is the interrupted pc, which only the redirect path can produce.

## Verified

- `cargo osdk test`: 15/15 pass. `cargo fmt --check` and `cargo osdk clippy` are clean.
- `cargo osdk run` boots, prints, and powers off with exit 0.
- The security tests can fail. With the GRANT check removed from `derive` and `Forwarder::revoke`
  turned into a no-op, 3 tests failed: `derive_requires_grant`,
  `revoke_kills_one_grant_and_its_descendants`, and `child_cannot_regrant_without_grant_right`.
  Restoring the file restores 15/15.

## Not verified, or not built

- **SMP.** Every test runs on `-smp 1` without preemption. Two gaps follow from that:
  - `cow_clone` dispatches its TLB flush but never waits for remote CPUs.
  - Interrupt racing with block was reasoned about (one lock covers both the waker and the
    redirect), not stress-tested.
- **Address-space switch on task switch.** OSTD leaves activating a task's `VmSpace` to the
  kernel, through `inject_post_schedule_handler`. Each test runs one user space per task, so
  this path does not exist yet.
- **CPU budget.** Only the memory budget is enforced. The CPU budget needs a custom scheduler
  (`inject_scheduler`) because OSTD's default FIFO scheduler has no budget hook. Whether the
  `Scheduler` trait exposes enough to enforce a budget is unchecked.
- **Syscall surface for capabilities.** User mode has only `exit`. The cap and IPC operations
  are kernel functions, called by the tests directly.
- **Capability-gated clone.** `cow_clone` takes a `&VmSpace`. An address-space object type in
  `cap::Object` is needed before procmgr can call it on another process's behalf.
- **Exceptions as messages** (shortcut in `user.rs`) and **futex**. Neither was in B2's scope.
  Both are mandatory in option B section 2.
- **Teardown.** Budget carved out for a child is never returned to the parent, and caps held by a
  dead agent are not cleaned up (shortcut in `agent.rs`).

## Friction to carry into B3

1. **Kernel writes into user memory skip the CoW path.** `VmWriter` user copies are fallible: a
   kernel write into a CoW page returns an error rather than faulting into `handle_cow_fault`.
   Any IPC that copies into a receiver's buffer must break CoW first, or catch the error, run the
   handler, and retry. This is inferred from the API and not exercised.
2. **Unmapped frames are freed after an RCU grace period.** As a result,
   `Frame::reference_count` can overcount for a while after the other space has copied away. The
   "last owner takes the frame over" fast path is therefore opportunistic: a parent may copy once
   more than it needs to. The behaviour is correct but not optimal. The test asserts only the
   writer's charge, which is deterministic.
3. **Redirect latency for a thread running in user mode is up to one timer tick.**
   `has_kernel_event` is polled only after an interrupt lands in user mode. A blocked thread is
   redirected immediately. For POSIX signals this is adequate. *(interpretation)*
4. **OSTD is 0.x and does not state API stability.** The version is pinned in `Cargo.toml` and
   the build image in `kernel/Dockerfile`. Each OSTD upgrade is a deliberate task.

## Reproduce

```
cd kernel
docker build -t agentic-os-kernel-dev:0.19.0 .
./osdk.sh cargo osdk test
./osdk.sh cargo osdk run
```

`osdk.sh` passes only `/dev/kvm` into the container, not `--privileged`.

## What would reverse the verdict

The B3 failure signal for this base is either of the following needing an OSTD patch:

- the CPU-budget scheduler;
- the per-task address-space switch.

If both can be built on the public API, OSTD stays. If either needs `unsafe` in our crate or a
fork of OSTD, re-score the Redox fork in [ADR 0002](adr/0002-rust-microkernel.md).
*(interpretation)*
