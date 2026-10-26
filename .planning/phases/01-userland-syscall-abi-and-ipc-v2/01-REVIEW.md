---
phase: 01-userland-syscall-abi-and-ipc-v2
reviewed: 2026-10-26T00:00:00Z
depth: standard
files_reviewed: 38
files_reviewed_list:
  - kernel/Cargo.toml
  - kernel/osdk.sh
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
  - userland/libsys/src/fpu.rs
  - userland/libsys/src/lib.rs
  - userland/libsys/src/raw.rs
  - userland/libsys/src/rt.rs
  - userland/build.sh
  - userland/tests/build.rs
  - userland/tests/src/bin/badsys.rs
  - userland/tests/src/bin/bulk.rs
  - userland/tests/src/bin/bulk_srv.rs
  - userland/tests/src/bin/cap_recv.rs
  - userland/tests/src/bin/cow_writer.rs
  - userland/tests/src/bin/fpu.rs
  - userland/tests/src/bin/futex_waiter.rs
  - userland/tests/src/bin/futex_waker.rs
  - userland/tests/src/bin/nullipc.rs
  - userland/tests/src/bin/nullsrv.rs
  - userland/tests/src/bin/park_srv.rs
  - userland/tests/src/bin/reader.rs
  - userland/tests/src/bin/sibling.rs
  - userland/tests/src/bin/spin.rs
  - userland/tests/src/bin/spinner.rs
  - userland/tests/src/bin/tracer.rs
  - userland/tests/src/bin/ud2.rs
  - userland/tests/src/bin/yielder.rs
findings:
  critical: 1
  warning: 8
  info: 6
  total: 15
status: issues_found
---

# Phase 1: Code Review Report

**Reviewed:** 2026-10-26
**Depth:** standard
**Files Reviewed:** 38
**Status:** issues_found

## Summary

Reviewed the Phase 1 kernel (capability space, IPC, futex, scheduler, syscall layer, CoW, flat-image
loader), the ring-3 runtime (`libsys`, `abi`), the 18 ring-3 test programs, and the gate scripts.
`docs/architecture/unsafe-ledger.md` was read as context only. `kernel/src/agent.rs` was read to
trace budget accounting and is cited where relevant but is not in the scope list.

The user-pointer layer is sound. `UserPtr`, `UserSlice` and `Handle` use `checked_add` against
`USER_TOP`, cap bulk lengths at `MAX_IPC_BYTES`, and bound handles by `MAX_SLOTS`. `copy_to_user`
and `copy_from_user` have a bounded retry loop. The futex wait/wake/requeue ordering and the
IPC lock order (queue, then replies, then state) are consistent. The `libsys` asm blocks declare
their clobbers correctly. No `unsafe` sits outside the ledgered files.

One reachable kernel panic from ring 3 was found, in the CoW fault path (CR-01). The remaining
findings are lost-wakeup and bound violations in IPC receive, accounting evasion in the scheduler,
three allocator `unwrap`s that turn memory pressure into a kernel panic, and gaps in the gates
meant to catch regressions (tests whose assertions cannot detect the failures they exist for, and
a boot gate with no timeout).

CR-01 was established by reading the kernel and OSTD 0.19.0 sources; it was not executed.

## Critical Issues

### CR-01: Ring-3 write to a kernel-half address panics the kernel

**File:** `kernel/src/vm.rs:66` (reached from `kernel/src/user.rs:264-267`)
**Issue:** `run_ctx` passes the raw CR2 value of every write page fault to `handle_cow_fault`:

```rust
CpuException::PageFault(pf) if pf.error_code.contains(PageFaultErrorCode::WRITE) =>
    Some(handle_cow_fault(space, thread.agent(), pf.addr))
```

`handle_cow_fault` then does `space.cursor_mut(&guard, &(page..page + PAGE_SIZE)).unwrap()`.
OSTD's `cursor_mut` returns `Err` for any range outside the user page-table range (checked in
the page-table cursor construction). A ring-3 program that stores to a canonical kernel-half
address such as `0xffff_8000_0000_1000` takes a #PF with the WRITE and USER bits set and CR2 equal
to that address. The `unwrap` panics, so one unprivileged store halts the whole kernel instead of
delivering `Exit::Fault` to the agent's fault endpoint. For `addr >= 0xffff_ffff_ffff_f000`,
`page + PAGE_SIZE` also overflows before the cursor is built.

`badsys` and the syscall fuzz test only exercise kernel-half addresses passed to syscalls
(`UserPtr`/`UserSlice` reject those first), so nothing covers a direct ring-3 store. This breaks
the core invariant that a hostile agent cannot take the kernel down.

**Fix:**

```rust
// vm.rs
pub fn handle_cow_fault(space: &VmSpace, agent: &Agent, addr: Vaddr) -> Result<bool, AgentError> {
    if addr >= abi::USER_TOP {
        return Ok(false);
    }
    let page = addr & !(PAGE_SIZE - 1);
    let guard = disable_preempt();
    let Ok(mut c) = space.cursor_mut(&guard, &(page..page + PAGE_SIZE)) else {
        return Ok(false);
    };
    // ...
```

Add a ring-3 test program that stores to `0xffff_8000_0000_0000` and assert the run ends with
`Exit::Fault(addr)` and vector 14 delivered, not a panic.

## Warnings

### WR-01: `recv_parked` can consume a wakeup without consuming a message

**File:** `kernel/src/ipc.rs:351-375`
**Issue:** `push` calls `wq.wake_one()` once per queued message. The woken receiver can leave the
`recv_parked` condition without taking that message:

- the pending-Cancel branch returns the cancel message first (`next_cancel`), leaving the queued
  message in place;
- `me.replies.lock().insert(w.clone())` returning `None` returns `Err(IpcError::Full)` with the
  message "staying queued" (documented in the doc comment).

In both cases, if a second receiver thread is blocked on the same endpoint, nobody wakes it. The
message sits in the queue until the next unrelated `send` or `unreceive`. This is the lost-wakeup
shape the review brief asked about. It needs two receiver threads on one endpoint, which the
current tests do not build.

**Fix:** After returning without popping, wake another waiter if the queue is non-empty. For
example, in the two early-return branches call `self.wq.wake_one()` when `!q.is_empty()` (take care
not to do it under the queue lock if the waker takes the same lock), or wrap `recv_parked` so that
any `Ok(cancel)` or `Err(Full)` exit re-wakes. Add a ktest with two receivers where the first has a
full reply table.

### WR-02: `unreceive` pushes past `QUEUE_LEN` and can hand a stranded caller a second chance only by luck

**File:** `kernel/src/ipc.rs:378-391`
**Issue:** `push` enforces `QUEUE_LEN` (16, line 311), but `unreceive` does `q.push_front(m)`
unconditionally. Between the failed `recv_parked` pop and `unreceive`, senders can refill the
queue to 16, so the queue reaches 17. The kernel-heap bound documented at line 511 (`MAX_IPC_BYTES *
QUEUE_LEN` per endpoint) is then off by one message. Separately, if the caller was interrupted
after `recv_parked` marked the call `Received` but before `unreceive` ran, `abandon` has already
pushed a Cancel and entered the uninterruptible `block_until_committed`. `unreceive` then resets
the state to `Queued` and requeues the call. The caller is now uninterruptible and its call is
queued with no receiver guaranteed, so it hangs until some server happens to receive. That is the
documented "server that never answers" shortcut (T-01-26), but here the cause is a kernel-side
rollback, not a server.

**Fix:** For the bound, accept the one-message overshoot explicitly in a comment and a test, or
reserve one slot (`QUEUE_LEN` for senders, `QUEUE_LEN + MAX_RECEIVERS` for requeue). For the
hang, make `unreceive` check `reply.state` for a committed abandon (a `Cancelled` state set by
`abandon`) and complete the call with `EINTR` instead of requeueing.

### WR-03: Frame allocation `unwrap`s turn memory pressure into a kernel panic

**File:** `kernel/src/vm.rs:83`; `kernel/src/user.rs:90`; `kernel/src/user.rs:126`
**Issue:** `FrameAllocOptions::new().alloc_frame().unwrap()` appears in the CoW break (after
`agent.charge_pages(1)?` has already succeeded), in `load_image` (one frame per page, up to 16 MiB
per image), and in `map_stack`. Exhausting physical memory panics the kernel instead of returning
an error. In the CoW path the page charge is also never refunded when the allocation fails.
`load_image` only loads trusted test images today, but its header validation is written for
untrusted input, and `handle_cow_fault` is reachable by any agent with a budget larger than free
memory.

**Fix:** Map allocation failure to an error. In `handle_cow_fault`, allocate before charging (or
refund on failure) and return `Err(AgentError::OutOfMemory)`, which `run_ctx` already maps to
`Exit::Budget`. In `load_image` and `map_stack`, add a `LoadError::NoMemory` variant.

### WR-04: CPU budget is sampled at the tick, so a thread can run uncharged

**File:** `kernel/src/agent.rs:123-134`; `kernel/src/sched.rs:82-94`
**Issue:** `charge_tick` is called only from `RunQueue::update(UpdateFlags::Tick, ..)` and charges
whichever task is `current` when the timer fires. A thread that issues `sched_yield` (or blocks in
IPC) just before each tick is current at the tick only if it is running then. A program that
spins for most of a tick and yields shortly before the boundary is charged for none of it. The
comment calls this a shortcut, but budget enforcement is the Phase 1 isolation claim for
"scheduler and budget accounting", and an agent is the adversary here. `cpu_budget_check` uses
non-yielding spinners, so it cannot see this.

**Fix:** Charge by elapsed time at every switch-out: record a TSC or `Jiffies` timestamp when a
task becomes `current` in `pick` and add the delta to its agent in `dequeue_current` and on
`Tick`. Add a boot-check spinner that yields on a short fixed interval and assert it is charged
roughly its share.

### WR-05: The CPU-budget boot check cannot detect starvation or under-charging

**File:** `kernel/src/lib.rs:139-141`
**Issue:** The pass condition is `child_ticks <= CHILD_QUOTA * periods && root_ticks <= (10 -
CHILD_QUOTA) * periods && child_ticks > 0 && root_ticks > child_ticks`. The upper bounds are fine
(they fail if enforcement is absent). The lower bounds are not: a scheduler that gives the child
exactly one tick in 300, or charges nothing but the first tick, still prints "passed". `periods`
also carries `+ 1` slack, so the child bound is 62 for an expected ~60. The `fpu_preemption_check`
has the same shape (`ticks.iter().all(|&n| n > 0)`). `check.sh` gates on this output string.

**Fix:** Assert a lower bound derived from the quota, for example `child_ticks * 2 >=
CHILD_QUOTA * (periods - 1)` and a matching floor for the root, with the margins chosen from a
measured run. Print the bounds in the log line.

### WR-06: The syscall fuzz test asserts almost nothing

**File:** `kernel/src/syscall.rs:547-598`
**Issue:** The 100,000-iteration loop asserts only `v >= 0 || KNOWN.contains(&-v)`. The per-errno
counters are printed and never compared. A regression that makes every call return `EFAULT`, or
that leaks capability slots or reply handles on each failed call, passes. The test also never
checks post-conditions (cspace slot count, reply-table size, endpoint queue length) after the
run, even though leak-on-error is the main risk in `recv_into`/`call_caps`. With a pending
interrupt every blocking path returns `EINTR` immediately, so the success paths of
`SYS_REPLY`/`SYS_REPLY_RECV` are barely reached.

**Fix:** After the loop assert the leak state: `agent.cspace.lock().len()` equal to its initial
value plus intentional installs, `t.replies.lock().len() == 0`, empty endpoint queues. Assert that
each of the five counted errnos is non-zero. Assert that `ITERATIONS` calls produced at least one
`Ok` return.

### WR-07: The boot gate in `check.sh` has no timeout and discards the exit status

**File:** `scripts/check.sh:28`
**Issue:** `cargo osdk run ... || true` runs QEMU with no time limit. If `cpu_budget_check`
deadlocks or a regression leaves a thread uninterruptible (WR-02), `check.sh` hangs and CI burns
until its own limit. The `|| true` also hides a QEMU crash or a kernel panic after both pass
strings have printed, since only the log text is checked. An unknown flag (for example
`--nokvm`) is silently ignored (`check.sh:8`), so it runs the full KVM gates.

**Fix:**

```sh
./osdk.sh sh -c 'rm -rf target/osdk && ../userland/build.sh && timeout 600 cargo osdk run' >"$log" 2>&1; rc=$?
cat "$log"
[ "$rc" -eq 0 ] || { echo "check: boot run exited $rc" >&2; exit 1; }
```

and reject arguments other than `--no-kvm`. Confirm that `poweroff(ExitCode::Success)` produces
exit status 0 in your OSDK version before making `rc` fatal.

### WR-08: `unsafe_check.py` misses multi-line `allow` and files outside `src/`

**File:** `tools/unsafe_check.py:21, 101-108, 118-119`
**Issue:** `ALLOW` is applied per line, so

```rust
#[allow(
    unsafe_code
)]
```

is not detected. `rs_files` scans only `kernel/src/**` and `userland/*/src/**`, so `unsafe` or
`allow(unsafe_code)` in `userland/tests/build.rs`, a `kernel/build.rs`, `kernel/tests/`, or a new
crate whose sources are outside `src/` is invisible. `parse_ledger` records only the first `.rs`
path in a row's Location cell, so a row naming two files ledgers one. Rule (b) accepts any
`// SAFETY: <ID>` from the file's own ID set, so a comment for U-03 satisfies an `unsafe` block
that belongs to U-04 in the same file. None of this fails today. The gate is the stated SAFE-01
enforcement, so its blind spots are worth closing.

**Fix:** Apply `ALLOW` to the whole blanked source (`re.compile(..., re.S)` with `finditer` and
line numbers from match offsets). Extend `rs_files` to `rglob("*.rs")` under `kernel/` and
`userland/` excluding `target/`. Reject ledger rows with more than one `.rs` path. Optionally key
the row ID to a function name in the Location cell and require the comment to precede that
function.

## Info

### IN-01: Capability delegation depth is documented but not enforced

**File:** `kernel/src/cap.rs:72-78, 124-134`
**Issue:** `Target::resolve` recurses through `Target::Via` links holding each `Forwarder`'s
`SpinLock` across the recursive call, and dropping a long chain recurses as well. The comment
relies on Shapiro's observed depth of 3. `grant_revocable` applies no limit. No syscall reaches it
in Phase 1, so this is latent. Once capability transfer over IPC can call `grant_revocable`, a
chain of a few thousand grants overflows the kernel stack.
**Fix:** Store a `depth: u8` in `Cap`, refuse `grant_revocable` past a small constant, and make
`resolve` iterative.

### IN-02: `load_image` accepts an entry inside the header and an image that overlaps `TEST_DATA`

**File:** `kernel/src/user.rs:70-77, 137-142`; `userland/abi/src/lib.rs:143`
**Issue:** `text = IMAGE_BASE..rx_end` includes the 40-byte header, so `entry` or `handler` may
point at the header quads. `MAX_IMAGE_SIZE` lets an image extend to `0x140_0000`, which covers
`TEST_DATA` (`0x50_0000`). `map_test_page` then maps over an image page, and its
`cursor_mut(..).unwrap()` and `c.map` behaviour on an occupied page is not something the loader
guards.
**Fix:** Require `entry >= IMAGE_BASE + IMAGE_HEADER_LEN` (same for `handler`), and reject
`mem_end > TEST_DATA` or move `TEST_DATA` above `IMAGE_BASE + MAX_IMAGE_SIZE`.

### IN-03: Hardcoded quota in the boot log line

**File:** `kernel/src/lib.rs:131`
**Issue:** The println contains the literal `quota 8`, which duplicates `CPU_PERIOD_TICKS -
CHILD_QUOTA`. It goes stale if either constant changes.
**Fix:** Print `CPU_PERIOD_TICKS - CHILD_QUOTA` with a format argument.

### IN-04: `reader` handler relies on redirect timing

**File:** `userland/tests/src/bin/reader.rs:37-40, 64-68`
**Issue:** The handler adds `ARGS[SEND_LEN]` to `COUNT` and the main loop also does
`swap(0)` then `fetch_add`. If the redirect lands in ring 3 between the `swap` at line 64 and the
`fetch_add` at line 68, the bytes are dropped and the test fails with exit 1. The comment says a
redirect only arrives while the call is blocked in the kernel, which holds only if the harness
interrupts after confirming the thread is blocked. The failure is a false negative (flaky fail),
not a false pass.
**Fix:** Keep the harness interrupt gated on the reader being blocked, and say so in the test
that drives it, or fold both updates into one atomic step.

### IN-05: `build.sh` leaves stale images in `out/`

**File:** `userland/build.sh:8-15`
**Issue:** `out/` is never cleaned. A removed or renamed test program keeps its old
`out/<name>.bin`, and a `user_image!` reference to it would still build and run against stale
code. The script also does not check the image against `MAX_IMAGE_SIZE`.
**Fix:** `rm -rf out` before `mkdir -p out`; fail if `wc -c < out/$name.bin` exceeds
`abi::MAX_IMAGE_SIZE`.

### IN-06: `osdk.sh` can leave root-owned files after an interrupted run

**File:** `kernel/osdk.sh:15`
**Issue:** The `chown -R` that returns ownership to the host user runs inside the container after
the command. If the container is killed (Ctrl-C, timeout), `kernel/target` and `userland/target`
stay root-owned and the next host-side command fails on permissions.
**Fix:** Run the container with `--user "$(id -u):$(id -g)"` and a writable `CARGO_HOME`, or trap
signals and chown in a final `docker run`.

---

_Reviewed: 2026-10-26_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
