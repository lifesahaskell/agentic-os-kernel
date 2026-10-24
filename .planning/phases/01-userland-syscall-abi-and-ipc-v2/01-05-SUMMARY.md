---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 05
subsystem: kernel-ipc
tags: [block_until, reply-handles, recv, reply_recv, bulk-ipc, KERN-12, IPC-01, IPC-03, IPC-04]
requires: [01-03, 01-04]
provides:
  - "Thread::block_until(wq, cond, deadline) -> Result<R, BlockError>, the only wait primitive outside thread.rs"
  - "SYS_RECV 0x101, SYS_REPLY 0x102, SYS_REPLY_RECV 0x103 with libsys wrappers"
  - "per-thread ReplyTable (MAX_REPLIES = 256) with generation handles"
  - "Message.tid / Message.tag stamped by the kernel; Thread::new_tagged"
  - "bulk call/recv/reply data, copied in each side's context, bounded by MAX_IPC_BYTES"
affects: [01-06, 01-07]
key-files:
  created:
    - userland/tests/src/bin/park_srv.rs
    - userland/tests/src/bin/sibling.rs
    - userland/tests/src/bin/bulk.rs
    - userland/tests/src/bin/bulk_srv.rs
  modified:
    - kernel/src/thread.rs
    - kernel/src/ipc.rs
    - kernel/src/syscall.rs
    - kernel/src/futex.rs
    - kernel/src/user.rs
    - kernel/src/vm.rs
    - userland/abi/src/lib.rs
    - userland/libsys/src/lib.rs
decisions:
  - "Reply-handle generations come from one global AtomicU32, so a handle is valid only in the table that issued it and cross-thread use is EBADF"
  - "A failed delivery to ring 3 puts the received message back at the queue front (unreceive), so a call is not stranded"
  - "Kernel-side sys_recv(t, slot) is kept for the existing ktests; the ring-3 entry is sys_recv_args"
  - "After recv and after call, IpcArgs.send_len holds the full length of the bytes that arrived"
  - "Bulk test buffers are zero-initialised .bss AtomicU8 statics, the only way to have a user-owned mutable buffer under #![deny(unsafe_code)]"
status: complete
commits: 3
plan_head_before: 37dc2fd3f8d66b505ae3a7ba90bb21cacc66f562
plan_head_after: e43593aa0d5be6bd9e7676202f544cf2bacecc8e
requirements-completed: [KERN-12, IPC-01, IPC-03, IPC-04]
actuals:
  tasks: 3
  commits: 3
---

# Phase 1 Plan 05: block_until, reply handles, identity stamps and bulk IPC Summary

Every kernel wait now goes through `Thread::block_until` with an optional deadline and ktest fail-points; ring-3 servers park calls under kernel-held per-thread reply handles and answer them later with `reply` or `reply_recv`; the kernel stamps `tid` and `tag` on every message; call, receive and reply carry up to 64 KiB of bytes by copying in each side's own context.

## Commits

- 833e5f3 feat(01-05): block_until with deadlines and ktest fail-points; migrate every wait site
- b75087e feat(01-05): kernel-held reply handles, recv/reply/reply_recv, tid and tag stamped on every message
- e43593a feat(01-05): bulk IPC data by kernel copy in each side's context

## Results

- `cargo osdk test`: `test result: ok. 54 passed; 0 failed` (47 after Task 1, 51 after Task 2).
- `python3 -I tools/unsafe_check.py && sh scripts/check.sh`: ran unrefused, last line `check: all gates passed` (after one retry, see Deferred Issues).
- Acceptance greps: no `block_on`, one `pub fn block_until`, no `wait_until|new_pair` outside `thread.rs`; `MAX_REPLIES` present; three `SYS_(RECV|REPLY|REPLY_RECV)` in abi; one `fn new_tagged`; `grep -c copy_from_user kernel/src/ipc.rs` is 3; `grep -rnE '\.map\(|cursor_mut' kernel/src/ipc.rs` and the `unsafe` grep over `userland/tests/src userland/abi/src` print nothing.

## Facts the plan asked to record

1. **1000x interleaving loop.** `block_until 1000x: ok=679 interrupted=321 timed_out=0`, with per-point counts (ok, interrupted, timed_out) of point 1 `[(0,111,0),(109,0,0),(123,0,0)]`, point 2 `[(0,101,0),(120,0,0),(102,0,0)]`, point 3 `[(0,109,0),(98,0,0),(127,0,0)]` per action (interrupt, wake, both). Every iteration ended in ok or interrupted; none timed out. `both` always resolves to ok: a satisfied condition beats a pending interrupt.
2. **OSTD `wait_until_or_cancelled`.** Read in the OSTD 0.19.0 source: each loop iteration calls `cond()` before `cancel_cond()`; on a cancel error it closes the waker and calls `cond()` once more, returning `cond().ok_or(e)`. So the condition is checked before the cancel closure and again after it; a wake that races an interrupt or deadline wins.
3. **Bulk test buffer sizes.** 3 * 4096 + 123 = 12,411 bytes for each of `SEND`, `RECV` (client) and `BUF` (server), plus truncation runs with a 100-byte receive limit in each direction.

## Deviations from Plan

**1. [Rule 1 - Bug] `cow_clone` jumped the source cursor out of range on the last page**
- **Found during:** Task 2 (`siblings_are_distinguished_by_tid_and_tag`, second clone of one parent)
- **Issue:** a read-only (or already CoW) page at `range.end - PAGE_SIZE` made `pc.jump(va + PAGE_SIZE)` fail with `AccessDenied`; the first clone write-protects the parent's stack, so any second clone of that range panicked.
- **Fix:** the last page ends the walk instead of jumping.
- **Files modified:** `kernel/src/vm.rs` (not in the plan's `files_modified`)
- **Commit:** b75087e

**2. [Naming] Ring-3 receive is `sys_recv_args`, not `sys_recv`.** The kernel-side `sys_recv(t, slot) -> Message` is used by the existing ktests and the kernel server tasks; renaming it would have rewritten them. `sys_reply` and `sys_reply_recv` keep the planned names.

**3. [Process] Verify commands ran from the worktree.** The plan's verify path names the main checkout (`/home/rakdos/repos/agentic-os-kernel/kernel`); every build and test ran from this worktree's `kernel/` instead.

**4. [Design] Bulk buffers in the bins are `AtomicU8` statics, not plain `.bss` arrays.** Mutable statics need `unsafe`, which `userland/tests` may not contain. Zero-initialised atomics still land in `.bss` and are never written by user code before the kernel's first write to the receive buffer. The parent-unchanged check scans every page of the parent from `text.end` to `mem_end` (the whole post-text region) for non-zero bytes, since the buffer address is not known to the test.

**5. [Design] `IpcArgs::with_send` / `with_recv` exist as planned but no bin uses them**, because the bins pass `AtomicU8` statics by address. They take `&[u8]` / `&mut [u8]`.

## Known Stubs

None.

## Shortcuts (ledgered in code as `shortcut:` comments)

- Staging memory for queued bulk data is not charged to the sender's agent (`read_bytes` in `ipc.rs`); Phase 2 accounting should charge it. Bound today: `MAX_IPC_BYTES * QUEUE_LEN` per endpoint.
- A server thread that exits with calls parked in its `ReplyTable` strands those callers until 01-06's Cancel-then-wait. A caller interrupted mid-call leaves its message queued (unchanged from before).
- Reply-handle generations are 32 bits and wrap after about 4e9 parked calls (`NEXT_REPLY_GEN`).
- The deadline timer assumes one CPU (`arm_deadline_timer`).
- `recv` writes the arrived byte count into the receiver's `IpcArgs.send_len`, as the plan specified; a server that calls `reply_recv` must set `send_buf` and `send_len` before every reply or it replies with the previous request's length.

## Deferred Issues

- **`cpu budget check` in `cargo osdk run` is flaky and was already.** The check bounds the child's ticks by `quota * periods`, and the child lands one tick over about one run in four: on this branch 2 of 6 boot runs failed (`child 67` against a bound of 66, `child 65` against 64); on the untouched base `37dc2fd` 1 of 5 failed (`child 65` against 64). The scheduler, agent accounting and `lib.rs` are unchanged by this plan, and the deadline timer is only registered when a wait has a deadline, which the boot path never has. `check.sh` passed on retry. Not fixed: out of scope, and the right change (a tolerance of one tick per period, or counting periods from the actual tick the first spinner ran) is a decision for the author of the check.

## Threat Flags

None. The surface added (SYS_RECV, SYS_REPLY, SYS_REPLY_RECV, bulk buffers) is covered by T-01-18 to T-01-22: identity comes from the kernel, handles are per thread and generation-checked, staging is bounded, writes go through `copy_to_user`.

## Self-Check: PASSED

- Files: `userland/tests/src/bin/park_srv.rs`, `sibling.rs`, `bulk.rs`, `bulk_srv.rs` exist; the eight modified files are in the commits.
- Commits: 833e5f3, b75087e and e43593a are ancestors of HEAD (`git rev-list --count 37dc2fd..HEAD` was 3 before this SUMMARY).
