# Phase 2: Process objects, teardown and timers - Research

**Researched:** 2026-10-25
**Domain:** Rust capability-microkernel internals on Asterinas OSTD 0.19.0 (scheduler protocol, user-mode context, timers, object lifetime)
**Confidence:** MEDIUM-HIGH. OSTD and kernel behaviour were read in source; nothing was compiled or run (no KVM on this host).

**Basis.** Written against the post-01-07 shape of the repo. Plan 01-07 has not run (blocked on KVM). It deletes the B2 loader, `prog`, `run` and the ktest `PROGRAM` array, and adds `abi::TEST_DATA` and `libsys::test_page`. Findings that depend on it are marked **[01-07]**. The kernel facts below describe the tree as it stands today (HEAD `61f31a5`, 01-06 landed).

**Read this session:** OSTD `task/{mod,processor,preempt/mod}.rs`, `task/scheduler/{mod,fifo_scheduler,info}.rs`, `timer/mod.rs`, `user.rs`, `arch/x86/cpu/context/mod.rs`, `arch/x86/trap/syscall.{S,rs}`, `sync/wait.rs`, `mm/vm_space.rs`, `mm/frame/*`; kernel `sched.rs`, `thread.rs`, `user.rs`, `vm.rs`, `syscall.rs`, `futex.rs`, `agent.rs`, `cap.rs`, `ipc.rs` (lines 1-780), `Cargo.toml`, `OSDK.toml`, `rust-toolchain.toml`, `lib.rs`; `scripts/check.sh`, `userland/abi/src/lib.rs` (constants only), the 01-05/01-06/01-02 SUMMARYs, 01-07-PLAN, `b2-spike-report.md`, `unsafe-ledger.md`, `option-b-microkernel.md`, ADR 0002.
**Not read:** `userland/libsys/src/*`, `userland/tests/src/bin/*`, `tools/unsafe_check.py`, `ipc.rs` lines 780-1439 (tests), 01-RESEARCH/CONTEXT/PATTERNS, wiki pages, and the OSDK base crate (`kernel/target/osdk` is absent on this host, so the default frame and heap allocator source could not be inspected).

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| KERN-04 | `Object::Space` and `Object::Thread`; `thread_create(space, ctx, tag)` gated by the Space capability; a holder creates and signals a thread in another space, a non-holder cannot | `cap::Object` has only `Endpoint` and `Agent` today. `Thread::new_tagged` is the kernel half already. A thread maps to `TaskOptions::new(f).data(Arc<Thread>).spawn()`. New `Rights` bits are needed because `ALL` is 7. See Pattern 1. |
| KERN-07 | Idle task halts the CPU when nothing is runnable | OSTD busy-spins when the current task parks or exits with an empty queue (`ReschedAction::Retry`). OSTD has no idle task. A kernel idle `Task` looping the public `ostd::task::halt_cpu()` in `BudgetScheduler` needs no patch and no `unsafe`. See Pattern 3. |
| KERN-08 | Destroying a process returns carved budgets and capabilities to the parent (`charge_pages` inverse) and wakes futex waiters; 10,000x fork+exit leaves page and capability counts flat | No unmap, teardown, charge inverse, parent link, or per-thread futex waiter tracking exists. `ACTIVATED_VM_SPACE` pins the last-activated space. See Pattern 2 and Pitfalls 2-4. |
| KERN-09 | `futex` wait and IPC `recv` accept timeouts; a timer delivers a message to an endpoint | `block_until(..., deadline)` already supports absolute-jiffy deadlines. Futex and `recv` pass `None` today. Timer-to-endpoint needs a kernel timer task, not an IRQ-context push. See Pattern 4. |
| KERN-10 | Thread-exit and exception notifications go to a process endpoint and name the faulting `tid` and `tag` | `user::deliver` sends `[vector, addr, rip, err]` to `exc_handler` and does not name the thread (`shortcut:` at `user.rs:237`). See Pattern 5. |
| KERN-11 | Signal redirect saves the full user context on a kernel-held stack; `sigreturn` restores it; no kernel write to the user stack | `UserContext` and `FpuContext` are both `Clone` and safely accessible. `syscall_return` takes the iret path when rcx/r11 differ, so arbitrary GPR restore works. FS base needs explicit save/load. See Pattern 6. |
| AGNT-02 | Budget exhaustion maps to standard errno (`ENOMEM`, `EAGAIN`) and `SIGXCPU`; budget-exhausted `mmap` returns `ENOMEM` | `Agent::charge_pages` returns `AgentError::Budget`; no `mmap` syscall exists yet. Image, stack and IPC staging pages are uncharged. See Pattern 7. |
| AGNT-05 | Server CPU-exemption decision recorded with a request ledger and documented limit; core-value wording does not overclaim | A policy decision plus a ledger structure and a PROJECT.md edit. Needs the user. See Open decisions. |
</phase_requirements>

## Summary

Phase 2 needs no new crates, no OSTD patch and no new `unsafe`, as far as source reading can tell. Every primitive the phase needs is exposed safely by OSTD 0.19.0: `ostd::task::halt_cpu()` for idle, `UserContext: Clone` and `FpuContext: Clone` for kernel-held signal frames, `FsBase::save/load` for per-thread TLS base, `TaskOptions::spawn` for `thread_create`, `register_callback_on_cpu` plus the existing `DEADLINES` machinery for timeouts. The Redox re-score trigger in ADR 0002 is not tripped by anything found. The one exception is exact physical-frame leak counting, which would need a macro-expanded `unsafe impl`. Avoid it: count kernel-owned state instead.

The real risks are in lifetime and ordering, not APIs. (1) OSTD busy-spins instead of halting when nothing is runnable, so the idle task is mandatory for KERN-07, and its `enqueue` must request preemption or wake latency is one tick. (2) Teardown is an object-graph problem: `Thread -> Arc<Agent> -> CSpace -> Cap -> Thread` already forms cycles for `Object::Agent`, and adding `Object::Thread`/`Object::Space` multiplies them; the 10,000x loop will only come out flat if teardown drains the CSpace explicitly and clears futex, deadline, reply-table, FPU-owner and run-queue references. (3) `VmSpace::activate` pins the last-activated space per CPU, so physical-frame counts are off by one process's footprint right after a teardown. (4) Timer callbacks run in IRQ context at 1 kHz; they may only wake, never push to an `Endpoint` (its queue lock is not IRQ-safe).

Several items are policy, not engineering, and must go to the user before planning locks them: the unit of teardown (Space/process versus Agent), the AGNT-05 CPU-exemption stance, the exception message shape, and the leak-measurement method. See `## Open decisions needing the user`.

**Primary recommendation:** Plan this phase as five slices in this order: (A) idle task + scheduler-protocol ktests, (B) Space/Thread objects + `thread_create` + exception/exit notifications, (C) deadlines for futex/`recv` + timer task, (D) teardown with counters and the 10,000x loop, (E) saved-context signals + `mmap` charging + AGNT-05 record. Slice A first because it fixes the scheduler contract every other slice relies on; slice D before E because teardown defines the object graph that `sigreturn` frames must not outlive.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Thread/Space objects, `thread_create` authority check | Kernel (`cap`, `thread`) | Userland `libsys` (wrapper) | Authority is exactly the held capability handles; the check must be in the kernel. |
| Process teardown, budget return, cap drain | Kernel (agent/space lifecycle) | procmgr server (decides when to kill) | Leak freedom is a kernel invariant; policy of who dies is a server decision. |
| Idle / halt | Kernel (`sched`) | OSTD (`halt_cpu`) | Scheduler policy is ours; the halt primitive is OSTD's. |
| Timeouts, timer-to-endpoint | Kernel (`thread::block_until`, timer task) | OSTD (`timer`) | OSTD gives only a 1 kHz per-CPU callback list. |
| Exception / exit notification | Kernel (`user::deliver`) | procmgr (receives) | The kernel stamps `tid`/`tag`; the server only reads. |
| Signal frame save/restore | Kernel (`thread`, `user`) | libc/libaok (`sigreturn` caller) | Frames live in kernel memory so user stack corruption cannot forge them. |
| Memory-budget `mmap` | Kernel (cap-less syscall on caller's own space) | libaok (errno translation) | Per the roadmap's decision-due item, self-memory calls are not routed through procmgr. |
| Server CPU-exemption ledger | procmgr/vfs servers (ledger) | Kernel (documented limit only) | The kernel cannot attribute server-side CPU to the client; PROJECT.md wording must say so. |

## Standard Stack

### Core
No new dependencies. The phase uses what `kernel/Cargo.toml` already has: package `aok`, edition 2024, `ostd = "0.19.0"`, path dependency `abi` (`../userland/abi`). Toolchain `nightly-2026-07-21`, image `asterinas/osdk-dev:0.19.0-20261012`; never bump one without the other (`kernel/rust-toolchain.toml`, project constraints).

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ostd | 0.19.0 | Task, scheduler traits, UserMode, FpuContext, VmSpace, timer | Pinned base of the whole kernel. [VERIFIED: kernel/Cargo.toml `ostd = "0.19.0"` per earlier read] |
| alloc (`BTreeMap`, `VecDeque`, `Arc`, `Weak`) | toolchain | Registries, queues | Already used throughout the kernel. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Linear `DEADLINES` `Vec` | `BTreeMap<(u64, seq), Arc<Waker>>` | O(log n) insert, O(k) expiry. Only worth it if the timed-wait count grows; the current scan is O(n) per tick. Optional. |
| Counters for leak checks | Tracked `AnyFrameMeta` | Exact, but `impl_untyped_frame_meta_for!` expands to `unsafe impl`, which `deny(unsafe_code)` rejects outside a ledgered module. `[ASSUMED]` not compiled. Avoid. |

**Installation:** none.
**Version verification:** n/a, no packages added.

## Package Legitimacy Audit

No external packages are installed by this phase. The `research-plan` / `classify-confidence` seams and the research cache were not used in this run (source reading only); no digests were cached.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
ring 3 thread                                   kernel
-------------                                   ------
syscall (thread_create / futex_wait+timeout /   UserMode::execute -> ReturnReason
  recv+timeout / mmap / sigreturn / exit)  ---> UserSyscall -> syscall::decode -> dispatch
fault / exit                              ---> UserException -> handle_cow_fault | deliver()
                                                      |                              |
                         +----------------------------+                              v
                         v                                              exc/exit message
  cap check: CSpace.get(handle) -> Object::{Space,Thread,Endpoint}      (tid, tag stamped by kernel)
         |                                                                           |
         +--> thread_create: Thread::new_tagged + TaskOptions::spawn                 v
         |        -> BudgetScheduler.enqueue(Spawn) --> RunQueue                process endpoint
         |                                                                           ^
  block_until(wq, cond, Some(deadline)) --> DEADLINES <--- timer cb (IRQ ctx, wake only)
                                                |                                  |
                                                v                                  |
                                    kernel timer task (task ctx) --Endpoint::push--+

 BudgetScheduler.RunQueue { current, queue }          idle Task (never budgeted)
   pick(): first eligible thread task, else idle  --> loop { halt_cpu() }   (sti; hlt)

 teardown(process): interrupt+reap threads -> wake futex waiters (TABLE) -> drop DEADLINES
   -> free ReplyTable waiters (answer ESRCH/EIO) -> drain CSpace (return caps to parent)
   -> unmap Space -> charge_pages inverse -> return carve to parent -> drop Arc<Space>
```

### Recommended Project Structure
```
kernel/src/
├── sched.rs      # add idle task, pick fallback, enqueue preempt hint
├── thread.rs     # Thread registry, signal-frame stack, FS base field, interrupt()
├── cap.rs        # Object::{Space,Thread}, new Rights bits
├── space.rs      # NEW: Space object (VmSpace + charge ledger + thread list)
├── timer.rs      # NEW: timer task + armed-timer table (or fold into thread.rs)
├── agent.rs      # uncharge_pages, return_carve, parent link
├── futex.rs      # per-thread waiter tracking, timeout, wake-on-death
├── vm.rs         # unmap_all / uncharge on teardown, mmap charge
└── syscall.rs    # thread_create, sigreturn, timer_arm, mmap; recv/futex timeouts
```
`space.rs`/`timer.rs` are suggestions. The file layout is the planner's call.

### Pattern 1: Space and Thread objects, `thread_create` (KERN-04)
**What:** Add `Object::Space(Arc<Space>)` and `Object::Thread(Arc<Thread>)` to `cap::Object`. `thread_create(space_cap, ctx, tag)` resolves the Space cap with a CREATE-style right, builds a `Thread` via `Thread::new_tagged`, and spawns a task. A Thread cap carries a SIGNAL right that `interrupt(entry)` requires.
**Evidence:**
- `Object` today: [VERIFIED: kernel/src/cap.rs:40-43] `pub enum Object {\n    Endpoint(Arc<Endpoint>),\n    Agent(Arc<Agent>),\n}`.
- Rights today: [VERIFIED: kernel/src/cap.rs:15-19] `SEND = Self(1)`, `RECV = Self(2)`, `GRANT = Self(4)`, `ALL = Self(7)`. `ALL` and `Rights::from_bits` (added in 01-06) must be extended together or a new right is silently masked.
- Kernel half exists: `thread.rs` doc on `new_tagged` reads "This is the kernel half of `thread_create` until Phase 2 adds Thread capabilities." [VERIFIED: kernel/src/thread.rs, earlier read; quote from summary]
- OSTD side: `TaskOptions::new(f).data(x).local_data(y).build()/.spawn()`, `Task::data()`, `Task::current()` [VERIFIED: ostd task/mod.rs, read this session].
**Hazard:** `Thread.agent: Arc<Agent>`, `Agent.cspace` holds `Cap`s, a `Cap` may hold `Object::Thread`. That is a strong cycle when a process holds a cap to its own thread. `Object::Agent` already has the same shape. Break it by explicit CSpace drain in teardown (preferred, see Pattern 2) or by `Weak` in the object. Reasoned, not compiled. `[ASSUMED]` that no other cycle exists; the ktests in Validation must assert `Arc::strong_count` returns to baseline.
**Cross-space thread creation:** the new thread's `Thread.space` is the target Space's `VmSpace`. The post-schedule handler `activate_current_space` already activates per-task. The new thread's agent is the Space owner's agent, not the caller's, so budget charging follows the Space.

### Pattern 2: Process teardown (KERN-08)
**What:** One `teardown` routine, idempotent, ordered:
1. Mark the process dying so new `thread_create`/`call` into it fail.
2. For each thread: `interrupt` it, wait for task exit (or mark `dead` and drop from run queue if parked).
3. Futex: remove the dead threads' waiters from `futex::TABLE` and wake the *other* waiters on keys that belonged to this space. Today `TABLE: SpinLock<BTreeMap<usize, Bucket>>` is keyed by physical address and there is no per-thread tracking [VERIFIED: kernel/src/futex.rs:77 per earlier read, quote in summary]. A per-thread `Weak<Waiter>` list is needed.
4. `DEADLINES`: drop entries whose waker belongs to a dead thread (the `retain` in the timer callback will otherwise hold them until due).
5. `ReplyTable` / parked `ReplyWait`: answer every call parked in a dead server's table with an error so the caller wakes. 01-06 Fact 5: "A server thread that exits, or never answers, with calls parked in its reply table still strands the caller ... Freeing these callers needs the Phase 2 kill path" [VERIFIED: .planning/phases/01-.../01-06-SUMMARY.md Fact 5 (read this session)]. Also the 01-06 shortcut: a kernel-task `Endpoint::recv` marks the call `Received(None)`.
6. Drain the CSpace (removes Thread/Space/Agent caps, breaking cycles) and return transferable caps to the parent. "Capabilities returned" needs a definition: see Open decisions.
7. Unmap and uncharge: `vm.rs` has `handle_cow_fault` charging `agent.charge_pages(1)` per copy but no unmap or credit [VERIFIED: kernel/src/vm.rs, earlier read]. Add `Agent::uncharge_pages` (saturating `fetch_sub`), mirror of [VERIFIED: kernel/src/agent.rs:101-109]:
```
pub fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
    self.mem_used_pages
        .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(pages)
                .filter(|&n| n <= self.mem_limit_pages)
        })
```
8. Carve return: `// shortcut: carved budget is never returned to the parent, add on agent teardown.` [VERIFIED: kernel/src/agent.rs:143]. `spawn_child(grants, mem_limit_pages, cpu_quota_ticks)` carves both memory and CPU. The child needs a `Weak<Agent>` parent (today only the parent id lives in `cred`).
9. `FPU_OWNER` (`static FPU_OWNER: SpinLock<Option<SharedFpu>, ...>`, thread.rs:91): clear it if it points at a dead thread's FPU, or accept the 4 KiB pin until the next swap.
**Frame accounting:** Unmapping frees through `Frame::drop` on the last reference (`mm/frame/mod.rs:259-277`), and per the B2 report frees are RCU-delayed (friction 2). CoW-shared frames are freed only when both sides drop.

### Pattern 3: Idle task (KERN-07)
**What:** OSTD has no idle task. When the current task parks or exits and nothing is queued, OSTD loops. Evidence from ostd `task/scheduler/mod.rs`:
```
ReschedAction::Retry => {
    continue;
}
```
[VERIFIED: ostd task/scheduler/mod.rs:585-587, `Retry` returned at lines 484 and 541]. The loop re-invokes the scheduler's `try_pick_next` until a task appears: a 100% busy spin on a QEMU vCPU.
**Fix, safe:** a kernel `Task` in the `BudgetScheduler` run queue whose body is `loop { ostd::task::halt_cpu(); }`. Evidence for `halt_cpu`:
```
pub fn halt_cpu() {
    crate::task::atomic_mode::might_sleep();
    let irq_guard = crate::irq::disable_local();
    if cpu_local::need_preempt() {
        drop(irq_guard);
    } else {
        core::mem::forget(irq_guard);
        // IRQs were previously enabled (checked by `might_sleep`). So we can re-enable them now.
        crate::arch::irq::enable_local_and_halt();
    }
    super::scheduler::might_preempt();
}
```
[VERIFIED: ostd task/preempt/mod.rs:23-37; re-exported as `ostd::task::halt_cpu`, task/mod.rs:27 per earlier read]. No `unsafe` in our crate, no OSTD patch.
**Scheduler protocol the idle must respect** (ostd `task/scheduler/mod.rs` lines 198-224, 360-401, read this session): `pick_next` is only called after `update_current` returned true; an empty queue means `update_current` must return false; a Wait with a non-empty queue means true. `dequeue_current` must call `schedule_info().cpu.set_to_none()` (as `sched.rs` does).
**Policy in `BudgetScheduler`** (`sched.rs`, 237 lines):
- `eligible(task, now)` is `thread_of(task).is_none_or(|t| t.agent().has_cpu(now))`, so kernel tasks are never budgeted. The idle task must be distinguished from other kernel tasks (the ktest runner is a kernel task too), by a marker in `Task::data()`. `[ASSUMED]` marker approach; any tag works.
- `pick`: first eligible non-idle task; else idle. Never return `None` and never return the running task after `update_current` asked for a switch (B2 friction 4).
- `update_current`: if idle is current, return true as soon as any non-idle task is eligible (so Tick preempts idle); for a non-idle current with only idle left, `Wait`/`Exit` return true (idle is runnable), `Tick` returns false unless a better candidate exists. Idle must not count in "others".
- `enqueue`: currently always returns `None` (ostd enqueue doc: returns the CPU to preempt). Return `Some(CpuId::bsp())` when the current task is idle (or the queue is empty of real tasks) so a wake from IRQ sets `need_preempt`; otherwise a woken task waits for the next 1 ms tick.
- Out-of-budget threads only: idle runs and halts until the period rolls (`CPU_PERIOD_TICKS = 10`, agent.rs:33), then the tick preempts it.
**Test seam:** `install()` cannot be called from a ktest because the ktest runner is a task (stated in `sched.rs`). So protocol tests are run-queue policy unit tests (as today); the halt itself is checked in the boot check (see Validation): count idle iterations over N ticks. A halting idle gives about N; a spinning one gives millions.
**Caveat, `-smp 1` only:** `halt_cpu` and the "one global FPU owner" shortcut both assume one CPU. The project fixes `-smp 1`.

### Pattern 4: Timeouts and timer-to-endpoint (KERN-09)
**What:**
- Futex and `recv` timeouts: pass `Some(deadline)` to `thread::block_until(wq, cond, deadline)`. The signature is `block_until(wq, cond, deadline: Option<u64>)` with `BlockError{Interrupted, TimedOut}` and absolute `Jiffies` deadlines [VERIFIED: kernel/src/thread.rs, earlier read]. The futex site reads `// shortcut: no timeouts; add one with a timer callback when pthread_cond_timedwait ...` [VERIFIED: kernel/src/syscall.rs:237]. `FutexError::TimedOut` already exists. `abi::errno::ETIMEDOUT = 110` already exists [VERIFIED: userland/abi/src/lib.rs:28].
- OSTD timer facts [VERIFIED: ostd timer/mod.rs:25, 34]: `pub const TIMER_FREQ: u64 = 1000;` and `register_callback_on_cpu<F>(func: F)`. The callback list is per-CPU, runs in IRQ context, and has no unregister. There is no timer wheel. Resolution is 1 jiffy = 1 ms.
- The kernel registers exactly one callback (`arm_deadline_timer`, guarded by `DEADLINE_TIMER: AtomicBool`) which `retain`s `DEADLINES` entries with `due > now` and wakes the rest.
- **Tolerance to state in the plan:** a deadline set to tick `d` fires in the callback at the first tick with `now >= d`, then the waiter runs at the next schedule: expect `[0, +2]` ticks over the request (the task wakes into a queue, possibly behind others). State as "request + 1 tick minimum, +3 ticks maximum, `-smp 1`, no other CPU-bound thread in budget"; the number is a planning choice, not measured. `[ASSUMED]`
- **Timer-to-endpoint:** a timer callback must only wake. The `Endpoint` queue lock is `SpinLock<VecDeque<Message>>`, not `LocalIrqDisabled` [VERIFIED: kernel/src/ipc.rs:214 per earlier read], so taking it from IRQ context can deadlock against a task holding it on the same CPU; push also allocates. Use a kernel timer task that sleeps on a `WaitQueue`, is woken by the callback via `Waker::wake_up`, drains an armed-timer table in task context, and calls `Endpoint::push`. The message carries a kernel-stamped sender (timer id as tag).
- A message delivered from a timer needs a defined `tid`/`tag` stamp: use tid 0 and the tag given at arm time, with a distinct label (as `LABEL_CANCEL` does). Cancel messages already carry tid=0 and tag=0 (ipc.rs, earlier read), so pick a label that cannot collide. `LABEL_CANCEL` from ring 3 is rejected (01-06 SUMMARY threat flags); do the same for the timer label.

### Pattern 5: Exception and exit notification naming tid and tag (KERN-10)
**What:** `user::deliver(thread, exc, rip)` sends `[vector, addr, rip, err]` to `Thread.exc_handler` and waits for the reply, with this shortcut comment: `cap::Object` has no thread type, so the message "does not name the faulting thread and the handler replies through an `Arc<Thread>`" [VERIFIED: kernel/src/user.rs:237 (shortcut marker); wording from earlier read]. With `Object::Thread`, either (a) the message carries a Thread capability to the faulting thread (installed in the handler's CSpace; handler replies through it), or (b) the message carries only numeric `tid` and `tag` and the reply goes through the call's reply handle. Option (b) is smaller and has no cap-install failure mode (cspace full), but then "signal" needs a separate lookup by tid. See Open decisions.
- The `tid` source: `NEXT_TID: AtomicU64` starting at 1, never reused [VERIFIED: kernel/src/thread.rs, earlier read]. Never reused is what makes a tid safe to name after death.
- Thread exit (not just fault) must send a notification too; `Exit{Code, Fault, Budget, Exception}` in `user.rs` already classifies exits.
- The two-forked-siblings success criterion needs the notification to name the faulting *sibling*; both siblings share the same Space COW parent but have distinct tid and tag.

### Pattern 6: Saved-context signals and `sigreturn` (KERN-11)
**What:** On signal delivery (today the B2 hack: `ctx.set_rdi(ctx.rip()); ctx.set_rip(handler);` at the end of `run_ctx`), push a frame `{ UserContext clone, FpuContext clone, FsBase }` onto a per-thread kernel `Vec`, then set rip/rdi to the handler. `sigreturn` is a cap-less syscall that pops the frame and overwrites the live `UserContext`, FPU and FS base. The kernel never writes to the user stack.
**Evidence:**
- `UserContext` is `Clone`: `#[derive(Clone, Debug)] pub struct UserContext { user_context: RawUserContext, exception: Option<CpuException> }` [VERIFIED: ostd arch/x86/cpu/context/mod.rs:61-63 plus derive; `general_regs()/general_regs_mut()` public].
- `FpuContext` is `Clone` [VERIFIED: ostd arch/x86/cpu/context/mod.rs:646-652] `impl Clone for FpuContext { fn clone(&self) -> Self { let mut xsave_area = Box::new(XSaveArea::new()); ...`. XSAVE area up to 4096 bytes (`MAX_XSAVE_AREA_SIZE`), so each frame is up to about 4 KiB plus the context. Bound the nesting depth (suggest 8, return `EAGAIN` or kill beyond it); `[ASSUMED]` number.
- FS base: [VERIFIED: ostd arch/x86/cpu/context/mod.rs:114-136] `pub fn save(&mut self)` and `pub fn load(&self)` are safe public methods (the `unsafe` is inside OSTD). They are not saved or restored by the scheduler or `UserContext`. CR4.FSGSBASE is enabled when the CPU supports it (arch/x86/mod.rs:204-214, earlier read), so ring-3 code may `wrfsbase` itself; the kernel must save the FS base after each run and load it in `pre_user_run`. This also serves LIBC-04's set-FS-base call.
- Restore path: `syscall_return` (arch/x86/trap/syscall.S 55-99, earlier read) chooses sysret vs iret by comparing the saved rcx to rip and r11 to rflags, and requires no RF/TF and a canonical rip. It does not key on `trap_num`. A restored clone with rcx != rip therefore takes iret and keeps every GPR. `RawUserContext` is private to OSTD, so restoring is by assignment of the whole `UserContext`, not by fields.
- Caveat: a context saved at a syscall boundary has rcx and r11 clobbered by the `syscall` instruction. Same as Linux; document it.
- `set_rflags` masks to `USER_MODIFIABLE_RFLAGS`, so a forged frame cannot raise IOPL. But the frame is kernel-held, so nothing user-controlled is restored; only `sigreturn` itself must not accept user-supplied context. Good.
**FPU subtlety (important):** the live FPU state may sit in the CPU, not in `Thread.fpu`. `pre_user_run` swaps lazily: `old.lock().save(); self.fpu.lock().load(); *owner = Some(...)` and `FPU_OWNER` is a single global (thread.rs:91, "shortcut: one global owner because the kernel runs `-smp 1`"). Before cloning at signal delivery, if `FPU_OWNER` is this thread's FPU, save the live state into `thread.fpu` first, then clone. On `sigreturn`, copy the saved state back into `thread.fpu` and force a reload on the next `pre_user_run` (clear the owner or load directly in the hook).
**Roadmap decision-due:** "The saved-context stack holds FPU state (revisit only if OSTD cannot)." OSTD can. Recommend: yes, hold it. See Open decisions.

### Pattern 7: `mmap` charging and errno mapping (AGNT-02)
**What:** `mmap`, `brk`, `mprotect` are cap-less syscalls on the caller's own `VmSpace` (roadmap decision due). Each page mapped calls `agent.charge_pages(n)` first; `AgentError::Budget` maps to `-ENOMEM` (`abi::errno::ENOMEM = 12` exists, lib.rs:22). CPU-budget exhaustion maps to `SIGXCPU` (a signal through the Pattern 6 path), never a new interface.
- OSTD API: `VmSpace::cursor_mut(guard, range)` then `CursorMut::map(UFrame, PageProperty)`, `unmap(len) -> usize`, `protect_next` (ostd mm/vm_space.rs, read this session). `FrameAllocOptions::new().zeroed(true).alloc_frame()` (allocator.rs:32-100).
- Uncharged allocation sites today: image and stack frames use `FrameAllocOptions::new().alloc_frame().unwrap()` (`user.rs` ~70, 72, 138, 162, earlier read), so they are uncharged and panic on OOM; IPC staging `read_bytes` memory is uncharged (01-05-SUMMARY line 90: "Phase 2 accounting should charge it"). Decide whether to charge them (cheap, makes teardown symmetric) or record them as accepted. See Open decisions. If charged, teardown's uncharge must cover them or counts will drift in the 10,000x loop.
- Charging must be paired with the unmap path in Pattern 2 or the loop leaks budget even if frames are freed.

### Pattern 8: AGNT-05 request ledger
**What:** A per-client counter in each server (procmgr, vfs): requests handled and server-side ticks spent, with a documented cap per client, plus a PROJECT.md wording change so the core value does not claim kernel-enforced CPU limits cover server-side work. Server CPU is exempt from the client's `cpu_quota_ticks` because the server's own Agent runs the work. The decision (exempt, or lend the client's budget through a reply-handle credit) is the user's. The kernel part is nil unless the lend option is chosen. See Open decisions.

### Anti-Patterns to Avoid
- **Pushing to an `Endpoint` from a timer callback.** IRQ context, non-IRQ-safe lock, allocation.
- **Relying on `Drop` of the Space to free everything.** Cycles through the CSpace keep objects alive; teardown must drain explicitly.
- **Counting physical frames as the primary leak signal.** RCU-delayed frees plus the `ACTIVATED_VM_SPACE` pin make it noisy; use kernel counters as primary.
- **Letting `sigreturn` take user-supplied register state.** The frame is on a kernel-held stack by requirement.
- **Letting the idle task park or take budget.** It must always be runnable and never counted as "others".
- **Inlining `syscall` through crates like `rustix` in userland.** Already forbidden by the stack doc; mentioned because LIBC-04 start-up calls are scoped here.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CPU halt in idle | inline asm `hlt` | `ostd::task::halt_cpu()` | Handles the IRQ-enable/need_preempt race; no `unsafe`. |
| Saving user registers | field-by-field copy of `RawUserContext` | `UserContext::clone()` | `RawUserContext` is private; clone is complete. |
| Saving FPU/SSE/AVX | `fxsave` asm | `FpuContext::clone()`, `save()`, `load()` | xsave/xrstor with the right feature mask handled by OSTD. |
| FS/GS base | `wrfsbase` asm | `FsBase::{save,load}`, `GsBase` | Safe wrappers exist. |
| Sleep/wake | spin on a flag | `WaitQueue`, `Waiter`, `Waker` and existing `block_until` | Cancel/close semantics already handled. |
| Timeouts | a second timer callback per wait | the existing `DEADLINES` + single callback | One callback only; OSTD has no unregister. |
| Task spawn | raw `Task` struct | `TaskOptions::new(f).data(..).spawn()` | The only public constructor. |

**Key insight:** OSTD already exposes every hardware-touching primitive as a safe function. What is left to build is bookkeeping (who owns what, in what order it is released), and that is where leaks and deadlocks live.

## Runtime State Inventory

Kernel-internal state that survives "the process is gone" unless teardown unwinds it. Source: reading the files listed under Basis.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `Agent.mem_used_pages` (AtomicUsize), parent's `mem_limit_pages`/`cpu_carved_ticks` carve | Uncharge on unmap; return carve to parent (code edit, with parent link). |
| Kernel registries | `futex::TABLE` (BTreeMap by physical address); `thread::DEADLINES` (`Vec<(u64, Arc<Waker>)>`); `FPU_OWNER`; thread/space registry (new); `ReplyTable` of each server; run queue `RunQueue{current, queue}` | Remove dead-thread entries from each; answer parked callers with an error. |
| Capabilities | each `CSpace` slot (up to `MAX_SLOTS = 65_536`); Forwarder targets (`Forwarder{target: SpinLock<Option<Target>>}` holds no child registry, so no Weak list to clean) | Drain; return to parent per the definition decided. |
| Per-CPU hardware state | `ACTIVATED_VM_SPACE` holds `Arc::into_raw(Arc::clone(self))` of the last-activated space (ostd vm_space.rs `activate`, quoted in Pattern 2 evidence) | Space and frames stay pinned until another space is activated on this CPU. Allow one-process tolerance, or activate a kernel/dummy space before measuring. |
| Messages in flight | Messages queued on an `Endpoint` carrying `Vec<Cap>` (uncharged) | Dropped with the endpoint; confirm no strong cycle through `Arc<ReplyWait>`. |
| OS-registered state / secrets / build artifacts | None. Verified by reading: no files, no env vars, no external services. | None. |

## Common Pitfalls

### Pitfall 1: OSTD busy-spins and the CPU never halts
**What goes wrong:** KERN-07 appears to work (nothing crashes) but the vCPU runs at 100%.
**Why:** `ReschedAction::Retry` loops; there is no idle in OSTD (Pattern 3).
**Avoid:** idle task in our queue; boot check that counts idle iterations.
**Warning signs:** QEMU process pinned at 100% host CPU with all agents blocked.

### Pitfall 2: Arc cycles hide leaks
**What goes wrong:** Page and cap counts creep in the 10,000x loop.
**Why:** `Thread -> Arc<Agent> -> cspace -> Cap -> Object::Thread -> Thread`. Drop never runs.
**Avoid:** drain the CSpace in teardown before dropping; assert `Arc::strong_count`/`Weak` expectations in ktests.

### Pitfall 3: `ACTIVATED_VM_SPACE` pins the dead space
**What goes wrong:** A test that reads free memory immediately after teardown sees one process's frames still held.
**Why:** `activate` stores a raw `Arc` clone per CPU and releases it only when another space is activated; the kernel activates spaces only for tasks that carry a `Thread` space, so idle and kernel tasks activate nothing.
**Avoid:** primary counters first; for the physical probe, run a tiny second user space between teardown and measurement, or allow a tolerance of one process footprint. Also expect RCU-delayed frees (B2 friction 2): add a settle delay.

### Pitfall 4: Futex waiters of a dead space are never woken
**What goes wrong:** Waiters on shared futex words block forever. `TABLE` has no back-reference from a thread to its bucket entries.
**Avoid:** per-thread waiter tracking (new) and a teardown sweep; test with a waiter on a word in a space that is then killed.

### Pitfall 5: Stranded callers
**What goes wrong:** A client blocked in `call` to a server that dies never returns, and an interrupt no longer frees it (since 01-06, an interrupted caller waits for the reply).
**Avoid:** teardown answers every parked `ReplyWait` of the dead server with an error; extend to `Received(None)` kernel-task receives.

### Pitfall 6: IRQ-context work
**What goes wrong:** Deadlock or allocation in IRQ context from a timer callback.
**Avoid:** callback only calls `Waker::wake_up` (what `DEADLINES` already does).

### Pitfall 7: FPU live state vs saved state at signal time
**What goes wrong:** The signal frame captures stale XMM state because the live registers belong to this thread but were never saved to `thread.fpu`.
**Avoid:** flush if `FPU_OWNER` is this thread, then clone; on `sigreturn` reload (Pattern 6).

### Pitfall 8: FS base not saved
**What goes wrong:** After a context switch or `sigreturn` musl TLS points at another thread's block.
**Why:** neither the scheduler nor `UserContext` saves it; user code can write it with `wrfsbase`.
**Avoid:** save after each run, load in `pre_user_run`, include it in the signal frame.

### Pitfall 9: Rights masking
**What goes wrong:** New rights bits silently vanish at derive or on the wire.
**Avoid:** extend `Rights::ALL`, `from_bits` and `bits` together; add a ktest per right.

### Pitfall 10: Scheduler decision and switch are not atomic
**What goes wrong:** OSTD's own FIXME at scheduler/mod.rs 594-599 notes this. With `-smp 1` the window is IRQs-off; do not claim SMP safety.
**Avoid:** keep the scheduler lock `LocalIrqDisabled` (as `RunQueue` is).

### Pitfall 11: Timer resolution assumed finer than 1 ms
**Avoid:** state the tolerance; tests assert a window, not equality.

### Pitfall 12: Fuzz KNOWN errno list
`fuzz_syscalls_never_panic` has a `KNOWN` list of 11 errnos [VERIFIED: earlier read of syscall.rs tests]. New syscalls returning new errnos (for example `ETIMEDOUT` is already in `abi`, but `ESRCH` or `ENOTSUP` are not) must extend it or the fuzz ktest fails. Extend `abi::errno` and `KNOWN` together.

## Code Examples

Signatures only, since this research writes no kernel code. All from ostd 0.19.0 source read this session.

```rust
// Idle: ostd/src/task/preempt/mod.rs:23
pub fn halt_cpu();

// Scheduler protocol: ostd/src/task/scheduler/mod.rs:144-159, 360-401
pub trait Scheduler<T = Task> { fn enqueue(&self, runnable: Arc<T>, flags: EnqueueFlags) -> Option<CpuId>; /* local_rq_with, mut_local_rq_with */ }
pub trait LocalRunQueue<T = Task> { fn current(&self) -> Option<&Arc<T>>; #[must_use] fn update_current(&mut self, flags: UpdateFlags) -> bool; fn try_pick_next(&mut self) -> Option<&Arc<T>>; fn dequeue_current(&mut self) -> Option<Arc<T>>; }
// EnqueueFlags { Spawn, Wake }  UpdateFlags { Tick, Wait, Yield, Exit }

// Save/restore: ostd/src/arch/x86/cpu/context/mod.rs
impl Clone for UserContext; impl Clone for FpuContext;
impl FpuContext { pub fn save(&mut self); pub fn load(&self); }
impl FsBase { pub fn new(addr: usize) -> Self; pub fn save(&mut self); pub fn load(&self); }

// Timer: ostd/src/timer/mod.rs
pub const TIMER_FREQ: u64 = 1000;
pub fn register_callback_on_cpu<F: Fn() + Sync + Send + 'static>(func: F);

// Hooks: ostd/src/user.rs:126,134
fn has_kernel_event(&self) -> bool;  fn pre_user_run(&self, guard: &DisabledLocalIrqGuard);
```

## State of the Art

| Old Approach (B2) | Current Approach (Phase 2) | Impact |
|-------------------|----------------------------|--------|
| Redirect rewrites rdi/rip, no saved frame | Kernel-held context + FPU + FS base stack, `sigreturn` | No user-stack writes; full register survival |
| Spinning scheduler fallback | Idle task + `halt_cpu` | CPU halts |
| FUTEX_WAIT without timeout | `block_until` with deadline | `pthread_cond_timedwait` becomes possible |
| Exception message without thread identity | tid/tag stamped by kernel | procmgr can attribute faults |
| Carved budget never returned | Parent link + carve return | Fork loops do not exhaust the parent |

**Deprecated/outdated:** none. OSTD docs.rs 0.19.0 build failed (STATE.md), so source is the only reference; every upgrade is a deliberate task (roadmap research note) and should add the scheduler-protocol ktests so the contract is pinned.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Tracked frame-meta for exact leak counting needs a macro-expanded `unsafe impl` that `deny(unsafe_code)` rejects outside a ledgered module | Standard Stack | Could be allowed via a contained module; only affects whether to prefer counters |
| A2 | The OSDK base crate supplies the global frame allocator; no free-page statistics API is exposed | Pattern 2 | Free-page probe by exhaustion might be replaceable with a statistic |
| A3 | No Arc cycle exists beyond Thread/Agent/CSpace/Object | Pattern 1 | A leak survives teardown; ktests on `strong_count` are the check |
| A4 | Timer tolerance of request+1 to +3 ticks at `-smp 1` | Pattern 4 | Tests flake or the stated limit is too tight |
| A5 | Signal frame nesting bound of 8 is acceptable | Pattern 6 | Programs with deeper nesting die |
| A6 | Idle is identified by a marker in `Task::data()` | Pattern 3 | Implementation detail; any marker works |
| A7 | `halt_cpu` from a task with `-smp 1` wakes on the next IRQ (tick) and then reschedules via `might_preempt` | Pattern 3 | Idle latency; confirm by the boot check |
| A8 | Behaviour under SMP is out of scope (single CPU) | all | Project fixes `-smp 1` |
| A9 | Files not read (libsys, test bins, `unsafe_check.py`, ipc.rs tests) contain nothing that changes these findings | Basis | New ktests may need helper changes |

## Open Questions

1. **Does an exited thread's `Task` free its kernel stack promptly?** ostd `after_switching_to` drops the previous `Arc<Task>` after IRQ enable (task/processor.rs, read), but if the registry keeps an `Arc<Thread>` that holds the Task, kstack lives on. Check in ktest by holding only a `Weak`.
2. **Is the 10,000x loop's runtime acceptable under ktest?** Each iteration allocates a Space and a thread. A ktest has no time limit set in what was read; confirm the runner timeout in `OSDK.toml` (not checked).
3. **Does `[boot]` initramfs wiring matter here?** No; Phase 2 stays on the flat-image loader. [01-07] deletes the B2 loader; the Rust bins are the fork/exit workload.
4. **Default allocator behaviour under OOM:** `FrameAllocOptions::alloc_frame().unwrap()` panics on OOM; a charged path should return an error instead. Unknown whether the OSDK allocator reports OOM gracefully.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| /dev/kvm | ktests, boot check (`cargo osdk test/run`) | no on this host | n/a | None for execution; `sh scripts/check.sh --no-kvm` runs unsafe_check, userland build, fmt, clippy and `cargo osdk build` only |
| Docker image `agentic-os-kernel-dev:0.19.0` | `kernel/osdk.sh` | not checked | n/a | Build with `docker build -t agentic-os-kernel-dev:0.19.0 kernel` |
| OSTD 0.19.0 source | this research | yes (cargo registry) | 0.19.0 | n/a |
| OSDK base crate source | allocator questions | no (`kernel/target/osdk` absent) | n/a | Treat allocator claims as `[ASSUMED]` |

**Missing dependencies with no fallback:** KVM for anything that runs a ktest. Plans must be written so the executor runs them on a KVM host; verification commands cannot be proven green here.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | OSTD ktest via `cargo osdk test` (QEMU, `-smp 1`, isa-debug-exit) plus the boot check via `cargo osdk run` |
| Config file | `kernel/OSDK.toml`, `kernel/rust-toolchain.toml` |
| Quick run command | `cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'` (KVM) |
| Full suite command | `sh scripts/check.sh` (runs `tools/unsafe_check.py` first, then userland build/fmt/clippy, kernel fmt/clippy, `cargo osdk test`, `cargo osdk run` with grep gates) |

`scripts/check.sh` boot gate currently requires `aok: fpu preemption check passed` and `aok: cpu budget check passed` and fails on the string `FAILED` [VERIFIED: scripts/check.sh, read this session]. Phase 2 adds `aok: idle halt check passed` (and a timer line) to the `for want in ...` list.

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| KERN-04 | Holder creates and interrupts a thread in another space; non-holder refused (`EPERM` or `EBADF`) | ktest + ring-3 bin | `cargo osdk test thread_create` | Wave 0 |
| KERN-04 | Rights bit round-trip through derive and wire | ktest unit | `cargo osdk test rights` | Wave 0 |
| KERN-07 | `pick` falls back to idle; idle never counted as "others"; Tick preempts idle | ktest unit (policy) | `cargo osdk test sched` | Wave 0 |
| KERN-07 | CPU halts: idle iterations over N ticks about N, not millions | boot check | `check.sh` grep `aok: idle halt check passed` | Wave 0 |
| KERN-08 | 10,000x fork+exit: `mem_used_pages`, CSpace live slots, `futex::TABLE.len()`, `DEADLINES.len()`, registry sizes all equal baseline | ktest (long) | `cargo osdk test teardown_loop` | Wave 0 |
| KERN-08 | Dead process's futex waiters woken; stranded caller answered with error | ktest | `cargo osdk test teardown_wakes` | Wave 0 |
| KERN-08 | Physical-frame probe, secondary, with tolerance of one process | ktest | `cargo osdk test frame_probe` | Wave 0 |
| KERN-09 | Timed futex/`recv` expire within the stated tolerance | ktest | `cargo osdk test timeout` | Wave 0 |
| KERN-09 | Timer delivers a message to an endpoint (task ctx push) | ktest | `cargo osdk test timer_msg` | Wave 0 |
| KERN-10 | Fault/exit in one of two forked siblings names the right tid and tag | ktest + bin | `cargo osdk test exc_names_tid` | Wave 0 |
| KERN-11 | All GPR and XMM survive redirect + sigreturn; user stack bytes unchanged | ktest + bin | `cargo osdk test sigreturn` | Wave 0 |
| AGNT-02 | `mmap` past budget returns `ENOMEM` from ring 3 | ring-3 bin | `cargo osdk test mmap_enomem` | Wave 0 |
| AGNT-05 | Ledger exists, documented limit, PROJECT.md wording | doc check (grep) | `grep -n "server-side" .planning/PROJECT.md` | manual |

### Sampling Rate
- **Per task commit:** `sh scripts/check.sh --no-kvm` here; the targeted `cargo osdk test <filter>` on a KVM host.
- **Per wave merge:** full `sh scripts/check.sh` on a KVM host.
- **Phase gate:** full suite green before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] Scheduler-protocol ktests (idle fallback, `update_current` truth table, `enqueue` preemption hint). Roadmap research note asks for these.
- [ ] Teardown counters module: functions that return the primary counters so the loop test can diff them.
- [ ] Ring-3 bins for fork+exit, thread_create, sigreturn, mmap overrun. **[01-07]** The existing test bins and `libsys::test_page` / `abi::TEST_DATA` come from 01-07; Phase 2 bins must be written in the Rust-bin style, not as B2 assembly. If 01-07 has not landed, these tests cannot be planned against the B2 `PROGRAM` array, which it deletes.
- [ ] Boot-check lines for idle halt and timer, plus their entries in `scripts/check.sh`.
- [ ] Extend `abi::errno` and the fuzz `KNOWN` list for any new errno.

**Limit:** the idle-halt check and any test needing the injected `BudgetScheduler` can only run in the boot check, because `install()` cannot be called from a ktest.

## Security Domain

ASVS level 1 applies at the kernel boundary. V2/V3 (authentication, sessions) do not apply. Authority is capability possession (V4 analogue).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V4 Access Control | yes | Capability check on every `thread_create`, signal, `sigreturn`; rights only shrink through `derive` (GRANT required) |
| V5 Input Validation | yes | Syscall decode/`read_args` bounds (`ncaps > 4` is EINVAL), user pointers through `copy_from_user/copy_to_user` only; fuzz ktest |
| V6 Cryptography | no | n/a |
| V7 Error handling / DoS | yes | Budgeted memory/CPU, nesting bound on signal frames, teardown frees everything |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Non-holder creates/signals a thread in another space | Elevation of privilege | Kernel resolves the Space/Thread cap with a specific right; no ambient lookup by tid |
| Stale capability to a destroyed Space/Thread | Elevation / Tampering | Teardown marks objects dead; ops return error; tids never reused |
| Fork+exit loop exhausting parent budget or memory | Denial of service | Carve return + uncharge; the 10,000x loop test |
| Unbounded signal nesting (up to 4 KiB XSAVE per frame) | Denial of service | Depth bound; frames charged or capped |
| Forged `sigreturn` | Tampering | Frame is kernel-held; `sigreturn` takes no register input; rflags masked by OSTD (`USER_MODIFIABLE_RFLAGS`) |
| User-controlled FS base | Tampering | FS base is per-thread user state; the kernel uses no FS-relative addressing for itself; save/restore it |
| Timer label spoofing from ring 3 | Spoofing | Reject the timer label in user `call` (as `LABEL_CANCEL`) |
| IRQ-context lock inversion | DoS | Wake-only callbacks |
| TOCTOU on redirect (`take_redirect` vs state) | Tampering | Redirect lives under the thread `state` lock, taken once |

## Project Constraints (from CLAUDE.md)

- Rust on Asterinas OSTD 0.19.0, x86-64 only, `-smp 1`; toolchain `nightly-2026-07-21`; image `asterinas/osdk-dev:0.19.0-20261012`; never bump one without the other.
- `#![deny(unsafe_code)]` crate-wide. `unsafe` only in small named modules with `#[allow(unsafe_code)]`, a `// SAFETY: <ID>` comment within 3 lines before each `unsafe` token, and a row in `docs/architecture/unsafe-ledger.md` (enforced by `tools/unsafe_check.py`, run first by `scripts/check.sh`). The Kernel table in the ledger is currently empty. An OSTD patch, or an `unsafe` need that cannot be contained, is the recorded signal to re-score the Redox fork in ADR 0002. **Nothing found in this research triggers it.**
- Agent features are additive: they never change POSIX semantics; denials map to standard errno.
- Docs: wiki claims cite PDF pages; reasoning not stated by a source is marked *(interpretation)*; `raw/` is gitignored and never edited.
- Build/test: Docker image `agentic-os-kernel-dev:0.19.0`, `kernel/osdk.sh`; `./osdk.sh cargo osdk test`, `cargo osdk clippy`, `cargo fmt --check`, `sh scripts/check.sh`. KVM host required.
- Commits: short imperative subjects; never `Co-Authored-By:` or `Claude-Session:` trailers.
- Shortcuts are marked `// shortcut:` comments in code; keep that convention for any deliberate gap.

## Open decisions needing the user

From the roadmap's "Decisions due" for Phase 2:

1. **FPU state on the saved-context stack.** Roadmap: "The saved-context stack holds FPU state (revisit only if OSTD cannot)." OSTD can, safely (`FpuContext: Clone`). **Recommend: yes.** Cost: up to about 4 KiB per frame; needs a nesting bound.
2. **Self-memory calls.** `mmap`/`brk`/`mprotect` as cap-less syscalls on the caller's own space, not via procmgr. **Recommend: confirm.** This is what keeps `mmap`-budget enforcement in the kernel (AGNT-02).
3. **LIBC-04 kernel-call scope.** Which kernel calls does Phase 6 get: set-FS-base (safe via `FsBase::load`), thread create for musl start-up (same path as `thread_create`), exit-and-unmap (needs Pattern 2 teardown of a single thread's stack). **Recommend:** implement all three here, verify under LIBC-04.

Found during research:

4. **Teardown unit and "capabilities returned".** Is the unit a Space/process (threads and mappings) with the Agent surviving, or the Agent itself? Option-b says process = Space + threads inside an Agent and `fork` stays inside the agent, which suggests two levels: process teardown (return pages and caps within the agent) and agent teardown (return carve to parent). Does "returned to the parent" mean caps are re-inserted into the parent's CSpace, or dropped? Recommend: drop caps held by the dying process, return only budgets; caps that were granted by the parent come back via nothing, because the parent still owns the originals. Needs your call.
5. **AGNT-05 CPU exemption.** Server CPU spent on a client's behalf: (a) exempt and ledgered with a documented limit, or (b) charged to the client through a reply-handle credit. Recommend (a) with the ledger, and a PROJECT.md edit stating kernel CPU limits do not cover server-side work.
6. **Exception message shape.** Thread capability to the faulting thread versus numeric `tid`/`tag`. Recommend numeric ids plus the reply handle; add a Thread cap only if procmgr must signal the thread directly.
7. **Charge image, stack and IPC staging pages.** Charging makes teardown symmetric and budgets honest; not charging leaves known holes (01-05 SUMMARY line 90). Recommend charge them, with the uncharge in teardown.
8. **Leak-measurement method.** Counters (primary) plus an optional free-frame probe; or a ledgered tracked-frame-meta module (exact, adds the first Kernel row to the unsafe ledger). Recommend counters plus probe.
9. **Timer tolerance.** State the limit (suggest request +1 to +3 ticks at 1 kHz) and whether sub-millisecond resolution is ever required; OSTD gives no finer timer without a patch.
10. **Dependency on 01-07.** Phase 2 test bins assume the Rust-bin harness. Confirm 01-07 lands before Phase 2 plans execute, or the Phase 2 plans need a B2-compatible fallback.

## Sources

### Primary (HIGH confidence)
- OSTD 0.19.0 source, `/mnt/wslg/distro/home/rakdos/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ostd-0.19.0/src/`: `task/scheduler/mod.rs`, `task/scheduler/fifo_scheduler.rs`, `task/scheduler/info.rs`, `task/preempt/mod.rs`, `task/mod.rs`, `task/processor.rs`, `timer/mod.rs`, `user.rs`, `arch/x86/cpu/context/mod.rs`, `arch/x86/trap/syscall.{S,rs}`, `arch/x86/mod.rs`, `sync/wait.rs`, `mm/vm_space.rs`, `mm/frame/{mod,allocator,untyped}.rs`.
- Repo: `kernel/src/{sched,thread,user,vm,syscall,futex,agent,cap,ipc,lib}.rs`, `kernel/{Cargo.toml,OSDK.toml,rust-toolchain.toml}`, `scripts/check.sh`, `userland/abi/src/lib.rs`, `docs/architecture/{b2-spike-report,unsafe-ledger,option-b-microkernel}.md`, `docs/architecture/adr/0002-rust-microkernel.md`, `.planning/phases/01-*/{01-02,01-05,01-06}-SUMMARY.md`, `01-07-PLAN.md`, `.planning/{STATE,ROADMAP,REQUIREMENTS}.md`.

### Secondary / Tertiary
- None. No web sources used; docs.rs for OSTD 0.19.0 failed to build (STATE.md), so source reading is the reference.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, no new dependencies.
- Architecture / OSTD APIs: HIGH for signatures and semantics read in source; MEDIUM for the interaction of our scheduler with idle (reasoned, not run).
- Teardown design: MEDIUM, the object graph was reasoned and not compiled.
- Pitfalls: MEDIUM-HIGH; the Retry spin, `ACTIVATED_VM_SPACE` pin and IRQ-unsafe queue lock were read directly.
- Timer tolerance numbers: LOW, assumed.

**Research date:** 2026-10-25
**Valid until:** until the OSTD or toolchain pin changes, or 01-07 lands and alters the test harness shape (re-check Validation Wave 0).
