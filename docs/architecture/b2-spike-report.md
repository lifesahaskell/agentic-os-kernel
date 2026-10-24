# B2 spike report: capability-microkernel primitives on Asterinas OSTD

Status: complete, 2026-10-23, including the two B3 entry checks (CPU-budget scheduler, per-task
address-space switch; see [the last section](#what-would-reverse-the-verdict)). Milestone: B2 in [scoping.md](scoping.md). Design:
[option-b-microkernel.md](option-b-microkernel.md) section 2. Code: [`kernel/`](../../kernel/).

## Verdict

**OSTD does not fight the object model, at least for the primitives B2 covers.** All five were
built on OSTD 0.19.0's public API in ~640 lines of safe Rust (`#![deny(unsafe_code)]`), with
no OSTD patches. The two pieces named as the B3 failure signal, a CPU-budget scheduler and the
per-task address-space switch, were then built the same way: the crate is now ~1000 non-test
lines, still with no `unsafe` and no patch. All 17 kernel tests and the boot-time budget check
pass in QEMU/KVM on x86-64. The Redox-fork fallback is not triggered. The recommendation is to
continue on OSTD into B3, carrying the friction list below.

Time box: no number was set. The spike took one working session.

## What was built

| Primitive | Module | OSTD API it rests on | Tests (ktest, QEMU) |
|---|---|---|---|
| Boots in QEMU, x86-64 | `lib.rs` | `#[ostd::main]`, `power::poweroff` | `cargo osdk run` prints and exits 0 |
| Capability space: rights masks, attenuating derive, per-grant revoke via chained forwarders | `cap.rs` | none (plain Rust, `SpinLock`) | `derive_never_amplifies`, `derive_requires_grant`, `revoke_kills_one_grant_and_its_descendants`, `cspace_slots` |
| Agent object: credential (`agent_id`, `on_behalf_of`, parent), root cspace, memory budget carved from parent | `agent.rs` | none | `child_gets_attenuated_subset_and_carved_budget`, `child_cannot_regrant_without_grant_right` |
| IPC endpoints: kernel-stamped credential, rights-checked through the cspace, bounded queue | `ipc.rs` | `WaitQueue` | `kernel_stamps_sender_credential`, `rights_are_enforced_and_queue_is_bounded`, `send_wakes_blocked_receiver` |
| Thread interrupt and redirect: abort a blocked IPC with `Interrupted` (EINTR), then rewrite rip/rdi before user code resumes | `thread.rs`, `user.rs` | `Waiter::wait_until_or_cancelled`, `Waker::wake_up`, `UserModeHooks::has_kernel_event` | `interrupt_aborts_blocked_recv`, `pending_interrupt_fails_recv_without_blocking`, `timer_interrupt_redirects_a_running_thread` |
| CPU budget: each agent holds a quota of ticks per 10-tick period, carved from its parent like memory; a round-robin scheduler skips threads whose agent is out until the next period | `agent.rs`, `sched.rs`, `lib.rs` | `inject_scheduler`, `Scheduler`, `LocalRunQueue`, `enable_preemption_on_cpu`, `Task::data` | `out_of_budget_agent_is_skipped_until_the_next_period`, `child_gets_attenuated_subset_and_carved_budget`; boot check in `cargo osdk run` |
| Per-task address-space switch: the incoming task's `VmSpace` is activated after every context switch | `sched.rs`, `thread.rs` | `inject_post_schedule_handler`, `VmSpace::activate` | `each_task_resumes_in_its_own_address_space` |
| Lazy CoW address-space clone plus write-fault handler that charges the faulting agent | `vm.rs`, `user.rs` | `VmSpace::cursor_mut`, `CursorMut::{query, protect_next, map, unmap}`, `PageFlags::AVAIL2`, `Frame::reference_count`, `UserContext::take_exception` | `cow_clone_isolates_writes_and_charges_the_writer`, `cow_fault_over_budget_stops_the_thread`, `write_to_read_only_page_is_a_fault_not_cow` |

The user-mode tests run real ring-3 code: a 50-byte hand-assembled program, checked with
`objdump`. The redirect test spins in `jmp .` until a timer callback interrupts the thread. Its
exit code is the interrupted pc, which only the redirect path can produce.

## Verified

- `cargo osdk test`: 17/17 pass. `cargo fmt --check` and `cargo osdk clippy` are clean.
- `cargo osdk run` installs the budget scheduler, then runs a root agent (quota 8 of 10 ticks)
  and a child (quota 2), each spinning in ring 3 in its own address space for 300 ticks under
  tick preemption. Measured: over 33 periods, the child used 60-61 ticks (cap 66) and the root
  223-225 (cap 264). Both threads then exit through the redirect with their pc inside their own
  spin loop, and the kernel powers off with exit 0. It exits 1 if a cap is exceeded or either
  exit is wrong.
- Both new pieces can fail:
  - With the post-schedule handler replaced by a no-op,
    `each_task_resumes_in_its_own_address_space` fails with `Fault(0x40000f)`: the first thread
    resumed under the second one's page table and faulted fetching the instruction after its
    `sched_yield`.
  - With `has_cpu` always true, the boot check reports root 154 and child 148 ticks (an even
    split) and exits 1, and two ktests fail.
  - Restoring both files restores 17/17 and exit 0.
- The security tests can fail. With the GRANT check removed from `derive` and `Forwarder::revoke`
  turned into a no-op, 3 tests failed: `derive_requires_grant`,
  `revoke_kills_one_grant_and_its_descendants`, and `child_cannot_regrant_without_grant_right`.
  Restoring the file restores 15/15.

## Not verified, or not built

- **SMP.** Everything runs on `-smp 1`; the budget scheduler has one run queue pinned to the
  boot CPU (shortcut in `sched.rs`). Two further gaps follow:
  - `cow_clone` dispatches its TLB flush but never waits for remote CPUs.
  - Interrupt racing with block was reasoned about (one lock covers both the waker and the
    redirect), not stress-tested.
- **CPU budget precision and policy.** Charging is sampled at the timer tick: the task running
  when the tick lands pays for the whole tick (shortcut in `agent.rs`). The scheduler is
  round-robin with a one-tick slice and no priorities. Carved CPU, like carved memory, is never
  returned.
- **A budgeted thread with nothing else eligible keeps the CPU.** `update_current` may only ask
  for a switch when `pick_next` can return a task, so the boot path keeps a never-budgeted
  kernel task runnable as the idle fallback. A real idle task is a B3 item.
- **The budget scheduler under the ktest runner.** `inject_scheduler` panics once any task
  exists, and the ktest runner is a task, so ktests exercise the run-queue policy directly and
  the integrated scheduler is checked only by the boot check.
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
4. **A custom scheduler re-implements part of OSTD's scheduling protocol.** `enqueue` has to
   repeat the FIFO scheduler's handshake on `schedule_info().cpu`, for a task woken before it
   finished switching out, and `pick_next` must never return the running task or `None` after
   `update_current` asked for a switch. None of this is in the trait's types; it was read from
   OSTD's source. An OSTD upgrade can change it silently.
5. **OSTD is 0.x and does not state API stability.** The version is pinned in `Cargo.toml` and
   the build image in `kernel/Dockerfile`. Each OSTD upgrade is a deliberate task.

## Reproduce

```
cd kernel
docker build -t agentic-os-kernel-dev:0.19.0 .
./osdk.sh cargo osdk test
./osdk.sh cargo osdk run      # boot-time CPU budget check; exit 0 on pass
```

`osdk.sh` passes only `/dev/kvm` into the container, not `--privileged`.

## What would reverse the verdict

The B3 failure signal for this base was either of the following needing an OSTD patch:

- the CPU-budget scheduler;
- the per-task address-space switch.

If both could be built on the public API, OSTD stays. If either needed `unsafe` in our crate or a
fork of OSTD, the Redox fork in [ADR 0002](adr/0002-rust-microkernel.md) would be re-scored.
*(interpretation)*

**Result, 2026-10-23: neither needed one.** The address-space switch is a single
`inject_post_schedule_handler` callback. The scheduler is ~150 lines against `Scheduler` and
`LocalRunQueue`. Both are verified as above, with the limits listed under "Not verified". The
signal stays open for SMP: per-CPU run queues and remote TLB shootdown are the next places OSTD
could force a patch.
