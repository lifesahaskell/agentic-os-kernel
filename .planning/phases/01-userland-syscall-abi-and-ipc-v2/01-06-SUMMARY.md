---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 06
subsystem: kernel-ipc
tags: [cancel-then-wait, capability-transfer, EMFILE, LABEL_CANCEL, IPC-02, IPC-05]
requires: [01-05]
provides:
  - "Cancel-then-wait: an interrupted call is taken back from the queue if no server has it, otherwise the caller waits for the reply and the server gets a LABEL_CANCEL message"
  - "Thread::block_until_committed and Thread::push_cancel"
  - "abi::LABEL_CANCEL (u64::MAX, kernel-only), abi::MAX_IPC_CAPS (4), errno::EMFILE (24)"
  - "capability transfer on call, recv and reply: derive at send, all-or-none install at receive"
  - "CSpace::insert_all / room, Rights::from_bits / bits"
affects: [01-07]
key-files:
  created:
    - userland/tests/src/bin/reader.rs
    - userland/tests/src/bin/cap_recv.rs
  modified:
    - kernel/src/thread.rs
    - kernel/src/ipc.rs
    - kernel/src/syscall.rs
    - kernel/src/cap.rs
    - userland/abi/src/lib.rs
    - userland/libsys/src/lib.rs
decisions:
  - "A caller interrupted after a server received its call keeps waiting (uninterruptibly) for the reply; the server learns of it by a Cancel message carrying the call's reply handle, delivered ahead of queued messages and without taking a new reply-table slot"
  - "Stale Cancels (the server answered first) are dropped at receive; a ring-3 call with LABEL_CANCEL is EINVAL, so only the kernel can send one"
  - "Capabilities are derived (attenuated) in the sender's context at call/send/reply time and installed in the receiver's agent at receive time, all or none; no room is EMFILE and the message goes back to the queue front"
  - "A reply with capabilities installs them in the caller's CSpace at reply time; EMFILE goes to the server and its reply handle stays valid"
  - "Message lost PartialEq/Eq because Cap does not implement them; no code needed it besides one test"
status: complete
commits: 2
plan_head_before: 6668a7abd2eaf8c874859ca41a96845558cc20b7
plan_head_after: 207cd2d67ec1729d76fb8c8e93b011ad68c11f29
requirements-completed: [IPC-02, IPC-05]
actuals:
  tokens: 13200
  tasks: 2
  commits: 2
---

# Phase 1 Plan 06: Cancel-then-wait and capability transfer Summary

An interrupted `call` no longer loses or doubles work: it is taken back out of the queue if no server has it, and otherwise the caller waits for the real reply while the server receives a Cancel message naming the call; separately, `call`, `recv` and `reply` carry up to four attenuated capabilities, derived in the sender's context and installed all-or-none in the receiver's space.

## Commits

- 2d9ee3e feat(01-06): Cancel-then-wait for interrupted calls; recv still aborts at once
- 207cd2d feat(01-06): Capability transfer by message: derive at send, all-or-none install at receive

## Results

- `cargo osdk test`: `test result: ok. 66 passed; 0 failed` (54 base, 61 after Task 1, 66 after Task 2).
- `sh scripts/check.sh`: `check: all gates passed` on the first run; `python3 -I tools/unsafe_check.py`: ok. Kernel and userland clippy and fmt clean.
- Acceptance greps: `LABEL_CANCEL` appears 8 times in `ipc.rs` after Task 1, one `fn block_until_committed`, no `wait_until|new_pair` outside `thread.rs`; `derive` 11 times in `ipc.rs`, `MAX_SLOTS` once in `syscall.rs` (via `Handle::LIMIT`), `EMFILE = 24` once in `abi`.

## Facts the plan asked to record

1. **Reader, commit mode.** `reader_run(true)`: the server holds the call, the reader is interrupted while blocked, the server receives a `LABEL_CANCEL` message with the call's handle, answers it, and the reader still gets that chunk. Exit 0, 64 bytes in order, none lost or doubled.
2. **Reader, abort mode.** `reader_run(false)`: the interrupt lands while the call is still queued, the call is removed from the queue and the redirect handler repeats it. Exit 0, same stream.
3. **Races.** No cancel/dequeue race needed a fix beyond the endpoint queue lock. `Queued` becomes `Received` only inside `recv_parked` with the queue lock held, and `abandon` takes the same lock before reading the state, so it sees exactly one of queued or received. Lock order: queue, then reply table, then reply state; `abandon` is queue then reply state; `deliver` takes the CSpace lock and releases it before the reply-state lock. A cancel that arrives after the server answered is dropped in `next_cancel` (the handle is gone from the table).
4. **Flaky boot check.** `check.sh` passed on the first run; no retry was needed (1 run).
5. **01-05 stranded-callers question: not covered.** Cancel-then-wait covers a server that is alive and eventually answers. A server thread that exits, or never answers, with calls parked in its reply table still strands the caller, and an interrupt no longer frees it (the caller now waits for the reply instead of returning EINTR). Recorded as a `shortcut:` in `abandon` (T-01-26). Freeing these callers needs the Phase 2 kill path (drop the parked `ReplyWait` of a dead server and answer it with an error).

## Deviations from Plan

**1. [Process] Verify commands ran from the worktree.** The plan's verify path names the main checkout; every build and test ran from this worktree's `kernel/` instead.

**2. [Design] `Message` lost `PartialEq`/`Eq`.** It now holds `Vec<Cap>` and `Cap` has no equality. The one test that compared `Option<Result<Message, _>>` now compares the error. `Cap` got a `Debug` impl (rights only) so `Message: Debug` still derives.

**3. [Scope] `kernel/src/cap.rs` changed** though not in the plan's `files_modified`: `Rights::from_bits`/`bits`, `CSpace::insert_all`/`room`, `Debug for Cap`. All-or-none install needs the room count and the wire form needs the bit conversions.

**4. [Test bug, fixed] `cap_recv` reused its `IpcArgs` after a reply.** A successful call overwrites `caps`, `cap_rights` and `ncaps` with the reply's (empty) list, so the first run of the chain test sent a plain call and hung. The bin now sets `ncaps` and `cap_rights` again before each call. Kernel behaviour was correct. The chain test also fails instead of hanging if the child exits early (`join_soon`).

**5. [Scope] `ncaps > 4` is EINVAL in `read_args`** (the 01-05 guard rejected any non-zero `ncaps`); checked from ring 3 as step 6 of `cap_recv`.

## Known Stubs

None.

## Shortcuts (ledgered in code as `shortcut:` comments)

- A server that never answers, or whose thread is gone, strands an interrupted caller (see Fact 5); only the Phase 2 kill path frees it.
- A kernel-task `Endpoint::recv` marks the call `Received(None)`: no Cancel can reach a kernel task, so an interrupted call waits for its reply there too.
- Capabilities queued in an undelivered message live until the endpoint or message is dropped; they are not charged to the sender.

## Deferred Issues

None new. The `cpu budget check` flake from 01-05 did not appear in this plan's `check.sh` run.

## Threat Flags

None. The new surface (capabilities in `IpcArgs`, Cancel messages) is covered by the plan's threat model: rights only shrink through `derive` (GRANT required, mask applied), identity and Cancel come from the kernel, and `LABEL_CANCEL` from ring 3 is rejected.

## Self-Check: PASSED

- Files: `userland/tests/src/bin/reader.rs`, `userland/tests/src/bin/cap_recv.rs` exist; the six modified files are in the commits.
- Commits: 2d9ee3e and 207cd2d are ancestors of HEAD (`git rev-list --count 6668a7a..HEAD` was 2 before this SUMMARY).
