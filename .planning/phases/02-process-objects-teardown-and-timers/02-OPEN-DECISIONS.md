---
phase: 02-process-objects-teardown-and-timers
status: awaiting-sign-off
created: 2026-10-25
---

# Phase 2: Open decisions

There is no `02-CONTEXT.md`. Every decision below is an **assumed decision (needs user sign-off)**:
the plans are written against it, and each plan names the OD-NN ids it rests on. Two decisions
are costly enough that their plan stops at a blocking `checkpoint:decision` before any code
(OD-04/OD-07/OD-08 in 02-06; OD-05 in 02-03). The rest are flagged here and in each plan's
`<reversibility>` lines; sign them off before `/gsd-execute-phase 2`, or re-plan the affected
plans.

Source for the numbered recommendations: `02-RESEARCH.md` section "Open decisions needing the
user" (items 1-10). Items OD-11 to OD-19 surfaced during planning.

## Decisions

| ID | Decision (assumed) | Recommendation source | Plans | Reversibility |
|----|--------------------|-----------------------|-------|---------------|
| OD-01 | The saved-context (signal) stack holds FPU state: each frame is `{UserContext, FpuContext, FS base}`, nesting depth 8. | Research item 1; ROADMAP "Decisions due" | 02-08 | reversible |
| OD-02 | `mmap`, `munmap`, `mprotect` and `brk` are cap-less syscalls on the caller's own space, charged to the space's owner agent. They do not go through procmgr. | Research item 2; ROADMAP "Decisions due" | 02-09 | costly (shapes libaok and procmgr) |
| OD-03 | LIBC-04 kernel calls are built here and verified under LIBC-04 in Phase 6: set-FS-base (`arch_prctl` `ARCH_SET_FS`/`ARCH_GET_FS`, Linux number 158), thread create for musl start-up (`thread_create` with `SPACE_SELF`, no capability), and exit-and-unmap (`SYS_EXIT_UNMAP`). The `clear_tid` word in the thread context is written to zero and futex-woken at thread exit. `set_tid_address` and `exit_group` are not kernel calls in this phase: libaok can build both from these. | Research item 3; ROADMAP "Decisions due" | 02-04, 02-10 | costly |
| OD-04 | Teardown has two levels. **Process (Space) teardown** runs on last-thread exit or `Space::kill`: threads are reaped, every mapping is unmapped and uncharged through a per-Space charge ledger, futex waiters that waited through the space are woken, timers armed from the space are dropped, and callers parked in a dying thread's reply table are answered `ESRCH`. **Agent teardown** runs when an agent's last Space is torn down: its CSpace is drained (capabilities are dropped, never re-inserted into the parent) and its carved memory and CPU budgets return to the parent through a `Weak<Agent>` parent link. "Capabilities returned to the parent" is read as: the parent's own originals were never moved, so dropping the child's derived copies is the return. | Research item 4 | 02-06 (checkpoint), 02-07 | costly (procmgr builds on it) |
| OD-05 | AGNT-05 option (a): server CPU spent for a client is exempt from the client's kernel CPU budget. Each server keeps a per-client request ledger keyed by the kernel-stamped tag, with a documented limit; over the limit the server answers `EAGAIN`. PROJECT.md's core value and the "CPU budget per agent" line say kernel CPU limits do not cover server-side work. Option (b), lending the client's budget through a reply-handle credit, needs kernel changes and is the alternative the checkpoint offers. | Research item 5 | 02-03 (checkpoint) | costly |
| OD-06 | Exception and exit notifications carry numeric `tid` and `tag` stamped by the kernel, plus a label (`NOTE_FAULT`, `NOTE_EXIT`). A fault is a kernel-made `call` from the faulting thread to its handler: the verdict is reply `words[0]` (0 = resume, 1 = kill, anything else = kill). This replaces the `exc_reply` / `exc_wq` reply path in `thread.rs`, and the existing exception ktests are ported to it. No Thread capability is sent in the message. | Research item 6 | 02-05 | costly |
| OD-07 | Image pages, stack pages and IPC staging pages are charged to the owning agent, and teardown uncharges them. This closes the 01-05 SUMMARY hole and the `ipc.rs:510` shortcut. | Research item 7 | 02-06 (checkpoint), 02-07 | reversible |
| OD-08 | Leak measurement: kernel counters are the primary check (live threads, live and dropped Space objects, live agents, agent pages used, CSpace live slots, futex table keys, deadlines, armed timers, parked replies). A secondary free-frame probe counts the frames `FrameAllocOptions` can allocate before failure, before and after the loop, with a tolerance of one process footprint (the `ACTIVATED_VM_SPACE` pin and RCU-delayed frees). No tracked frame-meta module, so the Kernel table of the unsafe ledger stays empty. No new crate. | Research item 8 | 02-06 (checkpoint), 02-07 | reversible |
| OD-09 | Timer tolerance: a wait of D ms (D whole ticks at 1 kHz) ends between D+1 and D+3 ticks after the call, measured in kernel ktests on `-smp 1` with no other in-budget runnable thread. No sub-millisecond resolution is offered. Ring-3 tests assert "never early" and a looser upper bound, because the measuring thread is scheduled too. | Research item 9 | 02-02 | reversible |
| OD-10 | 01-07 lands before any Phase 2 plan executes. Every plan is a draft against the post-01-07 tree and lists the tasks to re-check once 01-07's SUMMARY exists. No B2-compatible fallback is planned. | Research item 10 | all | reversible |
| OD-11 | **Assumption delta, promote Space.** Research treats a process as "a VmSpace plus threads". The plans promote it to a kernel object `space::Space { vm: Arc<VmSpace>, owner: Arc<Agent>, threads: Vec<Weak<Thread>>, dying, notify: Option<Cap>, charged_pages }`. `Thread` holds `Arc<Space>`; `Thread::space()` keeps returning the `VmSpace` so `sched.rs` and `futex.rs` call sites are unchanged. Invariant, tested: a thread's credential equals its Space owner's. | Planner (see `<assumption_delta_decision>` in 02-04) | 02-04 onward | costly |
| OD-12 | AGNT-02's `SIGXCPU` leg belongs to SIG-03 (the signal phase), per the roadmap rule that a requirement maps to the phase that builds its primary component. Phase 2 reserves a kernel label range that SIG-03 takes one label from, and sends no CPU-budget notification. Phase 2 delivers the errno legs: `ENOMEM` for memory budget, `EAGAIN` for thread and timer limits. | Planner, ROADMAP rule (STATE.md decision 3) | 02-09 | reversible |
| OD-13 | `thread_create` with `SPACE_SELF` inherits the caller's tag, and a non-zero tag argument is `EINVAL`. Only a holder of a Space capability with the CREATE right chooses the tag of a thread in another space. A process therefore cannot mint new tags for itself. | Planner | 02-04 | reversible |
| OD-14 | Thread capabilities hold `Weak<Thread>`: holding one never keeps a dead thread or its agent alive, and resolving a dead one is `ESRCH`. Space capabilities hold `Arc<Space>`, because a freshly created space has no threads to keep it alive. A torn-down Space stays as a tombstone with no mappings until its last capability drops. | Planner | 02-04 | costly |
| OD-15 | Phase 2 ships process destruction as the kernel API `Space::kill`, driven by ktests. The ring-3 kill call arrives with procmgr's kill path (PROC/SIG requirements, Phases 4-5), which builds on this API. | Planner | 02-06 | reversible |
| OD-16 | A signal redirect moves the handler's `rsp` 128 bytes below the interrupted `rsp` (the System V red zone), aligned to 16 bytes and then down 8 so the handler sees a post-call frame. This is a register change only: the kernel writes no byte of user memory. | Planner (red zone: musl C code built by clang uses it; the Rust user target disables it) | 02-08 | reversible |
| OD-17 | Fault and exit notes have one destination: the Space's notify endpoint, set by `SYS_SPACE_NOTIFY`. The kernel-only per-thread exception handler from 01-05 (`Thread::set_exception_handler`, `ipc::sys_set_exception_handler`; no syscall number, so no ABI) is removed, and its ktests are ported to the Space endpoint. | Planner | 02-05 | reversible |
| OD-18 | `SYS_SPACE_NOTIFY` with `SPACE_SELF` succeeds only while the space has no notify endpoint (`EPERM` afterwards). A holder of the Space capability with SIGNAL can always replace it. A child process therefore cannot hide its faults and exit from the procmgr that set its endpoint. | Planner | 02-05 | reversible |
| OD-19 | Agent teardown (OD-04) runs automatically only for a child agent (one with a parent link), and only spaces it owns count: `Space::new` and `Space::from_image` count, while spaces adopted by `start`/`run` and by test helpers do not. A root agent never retires automatically. Dropping a child agent that never had a counted space still returns its carve. A retired agent's CSpace is closed: inserts fail, so a dropped capability cannot be re-inserted into a cycle. Consequence for procmgr: exec builds the new space before the old one is torn down. | Planner | 02-07 | reversible |

## Discretionary ABI choices

These are planner choices inside the discretion the roadmap leaves; they are recorded so that
libaok (Phase 4) and the Phase 6 LIBC-04 verification read one table.

### Syscall numbers

| Number | Name | Arguments | Result |
|--------|------|-----------|--------|
| 0x104 | `SYS_THREAD_CREATE` | `space` (slot, or `SPACE_SELF` = `u64::MAX`), `ctx` (pointer to `abi::ThreadCtx`), `tag` | slot of a new Thread capability with the SIGNAL right; the new `tid` is written into `ctx.tid` |
| 0x105 | `SYS_THREAD_INTERRUPT` | `thread` (slot, needs SIGNAL), `entry` | 0; `EAGAIN` when the signal-frame stack is full (after 02-08); `ESRCH` when the thread is dead |
| 0x106 | `SYS_TIMER_ARM` | `ep` (slot, needs SEND), `delay_ms` (1..=600,000), `cookie` | timer id (>= 1); `EINVAL` outside the delay range; `EAGAIN` past 16 armed timers per agent |
| 0x107 | `SYS_SPACE_NOTIFY` | `space` (slot needing SIGNAL, or `SPACE_SELF`), `ep` (slot, needs SEND) | 0; sets the endpoint that receives `NOTE_FAULT` and `NOTE_EXIT` for every thread in the space |
| 0x108 | `SYS_SIGRETURN` | none | does not return on success; `EINVAL` when the frame stack is empty |
| 0x109 | `SYS_EXIT_UNMAP` | `addr`, `len`, `code` | does not return; `EINVAL` (thread keeps running) for an unaligned or non-user range |
| 9 / 10 / 11 / 12 | `SYS_MMAP` / `SYS_MPROTECT` / `SYS_MUNMAP` / `SYS_BRK` | Linux argument order | Linux results; `ENOMEM` when the owner agent's memory budget cannot pay |
| 158 | `SYS_ARCH_PRCTL` | `ARCH_SET_FS` (0x1002) / `ARCH_GET_FS` (0x1003), value or pointer | 0; `EPERM` for a base at or above `USER_TOP` (Linux's result for `ARCH_SET_FS` past `TASK_SIZE_MAX`; amended in 02-10 from `EINVAL`); `EFAULT` for a bad `ARCH_GET_FS` pointer; `EINVAL` for another code |

The futex call (202) gains a timeout: `a[3]` points to a relative `abi::Timespec { sec: i64,
nsec: i64 }` (the Linux `FUTEX_WAIT` position); null means none. `EINVAL` when `nsec` is outside
`[0, 1e9)` or `sec < 0`; a zero timeout returns `ETIMEDOUT` after the value check. Ticks are
`ceil(ns / 1e6)`, and the deadline is `now + ticks + 1`.

`recv` (0x101) and `reply_recv` (0x103) take a timeout in `a[2]` (rdx): relative milliseconds,
0 = none, above 600,000 = `EINVAL`. Expiry is `ETIMEDOUT`.

### Types, rights, labels, limits

- `abi::ThreadCtx`, `#[repr(C)]`, 48 bytes: `rip`, `rsp`, `arg`, `fs_base`, `clear_tid`, `tid`
  (out). `to_bytes` / `from_bytes` like `IpcArgs`.
- Rights: CREATE = 8 (create threads in a Space), SIGNAL = 16 (interrupt a Thread; set another
  Space's notify endpoint). `ALL` = 31. `Rights::from_bits` and `bits` change together.
- New errno: `ESRCH` = 3, in `abi::errno`, the kernel `Errno` and the fuzz `KNOWN` list.
- Kernel label range: `LABEL_KERNEL_BASE = u64::MAX - 15`. Ring 3 may not send a label at or above
  it (`EINVAL`). `LABEL_CANCEL = u64::MAX` (existing), `LABEL_TIMER = u64::MAX - 1`,
  `NOTE_FAULT = u64::MAX - 2`, `NOTE_EXIT = u64::MAX - 3`; the rest are reserved, and SIG-03
  takes one for CPU-budget notification.
- Kernel messages enter through `Endpoint::push_kernel`. `NOTE_FAULT` and `NOTE_EXIT` bypass
  `QUEUE_LEN`, bounded at one outstanding note of each kind per thread. Timer messages respect
  `QUEUE_LEN`, and a full queue retries the timer one tick later.
- Timer message: label `LABEL_TIMER`, sender = the arming agent's credential, `tid` = 0, `tag` =
  the arming thread's tag, `words` = `[cookie, due_tick, fired_tick, timer_id]`.
- `NOTE_EXIT` words: `[kind, value, 0, 0]`, kind 0 = code, 1 = fault (value = address),
  2 = budget, 3 = exception (value = vector), 4 = killed.
- `MAX_THREADS_PER_SPACE` = 64 (`EAGAIN` past it). `MAX_ARMED_PER_AGENT` = 16. Signal-frame depth
  8.
- Self-memory calls (02-09). Anonymous private mappings only, populated eagerly, so every page is
  charged at `mmap` time and `ENOMEM` comes back from the call, never from a later fault.
  - Placement: first fit in `[MMAP_BASE, MMAP_TOP)` = `[0x1_0000_0000, 0x7f00_0000_0000)`. A hint
    is used when the whole range is free. `MAP_FIXED` replaces whatever is mapped there.
  - `PROT_NONE` reserves the range without frames or charge. `PROT_WRITE` or `PROT_EXEC` implies
    read, as on Linux x86-64.
  - Errors in Linux order: `len` 0, an unaligned `MAP_FIXED` address, unknown `prot` bits or an
    unaligned offset are `EINVAL`. A length or range past the top is `ENOMEM`. A mapping that is
    not `MAP_ANONYMOUS` is `ENODEV` (file mappings are libaok's job through vfs).
    `MAP_SHARED` is `ENOTSUP` = 95. Other flag bits are ignored, as Linux ignores unknown flags.
  - `MAX_MAPS_PER_SPACE` = 1024 mapping records: `ENOMEM` past it, Linux's `max_map_count`
    result. This includes a `munmap` that would split a record.
  - `brk` follows the raw Linux call: it returns the new break, or the old one on failure. The
    initial break is the image's `mem_end` rounded up to a page. It grows at most `BRK_MAX` =
    256 MiB, and a space built without an image has break 0, so every `brk` fails.
  - New errno: `ENODEV` = 19 and `ENOTSUP` = 95.

## What would show a decision was wrong

- OD-02: musl's allocator or BusyBox needs lazy (demand-paged) anonymous memory, for example a
  large sparse `mmap` that eager population cannot pay for. Then mmap needs a fault path that
  charges on first touch, and ENOMEM moves from the call to a fault.
- OD-03: libaok cannot answer musl's `set_tid_address(&__thread_list_lock)` in `__init_tp`
  without a kernel call. That call returns the thread's tid; this is from memory of musl 1.2,
  not re-read. Or a non-main thread needs its `clear_tid` changed after creation. Then a
  `SYS_SET_TID_ADDRESS` row (Linux 218) is needed, writing `Thread::set_clear_tid` from 02-10.
- OD-04: procmgr (Phase 4) needs a capability the child held to come back into the parent's
  CSpace. Then the drain has to re-insert, which changes the CSpace-full error paths.
- OD-06: procmgr needs to signal the faulting thread directly, not just reply. Then the message
  needs a Thread capability, with its cap-install failure mode.
- OD-09: a POSIX test in Phase 6 needs sub-millisecond `nanosleep` or `pthread_cond_timedwait`
  precision. Then OSTD needs a finer timer, which is an OSTD patch and the ADR 0002 re-score
  signal.
- OD-11 / OD-14: a ktest finds a strong cycle the explicit drain does not break (the 10,000x
  loop's live-Space count in `space::counts()` climbs).
- OD-19: procmgr needs a child agent to survive with zero processes (a daemon pool, a pre-forked
  agent). Then retirement needs an explicit hold or a kernel call, not the last-space trigger.
