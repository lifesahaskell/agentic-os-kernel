---
phase: 02-process-objects-teardown-and-timers
plan: 05
subsystem: kernel
tags: [notify, fault, exit, endpoint, space, syscall, ostd, ktest]
requires:
  - phase: 02-02
    provides: reserved kernel label range enforced in Endpoint::push, call machinery
  - phase: 02-04
    provides: Space object, thread_create, set_exit, thread credential equals Space owner
provides:
  - Per-Space notify endpoint (a Cap clone, resolved with SEND at each use)
  - NOTE_EXIT and NOTE_FAULT kernel-made messages naming tid, tag and owner credential
  - Fault delivery as a kernel-made call whose reply is the verdict (OD-06)
  - SYS_SPACE_NOTIFY 0x107 (self only while unset, slot form with SIGNAL replaces) (OD-18)
affects: [02-06, 02-08, procmgr]
tech-stack:
  added: []
  patterns:
    - "call_caps and call_note both go through call_via, which takes the enqueue closure"
    - "Notes bypass QUEUE_LEN through push_note (kernel-only); ring 3 sending the label is EINVAL"
    - "Exit order is set_exit, then note_exit, then reap"
    - "Notify capability stored as a Cap clone and resolved at each use, so revoking the original stops delivery"
key-files:
  created:
    - userland/tests/src/bin/fault_sibling.rs
  modified:
    - kernel/src/space.rs
    - kernel/src/ipc.rs
    - kernel/src/thread.rs
    - kernel/src/user.rs
    - kernel/src/syscall.rs
    - userland/abi/src/lib.rs
key-decisions:
  - "A reply with words[0] == 0 resumes a faulted thread; any other value kills it"
  - "Notes carry no caps and no data; exit words are [kind, value, 0, 0], fault words are [vector, addr, rip, err]"
  - "NOTE_KIND_KILLED exists in the ABI but has no Exit variant until 02-06"
  - "space_notify with SPACE_SELF checks and stores under one lock, so a child cannot redirect once an endpoint is set (OD-18)"
status: complete
requirements-completed: [KERN-10]
commits: 5
plan_head_before: 4298e59d31beab8154a133ac420083fc9a10c82c
plan_head_after: 0d63b301c033840c1f3c16c5e63d3da3b9775084
actuals:
  tokens: 9500
  tasks: 3
  commits: 5
duration: multi-session (dominated by TCG ktest runs of about 10 minutes each)
completed: 2026-10-26
---

# Phase 2 Plan 05: Exit and fault notes through the per-Space notify endpoint Summary

Thread exits and faults now reach one per-Space notify endpoint, each note naming the thread by tid and tag and stamped with the owner credential; a fault is a kernel-made call whose reply is the verdict, and the Phase 1 per-thread exception handler and its reply side-channel are gone.

## What was built

- **Task 1 (tracer, 9844cba)**: `Space.notify`, `set_notify_cap`, `notify_ep`, `note_exit`; `NOTE_FAULT`, `NOTE_EXIT` and `NOTE_KIND_*` in the ABI; `push_note` in `ipc.rs`; the thread exit point sends the exit note (`set_exit`, then `note_exit`, then `reap`). Four `note_*` ktests: exit names tid and tag of each sibling, exit bypasses QUEUE_LEN, exit without notify is dropped, labels are reserved. Full run 101 passed.
- **Task 2 (TDD, f76492b RED, e08103a GREEN)**: `call_note` and `deliver` rewritten so a fault is a call to the notify endpoint; `fault_sibling.rs` fixture; `ExcReply`, `exc_reply`, `exc_handler`, `exc_wq` and the five exception methods removed from `thread.rs`; `sys_set_exception_handler` removed from `ipc.rs`. Exception tests ported (table below). Two new sibling tests: `note_fault_then_exit_from_one_sibling` and `note_fault_interrupted_before_receive_resumes_redirect`.
- **Task 3 (TDD, b1fa98f RED, 0d63b30 GREEN)**: `SYS_SPACE_NOTIFY` 0x107 with `Syscall::SpaceNotify`, `space::set_notify`, `SpaceError::Denied` mapped to EPERM; ktests `space_notify_refuses`, `space_notify_self_only_while_unset`, `space_notify_ring3_self_sets_once`; decode lines, fuzz `numbers` and three targeted errno cases (ESRCH, EBADF, EPERM).

## Exception tests as ported

| Old name | New name | Change |
|---|---|---|
| `fault_message_carries_vector_addr_rip_and_credential` | `fault_note_carries_vector_addr_rip_tid_and_tag` | asserts `[vector, addr, rip, err]`, the `NOTE_FAULT` label, tid and tag |
| `pager_maps_the_page_and_resumes` | `pager_maps_the_page_and_resumes` | name kept; the pager receives the fault note and replies 0 |
| `kill_reply_makes_the_fault_fatal` | `kill_verdict_makes_the_fault_fatal` | adds a second case replying 7 |
| `revoked_or_missing_handler_is_fatal` | `revoked_or_missing_notify_is_fatal` | asserts both a Space with no endpoint and one whose capability was revoked |
| `handler_pair` (helper) | `endpoint_pair` | plus a new `NotifyRig` and `notify_rig` helper (client tag `0x2a`) |

## call_note reuse

Yes. `call_caps` and `call_note` both go through `call_via(from, enqueue_closure)`; the closure is the only part that differs (a caller-supplied message with caps versus a kernel-made note with no caps and no data). The `abandon` semantics are shared: if the note is still Queued when the caller is interrupted, it is withdrawn and `Interrupted` is returned; if it is Received, `block_until_committed` waits for the reply.

## Verification (all runtime evidence is QEMU TCG)

There is no `/dev/kvm` on this host. Every ktest ran under QEMU TCG, `-smp 1`. The KVM re-run is deferred to a host with `/dev/kvm`.

- Full unfiltered `./osdk.sh sh -c '../userland/build.sh && cargo osdk test'` after Task 3 GREEN: **106 passed, 0 failed** (log: `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/551cda72-80af-415e-95df-8a64bb7a2418/scratchpad/ktest-t3-green.log`). Earlier runs: Task 1 101 passed; Task 2 RED 98 passed 5 failed; Task 2 GREEN 103 passed; Task 3 RED 103 passed 3 failed.
- All thirteen plan-named tests are `ok` in that run: `note_labels_are_reserved`, `note_exit_names_tid_and_tag_of_each_sibling`, `note_exit_bypasses_queue_len`, `note_exit_without_notify_is_dropped`, `note_fault_then_exit_from_one_sibling`, `note_fault_interrupted_before_receive_resumes_redirect`, `space_notify_refuses`, `space_notify_self_only_while_unset`, `space_notify_ring3_self_sets_once`, `pager_maps_the_page_and_resumes`, `kill_verdict_makes_the_fault_fatal`, `revoked_or_missing_notify_is_fatal`, `fault_note_carries_vector_addr_rip_tid_and_tag`. `fuzz_syscalls_never_panic` is `ok` (n=100000, enosys=34603, efault=11247, einval=4410, ebadf=35040, eintr=75).
- `sh scripts/check.sh --no-kvm`: `check: static gates passed` after Task 3 GREEN (unsafe_check, pin_check, fmt, clippy, build). This mode does not compile ktests.
- Retirement grep: `grep -v '^\s*//' kernel/src/thread.rs | grep -cE 'ExcReply|exc_reply|exc_wq|exception_handler'` is 0 (baseline 13). `set_exception_handler` and `SYS_SET_EXCEPTION` appear nowhere in `kernel/src` or `userland`.
- Needs the deferred KVM re-run: everything above. TCG is a single-vCPU functional pass; it says nothing about timing or SMP behaviour of the note and verdict paths.

## TDD Gate Compliance

- **Task 1** was a tracer (`type="tracer"`): `feat` commit with its own ktests, run end-to-end (101 passed) before Tasks 2 and 3 expanded on it.
- **Task 2 RED (f76492b)**: 98 passed, 5 failed. The failures were the ported and new fault tests, which expect a note and got the old handler path. The pager test blocks on an unreceived note, so a bounded `recv_note` (panics on timeout) was added so RED fails cleanly instead of hanging. **GREEN (e08103a)**: 103 passed, 0 failed.
- **Task 3 RED (b1fa98f)**: 103 passed, 3 failed; the three `space_notify_*` ktests failed against a stub `set_notify`. **GREEN (0d63b30)**: 106 passed, 0 failed.
- No refactor commits. No mutation check beyond the RED runs.

## Deviations from Plan

**1. [Rule 3 - Blocking] Plan `<automated>` commands name the main checkout.** Ran the equivalent commands from the worktree `kernel/` directory.

**2. [Rule 3 - Blocking] Name filters match zero tests** with `cargo osdk test <filter>`; the full unfiltered run was used throughout.

**3. [Rule 3 - Blocking] Stale `target/osdk`** may need `./osdk.sh rm -rf target/osdk` after a killed container.

**4. [Process] `check.sh --no-kvm` does not compile ktests**, so it is not a substitute for the ktest run.

**5. [Rule 3 - Blocking] `Endpoint::push` widened to `pub(crate)`** so `note_labels_are_reserved` can assert the reserved-label refusal, and a `#[cfg(ktest)] queued()` accessor was added for the queue-depth assertions.

**6. [Plan detail] Exit ordering is reversed against 02-04.** 02-04 had reap before `set_exit`; this plan requires `set_exit`, then `note_exit`, then `reap`, and the thread exit point does so.

**7. [Rule 3 - Blocking] `start_ctx` split into `entry_ctx` and `run_registered`**, and testkit helpers `spawn_image_in` and `recv_note` added, so the ported tests can run an image in a Space that already has a notify capability.

**8. [Process] Task 3 syscall.rs tests (decode lines, fuzz `numbers`, targeted cases) landed in the GREEN commit, not RED.** The RED commit carried the three `space` ktests against a stub `set_notify`; the syscall-level tests need the dispatch arm to compile meaningfully.

**9. [Plan check ambiguity] `grep -c unsafe` on `fault_sibling.rs` prints 1.** The one hit is the mandated `#![deny(unsafe_code)]` line; the file contains zero unsafe blocks.

## Authentication gates

None.

## Known Stubs

None. The only scan hit is a test comment in `kernel/src/space.rs` ("Fill the capability space with a placeholder"), which is a fixture fill, not a stub.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: new-syscall-surface | kernel/src/syscall.rs | `space_notify` (0x107) takes two slot handles from ring 3; covered by decode tests, the fuzz and the three targeted errno cases |

Threat register status:

- T-02-20 (forged `NOTE_EXIT` or `NOTE_FAULT` label): mitigated; `note_labels_are_reserved`.
- T-02-21 (child redirects its own endpoint): mitigated; `space_notify_self_only_while_unset`, `space_notify_ring3_self_sets_once`.
- T-02-22 (notes bypass QUEUE_LEN): mitigated; `note_exit_bypasses_queue_len`. The bound on spaces by creator memory budget is 02-06's charge.
- **T-02-23 (faulting thread stranded by an endpoint that never answers): accepted and still open until 02-06.** A faulted thread waits for a verdict from the notify endpoint's receiver; if the receiver never answers it stays blocked. Same exposure as T-01-26. 02-06's `Space::kill` is what answers it.
- T-02-24 (revoked notify capability keeps delivering): mitigated; `revoked_or_missing_notify_is_fatal`.
- T-02-25 (note words leak kernel state): mitigated; words are vector, user address, user rip and error code only.

## Self-Check: PASSED

- Files exist: `kernel/src/space.rs`, `kernel/src/ipc.rs`, `kernel/src/thread.rs`, `kernel/src/user.rs`, `kernel/src/syscall.rs`, `userland/abi/src/lib.rs`, `userland/tests/src/bin/fault_sibling.rs`, this SUMMARY.
- Commits are ancestors of HEAD: 9844cba, f76492b, e08103a, b1fa98f, 0d63b30.
- Acceptance greps: `SYS_SPACE_NOTIFY: usize = 0x107` in the ABI (1), `SYS_SPACE_NOTIFY` in `syscall.rs` (more than 2 non-comment lines), `pub fn set_notify` in `space.rs` (present), `fn (note_|space_notify_)` in `space.rs` (10, at least 7 required).
