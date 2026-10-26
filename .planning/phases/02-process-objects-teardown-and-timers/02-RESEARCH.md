# Phase 2: Process objects, teardown and timers - Research

**Researched:** 2026-10-26 (force-refresh of the 2026-10-25 research)
**Domain:** Rust capability-microkernel internals on Asterinas OSTD 0.19.0 (scheduler protocol, user-mode context, timers, object lifetime, teardown and leak proof)
**Confidence:** MEDIUM-HIGH. OSTD, the OSDK allocator crates and the kernel were read in source. Nothing was compiled or run (this host has no `/dev/kvm`; Phase 1 evidence is QEMU TCG).

**What changed since the last version.** CONTEXT.md (2026-10-26) settled the old "Open decisions" 1-10 and added D-01 to D-05. The OSTD findings, patterns and pitfalls below are carried over and re-verified against the current tree (line numbers that moved are marked). New material is in `## New since the last research: D-01 to D-05`, Pitfalls 13-22, and `## Open decisions needing the user` (now only what CONTEXT.md leaves open). Plan 01-07 has landed (Phase 1 is complete), so the Rust test bins under `userland/tests` are the workload and `[01-07]` caveats no longer apply.

**Read this session (new):** `.planning/phases/02-*/{02-CONTEXT,02-OPEN-DECISIONS,02-RESEARCH(old),02-06-PLAN,02-07-PLAN,02-VALIDATION}.md` (grep on the last three), kernel `vm.rs` 1-215, `agent.rs` (whole), `sched.rs` (whole), `futex.rs` 28-125, `user.rs` 63-110 and 195-300 and 400-450, `ipc.rs` 440-540 and 860-985, `cap.rs` 13-44 and 137-200, `syscall.rs` 20-80 and 225-250; OSTD `mm/vm_space.rs`, `mm/page_table/{mod,node/mod,cursor/mod}.rs`, `mm/frame/allocator.rs`, `sync/rcu/monitor.rs`, `task/processor.rs`, `task/scheduler/mod.rs`, `error.rs`; `osdk-frame-allocator-0.19.0/src/lib.rs`, `osdk-heap-allocator-0.19.0/src/lib.rs` (copied out of the `agentic-os-cargo-registry` docker volume); `kernel/target/osdk/aok-run-base/Cargo.toml`.
**Not read:** `userland/libsys/src/*` and `userland/tests/src/bin/*`, `tools/unsafe_check.py`, `ipc.rs` tests beyond 860-985, `user.rs` 110-195 and 300-400, `thread.rs` after line 420, wiki pages. Not run: any build, ktest or probe.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

The 19 numbered decisions (OD-01..OD-19) with full wording, syscall numbers, types, rights, labels
and limits live in `02-OPEN-DECISIONS.md`. They were written by the planner as assumptions. This
file records which are now signed off and how. Read both.

#### Discussed and decided in this session: teardown and leak proof

- **D-01 (OD-04, refined):** Teardown has two levels, process (Space) then agent, as assumed. When
  a child agent retires, its CSpace is drained and its carved budgets return to the parent. The
  kernel does **not** revoke grants the dead agent derived to others (a server, a sibling, a
  grandchild); those derived copies survive. Verified in `kernel/src/ipc.rs:455`: capability
  transfer is `derive`, a copy, so the parent's originals never move and "returned to the parent"
  means "the child's copies are dropped". Revocation stays an explicit act by a holder of the
  revoke right. — **Reversibility:** costly — procmgr (Phase 4) and the vfs server build on "a
  delegation outlives its delegator"; adding revoke-on-retire later changes the CSpace drain, the
  forwarder registry and the 10,000x loop's failure paths.
- **D-02 (OD-19, confirmed):** A child agent retires automatically when its last counted Space is
  torn down. No explicit retire call and no pin count in this phase. A root agent never retires
  automatically. Consequence: an agent with zero processes between commands is destroyed unless a
  long-lived process holds it. Phase 4 or 7 adds a pin only if a daemon-agent case appears. —
  **Reversibility:** reversible — a pin count is an additive field and method on `Agent`.
- **D-03 (OD-08, confirmed and tightened):** The 10,000x fork+exit loop **fails** if the free-frame
  probe is low after the loop by more than one process footprint. Counters are the primary check
  (live threads, live and dropped Spaces, live agents, agent pages used, CSpace live slots,
  futex table keys, deadlines, armed timers, parked replies). The probe is the only check
  independent of the teardown code, so it is a gate, not a report. Add a settle loop (yields)
  before the second count to absorb RCU-delayed frees. No tracked frame-meta module, so the Kernel
  table of the unsafe ledger stays empty. — **Reversibility:** reversible.
- **D-04 (OD-07, OD-15, accepted as assumed):** Image, stack and IPC staging pages are charged to
  the owning agent and uncharged in teardown (closes the 01-05 SUMMARY hole and the `ipc.rs:510`
  shortcut). `Space::kill` is a kernel API driven by ktests; the ring-3 kill call arrives with
  procmgr. — **Reversibility:** reversible.
- **D-05 (new, RR-01 and RR-03 carried from the Phase 1 verification):** Make `cow_clone`
  fallible in this phase. It returns `Result`, unwinds a partial clone (uncharges the pages
  already copied, drops the half-built child), and replaces the `unwrap()` calls and the huge-page
  `assert_eq!` (`kernel/src/vm.rs:30-48`). `futex.rs:88` stops mapping every `handle_cow_fault`
  error to `FutexError::Budget` and returns the real errno. A ktest forces a partial-clone failure
  with a tiny budget. No Phase 2 plan covers this today: add a task to 02-06 (the clone and
  unwind) and one to 02-07 (the charge/uncharge accounting and the futex mapping). The three
  frame-allocation failure branches from 01-08 remain covered by inspection only, because OSTD
  0.19.0 has no failure injection. — **Reversibility:** reversible.

#### Accepted as assumed, not discussed

Corey chose to discuss only teardown. These were written into CONTEXT.md as signed off exactly as
written in `02-OPEN-DECISIONS.md`. Two are marked costly there; 02-03 and 02-06 still stop at a
blocking `checkpoint:decision` before code regardless.

- **Settled earlier (no discussion needed):** OD-01 (FPU state on the signal stack, depth 8; the
  roadmap's decision due, research confirmed OSTD can), OD-02 (cap-less self-memory calls), OD-03
  (LIBC-04 call scope: set-FS-base, thread create with `SPACE_SELF`, exit-and-unmap), OD-10
  (Phase 1 is complete, so 01-07 landed), OD-12 (`SIGXCPU` belongs to SIG-03).
- **Object model:** OD-11 (promote `Space` to a kernel object), OD-13 (`SPACE_SELF` inherits the
  caller's tag), OD-14 (Thread caps `Weak`, Space caps `Arc`, tombstone Space), OD-18 (set-once
  notify endpoint for a child). OD-11 and OD-14 are costly.
- **Notification and signals:** OD-06 (fault is a kernel-made call to the notify endpoint, verdict
  in reply `words[0]`), OD-17 (remove the per-thread exception handler), OD-16 (redirect skips the
  128-byte red zone), OD-09 (timer tolerance +1 to +3 ticks at 1 kHz).
- **AGNT-05:** OD-05, option (a). Server CPU spent for a client is exempt from the client's kernel
  budget; each server keeps a per-client request ledger keyed by the kernel-stamped tag and
  answers `EAGAIN` over a documented limit. PROJECT.md must not claim kernel CPU limits cover
  server-side work. Costly. The 02-03 checkpoint is where this is confirmed with the cost in view.

### Claude's Discretion

- The exact settle-loop length and the probe tolerance value, as long as the gate in D-03 is
  enforced and the tolerance is justified by a measured baseline from the first run.
- The `cow_clone` error type and how many pages the partial-clone ktest budgets, under D-05.
- Everything under "Discretionary ABI choices" in `02-OPEN-DECISIONS.md` (syscall numbers, types,
  rights, labels, limits), unless a plan finds it wrong.

### Deferred Ideas (OUT OF SCOPE)

- Revoke-on-retire of a dead agent's outgoing grants. Revisit if a Phase 3 or 4 server needs to
  stop honouring a dead originator's delegations.
- An agent pin count for daemon agents with zero processes. Revisit in Phase 4 (procmgr) or Phase 7.
- Re-running all Phase 1 and Phase 2 runtime evidence on a KVM host (STATE.md blocker).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| KERN-04 | `Object::Space` and `Object::Thread`; `thread_create(space, ctx, tag)` gated by the Space capability; a holder creates and signals a thread in another space, a non-holder cannot | `cap::Object` has only `Endpoint` and `Agent` (cap.rs:40-43). `Thread::new_tagged` is the kernel half. A thread is `TaskOptions::new(f).data(Arc<Thread>).spawn()`. New `Rights` bits are required because `ALL` is 7. Pattern 1. |
| KERN-07 | Idle task halts the CPU when nothing is runnable | OSTD busy-spins on `ReschedAction::Retry`. A kernel idle `Task` looping `ostd::task::halt_cpu()` in `BudgetScheduler` needs no patch and no `unsafe`. Pattern 3. |
| KERN-08 | Destroying a process returns carved budgets and capabilities to the parent (`charge_pages` inverse) and wakes futex waiters; 10,000x fork+exit leaves page and capability counts flat | D-01 to D-05 section: two-level teardown, `uncharge_pages`, `Weak` parent link, CSpace drain, retire state machine, free-frame probe through `osdk_frame_allocator::load_total_free_size`, settle loop, fallible `cow_clone`. Pattern 2, Pitfalls 2-4 and 13-22. |
| KERN-09 | `futex` wait and IPC `recv` accept timeouts; a timer delivers a message to an endpoint | `block_until(..., deadline)` already exists (thread.rs:248); futex and `recv` pass `None`. Timer-to-endpoint needs a kernel timer task, never an IRQ-context push. Pattern 4. |
| KERN-10 | Thread-exit and exception notifications go to a process endpoint and name the faulting `tid` and `tag` | `user::deliver` sends `[vector, addr, rip, err]` to the thread's handler and does not name the thread (`shortcut:` at user.rs:209 now, was 237). OD-06 and OD-17 decide the shape. Pattern 5. |
| KERN-11 | Signal redirect saves the full user context on a kernel-held stack; `sigreturn` restores it; no kernel write to the user stack | `UserContext` and `FpuContext` are `Clone`; `FsBase::{save,load}` are safe; `syscall_return` takes the iret path for arbitrary GPRs. Pattern 6. |
| AGNT-02 | Budget exhaustion maps to standard errno (`ENOMEM`, `EAGAIN`) and `SIGXCPU`; budget-exhausted `mmap` returns `ENOMEM` | `AgentError::{Budget,NoMemory}` already map to `ENOMEM` (syscall.rs:71-79). Cap-less `mmap` charges eagerly through `charge_pages`. Pattern 7. |
| AGNT-05 | Server CPU-exemption decision recorded with a request ledger and a documented limit; core-value wording does not overclaim | OD-05 option (a), signed off in CONTEXT.md. A libsys ledger type plus an ADR and a PROJECT.md wording change; no kernel mechanism. Pattern 8. |
</phase_requirements>

## Summary

Phase 2 needs no OSTD patch and no new `unsafe`. Every primitive is a safe OSTD API: `halt_cpu()` for idle, `UserContext`/`FpuContext: Clone` and `FsBase::{save,load}` for the saved-context stack, `TaskOptions::spawn` for `thread_create`, `DEADLINES` plus one timer callback for timed waits. The Redox re-score trigger in ADR 0002 is not tripped by anything found, including the new D-03 probe.

The five CONTEXT.md decisions reduce to four pieces of engineering with specific hazards. (1) **Retirement (D-01, D-02):** a child `Agent` needs a `Weak<Agent>` parent, a life-cycle state under one lock, `CSpace::drain` plus a `closed` flag (a server can otherwise install a reply capability into a dead agent after the drain), `uncharge_pages`, and a rule for retiring while grandchild agents still run (the carve must not return to the grandparent while a descendant still spends it). `Agent::spawn_child` takes `&self`, so it cannot create the `Weak`; it must become `self: &Arc<Self>`. (2) **Probe (D-03):** a safe free-frame read exists and needs no `unsafe`: `osdk_frame_allocator::load_total_free_size()` plus `osdk_heap_allocator::load_total_slab_size()`. Both crates are already linked into the OSDK base crate, so adding them as `aok` dependencies at `0.19.0` should unify to the same statics. That is an `[ASSUMED]` compile, and it contradicts OD-08's "no new crate" wording, so it is a Wave 0 spike and an open decision. RCU-delayed frees complete only on real context switches (`finish_grace_period` runs only in `switch_to_task`); a bare `yield_now` with no peer runnable does nothing, so the settle loop must spawn a peer task. (3) **Charging (D-04):** every site that allocates frames for a process or stages bytes must charge a `Payer` and be mirrored in teardown. (4) **Fallible clone (D-05):** the only `cow_clone` failures an `Err` can express are an invalid range, an unsupported mapping (MMIO or huge page) and a refused charge. OSTD itself `expect`s on page-table node allocation (page_table/node/mod.rs:70-75), so an out-of-memory inside `cow_clone` panics in OSTD and cannot be handled or injected. Today `cow_clone` charges nothing, so no budget can make it fail: to reach a partial-clone failure with a tiny budget it must charge something. Recommendation: charge the child's page-table node frames (exact, derivable from the virtual addresses, calibratable against the free-frame probe).

**Primary recommendation:** Keep the five-slice order from the last research (A idle and scheduler tests, B Space/Thread objects and notifications, C deadlines and timer task, D teardown with counters, probe and the 10,000x loop, E saved-context signals, `mmap` charging and the AGNT-05 record). Add the D-05 work to 02-06 (`cow_clone` returns `Result<Cloned, CloneError>`, takes a `&dyn Payer`) and 02-07 (the futex mapping, the `handle_cow_fault` paths). Run a one-ktest spike for the two allocator crates before 02-06 depends on the probe.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Thread/Space objects, `thread_create` authority check | Kernel (`cap`, `thread`, `space`) | Userland `libsys` (wrapper) | Authority is exactly the held capability handles; the check must be in the kernel. |
| Process (Space) teardown, charge ledger, `Space::kill` | Kernel (`space`) | procmgr (decides who dies, Phase 4) | Leak freedom is a kernel invariant; the ring-3 kill call arrives with procmgr (D-04). |
| Agent retirement, CSpace drain, carve return | Kernel (`agent`, `cap`) | — | Automatic on the last counted Space (D-02); no ring-3 call and no pin count. |
| Cloning an address space and unwinding a failed clone | Kernel (`vm`) | OSTD (`VmSpace`, cursors) | Fork's kernel half; fallibility and charging are ours (D-05). |
| Free-frame leak probe | Kernel ktest support | OSDK base crates (counters) | Independent of the teardown code, read through safe functions (D-03). |
| Idle / halt | Kernel (`sched`) | OSTD (`halt_cpu`) | Scheduler policy is ours; the halt primitive is OSTD's. |
| Timeouts, timer-to-endpoint | Kernel (`thread::block_until`, timer task) | OSTD (`timer`) | OSTD gives only a 1 kHz per-CPU callback list. |
| Exception / exit notification | Kernel (`user::deliver`) | procmgr (receives) | The kernel stamps `tid`/`tag`; the server only reads. |
| Signal frame save/restore | Kernel (`thread`, `user`) | libc/libaok (`sigreturn` caller) | Frames live in kernel memory so user-stack corruption cannot forge them. |
| Memory-budget `mmap` | Kernel (cap-less syscall on the caller's own space) | libaok (errno translation) | OD-02: self-memory calls are not routed through procmgr. |
| Server CPU-exemption ledger | Servers via `libsys::ledger` | Kernel (documented limit only) | The kernel cannot attribute server-side CPU to the client (OD-05 option a). |

## Standard Stack

### Core
No new dependency is required for the phase's behaviour. Existing: package `aok`, edition 2024, `ostd = "0.19.0"`, path dependency `abi` (`../userland/abi`); toolchain `nightly-2026-07-21`, image `asterinas/osdk-dev:0.19.0-20261012`; never bump one without the other (CLAUDE.md constraint).

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ostd | 0.19.0 | Task, scheduler traits, UserMode, FpuContext, VmSpace, timer | Pinned base of the whole kernel. [VERIFIED: kernel/Cargo.toml dependency `ostd = "0.19.0"`, read in the previous session] |
| alloc (`BTreeMap`, `VecDeque`, `Arc`, `Weak`) | toolchain | Registries, queues, parent link | Already used throughout the kernel. |

### Supporting (new, for D-03 only, pending the Wave 0 spike)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| osdk-frame-allocator | 0.19.0 | `load_total_free_size()`: bytes the global frame allocator holds free | Free-frame probe in ktests only. Already a dependency of the OSDK base crate. |
| osdk-heap-allocator | 0.19.0 | `load_total_slab_size()`: bytes of physical memory committed to slab caches | Added to the free count so slab growth during the loop does not look like a leak. |

Evidence (quoted verbatim from the crate sources, read this session from the `agentic-os-cargo-registry` volume):
- `osdk-frame-allocator-0.19.0/src/lib.rs`: `fast_smp_counter! { pub static TOTAL_FREE_SIZE: usize; }` and `pub fn load_total_free_size() -> usize { TOTAL_FREE_SIZE.get() }`. [VERIFIED: osdk-frame-allocator-0.19.0/src/lib.rs] `FrameAllocator::alloc` subtracts `layout.size()`; `dealloc` and `add_free_memory` add it back. [VERIFIED: same file, read previous session]
- `osdk-heap-allocator-0.19.0/src/lib.rs:34-41`: `/// Total size (in bytes) of physical memory committed to the slab caches.` ... `pub fn load_total_slab_size() -> usize {\n    TOTAL_SLAB_ALLOCATED.get()\n}`. [VERIFIED: osdk-heap-allocator-0.19.0/src/lib.rs:34-41]
- `kernel/target/osdk/aok-run-base/Cargo.toml:12,15` contains `[dependencies.osdk-frame-allocator]` and `[dependencies.osdk-heap-allocator]`; `cargo-osdk-0.19.0/src/base_crate/mod.rs:262-270` adds both at `version = "0.19.0"`. [VERIFIED: those two files]
- `ostd::mm::frame::allocator` exposes only the `GlobalFrameAllocator` trait and no statistics, which is why the OSDK crate is the only source of a free count. [VERIFIED: ostd-0.19.0/src/mm/frame/allocator.rs, read previous session]

Whether `aok` compiles with these two as direct dependencies and sees the same statics as the base crate is **not verified**. `[ASSUMED]` that Cargo unifies them (same registry crate, same version, so one copy). The `kernel/Cargo.lock` has no entry for either today (grep returned nothing), so adding them changes the lock file. A PoC copy of the tree was prepared but never built (no KVM, no time). Make it task 1 of the plan that first needs the probe: one ktest that reads both counters, allocates and drops a `FrameAllocOptions` frame, and asserts the free count moved by `PAGE_SIZE` and came back.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Free count from `osdk_frame_allocator` | Probe by exhaustion (allocate frames until `NoMemory`, count, free) | No new dependency, but slow with 2 GiB RAM, perturbs RCU and the heap, and a failed allocation inside a page-table path panics (page_table/node/mod.rs:70-75). Fallback only if the spike fails. |
| Free count | Tracked frame-meta module | Exact, but `impl_untyped_frame_meta_for!` expands to `unsafe impl`; D-03 rejects it and keeps the ledger's Kernel table empty. |
| Linear `DEADLINES` `Vec` | `BTreeMap<(u64, seq), Arc<Waker>>` | O(log n) insert. Only worth it if timed-wait counts grow. Optional. |

**Installation (only if the spike passes):** add to `kernel/Cargo.toml` `[dependencies]`:
```toml
osdk-frame-allocator = "0.19.0"
osdk-heap-allocator = "0.19.0"
```
Then commit the changed `kernel/Cargo.lock`. Use them only from `#[cfg(ktest)]` code so no production path depends on them.
**Version verification:** both are at 0.19.0 on crates.io, published 2025-03-14 and 2025-03-29, repo `github.com/asterinas/asterinas` (from the legitimacy check below).

## Package Legitimacy Audit

Run: `gsd_run query package-legitimacy check --ecosystem crates osdk-frame-allocator osdk-heap-allocator` (this session).

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| osdk-frame-allocator | crates.io | published 2025-03-14 | 161/wk | github.com/asterinas/asterinas | SUS (low-downloads) | Flagged: planner adds a `checkpoint:human-verify` before the `Cargo.toml` edit |
| osdk-heap-allocator | crates.io | published 2025-03-29 | 150/wk | github.com/asterinas/asterinas | SUS (low-downloads) | Flagged: same checkpoint |

`osdk-frame-allocator` [WARNING: flagged as suspicious by the seam on download count only; verify before using.] `osdk-heap-allocator` [WARNING: same.]

Mitigating facts, both read this session: both are the Asterinas project's own crates, `cargo-osdk` adds them to every kernel's base crate at the same version (`cargo-osdk-0.19.0/src/base_crate/mod.rs:262-270`), and `kernel/target/osdk/aok-run-base/Cargo.toml` already lists them, so they are already compiled into this kernel image. The checkpoint is a confirmation, not a new supply-chain exposure. No `postinstall` field (crates have none). The low download count reflects how few kernels use OSDK.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** `osdk-frame-allocator`, `osdk-heap-allocator` (planner inserts `checkpoint:human-verify` before the install)

## Architecture Patterns

### System Architecture Diagram

```
ring 3 thread                                   kernel
-------------                                   ------
syscall (thread_create / futex_wait+timeout /   UserMode::execute -> ReturnReason
  recv+timeout / mmap / sigreturn / exit)  ---> UserSyscall -> syscall::decode -> dispatch
write fault / other fault / exit          ---> UserException -> handle_cow_fault | deliver()
                                                      |                              |
                          Ok(true) resume <-----------+                              v
                          Err(Budget|NoMemory) -> Exit::Budget              exc/exit message to the
                          Ok(false)/other -> deliver()                      Space's notify endpoint
                                                                            (tid, tag kernel-stamped)

 fork (kernel half, procmgr later):
   cow_clone(parent_vm, range, payer: &dyn Payer) -> Result<Cloned{space, charged}, CloneError>
       |  per mapped page, ascending VA: charge new page-table nodes -> map shared frame, COW+RO both sides
       |  on Err: uncharge `charged`, drop half-built child, issue the TLB flush, parent pages stay COW-sole
       v
   Space::new(vm, owner) records ledger = charged; owner.enter_space()  (fails if owner is Retiring)

 process end (last thread exit, or Space::kill):
   Space::teardown (idempotent):  dying=true -> interrupt+reap threads -> wake futex waiters (TABLE)
     -> drop DEADLINES / armed timers of its threads -> answer parked ReplyWait with an error
     -> unmap the owned VmSpace -> owner.uncharge_pages(ledger.swap(0)) -> owner.leave_space()
   leave_space():  spaces -= 1; if spaces == 0 && !root  =>  Agent::retire()

 Agent::retire() (child only, once; D-01/D-02):
   state := Retiring (enter_space now fails; CSpace.close() makes insert fail)
   caps := CSpace.drain()   -- under the lock; drop `caps` AFTER the lock is released
   if live_children == 0: return carve to parent (uncharge_pages(mem_limit), cpu_carved -= quota,
                           parent.live_children -= 1, cascade if the parent was waiting)
   else: wait; the last child's retire completes this one
   state := Retired      (derived copies held by others are NOT touched)

 BudgetScheduler.RunQueue { current, queue }          idle Task (never budgeted)
   pick(): first eligible thread task, else idle  --> loop { halt_cpu() }   (sti; hlt)

 D-03 probe (ktest):  before = free(); loop 10_000 { fork; run; exit }; settle(); after = free()
   free() = load_total_free_size() + load_total_slab_size();   FAIL if before - after > footprint
   settle(): R rounds of { spawn a no-op kernel task; yield until it ran }  (real context switches)
```

### Recommended Project Structure
```
kernel/src/
├── sched.rs      # idle task, pick fallback, enqueue preempt hint
├── thread.rs     # Thread registry, signal-frame stack, FS base field, interrupt()
├── cap.rs        # Object::{Space,Thread}, new Rights bits, CSpace::{drain,close,live}
├── space.rs      # NEW: Space object (VmSpace + charge ledger + dying + thread list), Payer impl
├── timer.rs      # NEW: timer task + armed-timer table (or fold into thread.rs)
├── agent.rs      # Payer, uncharge_pages, parent Weak, Life state machine, retire
├── futex.rs      # per-thread waiter tracking, timeout, wake-on-death, FutexError mapping
├── vm.rs         # fallible cow_clone, handle_cow_fault, unmap_all, mmap charge
├── probe.rs      # NEW, #[cfg(ktest)]: free-frame probe, settle loop, counters snapshot
└── syscall.rs    # thread_create, sigreturn, timer_arm, mmap; recv/futex timeouts
```
File layout is the planner's call; names follow the existing plans (`Space`, `Payer`, `leave_space`, `AgentError::Retired`).

## New since the last research: D-01 to D-05

### D-01 and D-02: retirement of a child agent

**Facts about today's code** (all read this session):
- `Agent` has `cred`, `cspace: SpinLock<CSpace>`, `mem_limit_pages`, `mem_used_pages`, `cpu_quota_ticks`, `cpu_carved_ticks`, `cpu`. It has no parent link beyond `Credential.parent: Option<u64>`, no life-cycle state, no inverse of `charge_pages`. [VERIFIED: kernel/src/agent.rs:44-54] `pub cspace: SpinLock<CSpace>,` / `mem_limit_pages: usize,` / `mem_used_pages: AtomicUsize,` / `cpu_carved_ticks: AtomicU64,`.
- `charge_pages` is at agent.rs:103-111 (was 101-109) and the carve shortcut is at agent.rs:145 (was 143): `// shortcut: carved budget is never returned to the parent, add on agent teardown.` [VERIFIED: kernel/src/agent.rs:145]
- `spawn_child(&self, grants, mem_limit_pages, cpu_quota_ticks)` carves memory with `self.charge_pages(mem_limit_pages)` and CPU with `cpu_carved_ticks.try_update(... carved.checked_add(cpu_quota_ticks).filter(|&n| n <= self.cpu_quota_ticks))`; on a memory failure it rolls the CPU carve back with `fetch_sub`. [VERIFIED: kernel/src/agent.rs:146-171] So the exact inverse at retirement is `parent.uncharge_pages(mem_limit_pages)` and `parent.cpu_carved_ticks.fetch_sub(cpu_quota_ticks)`. Nothing else was carved.
- Capability transfer is a copy. [VERIFIED: kernel/src/ipc.rs:455-458] `/// Attenuated copies of the capabilities in `t`'s space, taken in the sender's own context.` ... `fn derive_caps(t: &Thread, list: &[(usize, Rights)]) -> Result<Vec<Cap>, CapError> {` ... `list.iter().map(|&(s, m)| cs.get(s)?.derive(m)).collect()`. That is what makes D-01 true: retiring drops copies, and copies given to others survive.
- `CSpace` has `insert`, `insert_all`, `get`, `remove`, `room()` and `MAX_SLOTS = 65_536`; no drain, no live count, no closed flag. [VERIFIED: kernel/src/cap.rs:137-200] `pub const MAX_SLOTS: usize = 65_536;` / `self.free.len() + (Self::MAX_SLOTS - self.slots.len())`. Live slots = `slots.len() - free.len()`.
- `spawn_child` is `&self`; callers hold `Arc<Agent>` (sched.rs:185, cap.rs:337, ipc.rs:708 and 866 all call it on an `Arc<Agent>`). [VERIFIED: grep of spawn_child call sites]

**Design (prescriptive).**
1. `spawn_child(self: &Arc<Self>, ...)` so it can store `parent: Option<Weak<Agent>>` in the child. `Weak`, not `Arc`: a strong parent link plus a parent CSpace holding an `Object::Agent` cap to the child is a cycle. Call sites already hold an `Arc`, so a receiver change is source-compatible.
2. A single `SpinLock<Life>` (not independent atomics), with `Life { state: Live | Retiring | Retired, spaces: usize, children: usize }`. `enter_space()` checks `state == Live` and increments in one critical section, so a Space cannot be created in an agent that is already retiring (02-07's `Space::from_image(img, C)` is `Err(AgentError::Retired)`). `leave_space()` decrements and, when `spaces == 0` and the agent is not a root, starts `retire()`. Root agents (`parent: None`) never retire (D-02).
3. `retire()` order: set `Retiring`; `cspace.lock()` then `drain()` then drop the lock; **drop the drained `Vec<Cap>` only after the lock is released** (dropping a `Cap` can drop an `Arc<Endpoint>` whose queued `Message`s carry more `Cap`s, and may drop an `Arc<Space>` whose `Drop` runs `teardown` and calls `leave_space` on an agent whose state is already `Retiring`; both must be harmless re-entries, so `retire` and `leave_space` must be idempotent on a non-`Live` state). Then return the carve (below). Then `Retired`.
4. `CSpace::close()` sets a flag after the drain; `insert` and `insert_all` then fail (reuse `CapError::Revoked`, which already maps to `EBADF`, or add a variant and extend the `From<CapError> for Errno` match at syscall.rs:60-67). Without it a server replying to a dead client installs capabilities into a drained CSpace through `install_caps` (ipc.rs:471-480) and the objects live as long as the `Arc<Agent>` that something else still holds. The D-03 "CSpace live slots" counter would catch this only if it also counts the dead agent.
5. **Descendants (not settled by CONTEXT.md).** A child's carve contains its own children's carves. If the child returns its whole carve to the parent while a grandchild is alive, the parent can re-carve memory the grandchild is still spending (budget over-commit). Rule: `retire()` returns the carve only when `children == 0`; each child's retire decrements the parent's `children` and, if the parent is `Retiring` and now at zero, completes the parent's return, cascading upward. `spawn_child` increments the parent's `children` under the parent's `Life` lock and fails with `Retired` if the parent is not `Live`. Listed as Open decision 1 with this as the default.
6. Counters for D-03: `live_agents` (static atomic, +1 in `build`, -1 in `Drop for Agent`), `CSpace live slots` per agent, `agent pages used`. The `Drop` count catches a surviving `Arc<Agent>` cycle; the retire path alone does not.

`debug_assert!` in `uncharge_pages` that it never underflows (02-06 already says this), but keep the saturating subtract in release builds so a double-uncharge cannot wrap into a huge budget.

### D-03: the free-frame probe and the settle loop

**How to read it safely.** `free() = osdk_frame_allocator::load_total_free_size() + osdk_heap_allocator::load_total_slab_size()`. Both are safe `pub fn`s returning `usize` (quoted above). Adding the slab count makes slab growth neutral: slab pages leave the frame allocator's free list (free falls) and are counted in the slab total (slab rises). Allocations too large for the slabs go through the linked-list heap and are not covered; the kernel objects in the loop (`Space`, `Thread`, `Agent`, `Arc`s, `Vec`s) are small, but this is `[ASSUMED]` and the measured baseline decides. `fast_smp_counter!` reads per-CPU values; with `-smp 1` there is one slot and no skew. The counter reads the allocator's internal count, not the number of untouched frames in a bitmap, so a frame in an RCU-delayed free queue is counted as used until the callback runs.

**RCU delay: what to expect.** [VERIFIED: ostd-0.19.0/src/task/processor.rs:45] `crate::sync::finish_grace_period();` is called from `switch_to_task`, and grep found no other caller. [VERIFIED: ostd-0.19.0/src/sync/rcu/monitor.rs] `finish_grace_period` returns at once when `!self.is_monitoring`; otherwise it adds the current CPU to `current_gp.cpu_mask` and, when the mask is full, takes the callbacks, restarts the next grace period if `next_callbacks` is non-empty, and runs the callbacks inline. With `-smp 1` one context switch completes one grace period. A callback queued while a grace period is in flight waits for the next one, so a deferred free needs at most two real switches, and a deferred free that queues another deferred free (page-table node under an unmapped frame) needs more. Per `yield_now`, with no second runnable task OSTD returns `DoNothing` and no switch happens (read previous session, task/scheduler/mod.rs), so a loop of bare `Task::yield_now()` never advances RCU.

**Settle loop (prescriptive).** `settle(rounds)`: each round spawns a kernel no-op task with `TaskOptions::new(|| {}).spawn()` and spins on `Task::yield_now()` until a flag it sets is observed. Each round forces at least two switches (to the peer and back). Use `rounds = 8` (16 switches against a bound of about 2 per level of deferral); the value is `[ASSUMED]` and Claude's discretion per CONTEXT.md. Calibrate by logging `free()` after rounds 1, 2, 4, 8, 16 on the first run and keeping the smallest count after which the value stops rising. Run the settle loop **inside the ktest**, which runs under OSTD's default FIFO scheduler, so it does not depend on the injected `BudgetScheduler` (which a ktest cannot install).

**Pin.** `VmSpace::activate` stores `Arc::into_raw(Arc::clone(self))` in the per-CPU `ACTIVATED_VM_SPACE` and releases the previous one only when another space activates. [VERIFIED: ostd-0.19.0/src/mm/vm_space.rs:126-143] `let self_ptr = Arc::into_raw(Arc::clone(self)) as *mut VmSpace;` / `ACTIVATED_VM_SPACE.store(self_ptr);`. So the last process's whole footprint stays allocated until something else activates. Before the second `free()` read, activate a tiny dummy `Arc<VmSpace>` (it pins only itself); then the tolerance can be less than a footprint. D-03 allows up to one process footprint, which absorbs it either way.

**Tolerance (prescriptive).** `footprint` is measured, not guessed: in the same ktest, run the iteration once before the baseline `free()` read (warm caches and slabs), then run it once more and record `F = free_before - free_after_settle` for that single iteration with the pin in place. Gate: `free_before_loop - free_after_loop_and_settle <= F`, and fail otherwise. Log `F` and the 10,000-loop delta in the test output so the planner can see how far from the gate the run was. The first run is a calibration run: if `F` is 0 (the dummy-space trick), the gate is strict zero plus slab noise; the planner must decide whether to keep `F` or a fixed slack, and put the number in the SUMMARY with the TCG caveat.

**Counters (primary), each needs a reader function:** live threads (registry size), live and dropped Spaces, live agents (`Drop` counter), agent pages used, CSpace live slots, `futex::TABLE.len()` (futex.rs:77 `static TABLE: SpinLock<BTreeMap<usize, Bucket>>`), `thread::DEADLINES.len()` (thread.rs:36), armed timers, parked replies in each `ReplyTable`, `FPU_OWNER` (thread.rs:91). [VERIFIED: grep of those declarations this session]

**Cost.** The loop is 10,000 process creations. Under TCG, with images of a few pages each, the run time is unmeasured (02-VALIDATION says "unmeasured"). Make the iteration count a `const` and record the first measured wall time.

### D-04: charge sites to cover and mirror in teardown

Each is a place that allocates user-visible or kernel-staged memory for an agent's process; each needs a `charge` before the allocation commits and an `uncharge` at the matching free.

| Site | Today | Required |
|------|-------|----------|
| Image and stack frames at `load_image` | `FrameAllocOptions::new().alloc_frame().map_err(\|_\| LoadError::NoMemory)?` (user.rs:92-94), uncharged | `Space::from_image(img, owner)` charges `img.pages` first, fails with `Budget` charging nothing (02-06) |
| IPC staging buffers (`read_bytes`) | `let mut buf = vec![0u8; src.len()];` with `// shortcut: staging memory is not charged ...` (ipc.rs:510-512) | Charge `ceil(len / PAGE_SIZE)` to the sender before the `vec!`, uncharge when the message is consumed or dropped (02-07 `staging_charges_the_sender_until_consumed`). Charge before allocating so an oversized request does not allocate first. |
| COW break in `handle_cow_fault` | allocates, then `agent.charge_pages(1)?` (vm.rs:101-104) | Through the `Payer` (the Space), so the ledger records it and teardown uncharges it |
| `mmap`/`brk` pages | not yet written | `charge_pages` before mapping; `ENOMEM` on `Budget` (AGNT-02) |
| Page-table nodes of a new Space | uncharged, allocated inside OSTD | Charged by `cow_clone` (D-05 below); for `load_image` spaces charge the same estimate or accept as a documented gap (Open decision 2) |

Charge-then-allocate versus allocate-then-charge: `handle_cow_fault` allocates first so a refused charge frees the frame on drop (vm.rs:99-100 comment). That ordering is safe only because the allocation is a single frame. For multi-page staging charge first and refund on allocation failure.

### D-05: fallible `cow_clone`, `handle_cow_fault` and the futex mapping

**Current code, verbatim** [VERIFIED: kernel/src/vm.rs:27-59]:
`pub fn cow_clone(parent: &VmSpace, range: Range<Vaddr>) -> VmSpace {` ... `let mut pc = parent.cursor_mut(&guard, &range).unwrap();` / `let mut cc = child.cursor_mut(&guard, &range).unwrap();` ... `let (r, item) = pc.query().unwrap();` ... `panic!("cow_clone: MMIO in a user range at {va:#x}");` ... `assert_eq!(r.len(), PAGE_SIZE, "cow_clone: huge page at {va:#x}");` ... `pc.jump(va + PAGE_SIZE).unwrap();` / `cc.jump(va).unwrap();`. The unwraps are at lines 30, 31, 34, 46, 48, the `panic!` at 36, the `assert_eq!` at 38. `handle_cow_fault` is at 68-110 with `unwrap()`s at 81 and 107.

**Every failure mode of `cow_clone`:**

| Site | Cause | Reachable | Handling |
|------|-------|-----------|----------|
| `VmSpace::new()` | Root page-table node allocation: `.expect("Failed to allocate a page table node")` [VERIFIED: ostd-0.19.0/src/mm/page_table/node/mod.rs:70-75] | Machine OOM only | OSTD panics; cannot be handled or injected. Cover by inspection (matches 01-08's "inspection only"). |
| `parent.cursor_mut` / `child.cursor_mut` | `Err(Error::AccessDenied)`: `impl From<PageTableError> for Error { fn from(_err: PageTableError) -> Error { Error::AccessDenied } }` [VERIFIED: ostd-0.19.0/src/error.rs:24-27]; `PageTableError` has `InvalidVaddrRange`, `InvalidVaddr`, `UnalignedVaddr` [VERIFIED: ostd-0.19.0/src/mm/page_table/mod.rs:42-50] | Yes: a range outside the user half or unaligned | `CloneError::Range`; a ktest passes an unaligned range and a range ending above `USER_TOP` |
| `pc.query()` | `Err` when the cursor leaves its range | No after `find_next` returned `Some`, but do not rely on it | `?` into `CloneError::Range` |
| item is `VmQueriedItem::MappedIoMem` | A user mapping of device memory [VERIFIED: ostd-0.19.0/src/mm/vm_space.rs:593-611, variants `MappedRam`, `MappedIoMem`] | Not constructible in a ktest without a real `IoMem` | `CloneError::Unsupported(va)`; inspection |
| `r.len() != PAGE_SIZE` | A huge-page mapping | Not constructible: `UFrame` mappings made through `map(frame, prop)` are 4 KiB and the kernel never builds huge frames (`alloc_frame` is single-frame, allocator.rs:49-51) | `CloneError::Unsupported(va)`; inspection |
| `pc.jump` / `cc.jump` | `Err` when the address is outside the cursor range | No (va comes from `find_next` inside `range`) | `?` into `Range` |
| `cc.map(frame, prop)` | Panics if the address is already mapped or outside the cursor range [VERIFIED: ostd-0.19.0/src/mm/vm_space.rs:365-371] | No: fresh child, address from the parent walk | Document; no handling |
| Page-table node allocation inside `cc.map` | `alloc_if_none(...).unwrap()` and `expect` in OSTD (page_table/cursor/mod.rs:465) | Machine OOM only | OSTD panics. Not injectable. |
| **Charge refused** | New: `payer.charge_pages(n)` returns `AgentError::Budget` | **Yes, with a tiny budget** | `CloneError::Budget`; this is the only failure a ktest can force |

**Error type (prescriptive).** `pub enum CloneError { Range, Unsupported(Vaddr), Budget }` in `vm.rs`, with `From<CloneError> for Errno`: `Range` and `Unsupported` map to `EINVAL`, `Budget` to `ENOMEM`. This adds no errno, so the fuzz `KNOWN` list in syscall.rs (11 entries, line 465) stays unchanged. There is no `NoMemory` variant on purpose: no `cow_clone` path can observe an allocation failure (OSTD panics), and a variant nothing can produce invites a dead match arm.

**What `cow_clone` can charge so a partial-clone failure is reachable.** Today it charges nothing: a COW-shared frame is charged to the parent only, and nothing the clone itself allocates (page-table nodes) is charged to anyone. [VERIFIED: 02-06-PLAN.md known-gap text, quoted: "A cow-cloned child's shared pages stay charged to the parent, and are uncharged at the parent's teardown even while the child still maps them."] With a charge of zero, no budget can make the clone fail, and D-05's ktest has nothing to force. Charge the child's page-table node frames: they are real kernel memory the clone allocates, exactly computable, and checkable against the probe.

Geometry: x86-64 uses 4 levels. [VERIFIED: ostd-0.19.0/src/arch/x86/mm/mod.rs:30-31] `const BASE_PAGE_SIZE: usize = 4096;` / `const NR_LEVELS: PagingLevel = 4;`. The root is one frame (`create_user_page_table` allocates `PageTableNode::alloc(PagingConsts::NR_LEVELS)`; the kernel half is shared, no extra nodes: page_table/mod.rs:365-395). Below the root, a new level-3 node is needed when `va >> 39` changes, a level-2 node when `va >> 30` changes, a level-1 node when `va >> 21` changes. `[ASSUMED]` that 512-entry nodes and these shifts hold (not read from `PagingConsts` directly this session); the calibration ktest below pins it.
- Charge 1 for the root up front, then walking mapped pages in ascending order: first page costs 3 (levels 3, 2, 1); thereafter `(va>>21 != prev>>21) + (va>>30 != prev>>30) + (va>>39 != prev>>39)` each as 1.
- Charge **before** `cc.map` for that page, so a refusal leaves the child with only the pages already mapped.

**Unwind (prescriptive).** Single pass, because the parent is mutated as it goes (`pc.protect_next` makes each writable page read-only and COW). On any `Err`:
1. Drop `cc` and `pc` (cursor locks) before the child; locals drop in reverse declaration order, so declare `child` first (as the code does) and return only after the cursors are gone.
2. Uncharge exactly what was charged (`payer.uncharge_pages(charged)`).
3. Issue and dispatch the TLB flush for the range anyway (the existing flush at vm.rs:55-56 is skipped by an early return otherwise).
4. Drop the child `VmSpace`: its page-table nodes free synchronously (whole-space drop, not an explicit `unmap`; see Pitfall 15) and the shared data frames lose one reference, so the parent still holds them.
5. Leave the parent's already-visited pages COW and read-only. This is benign: with the child gone the frame's reference count is 1, so the next write fault takes the `sole` branch in `handle_cow_fault` (vm.rs:85) and re-enables `W` with no copy and no charge. Document it in the doc comment and assert it in the ktest. Restoring `W` eagerly would need the visited-page list and a second cursor pass; not worth it.

**The ktest (prescriptive).** `cow_clone_partial_failure_unwinds`:
- Build a parent space with one writable page in each of two different 2 MiB windows inside one 1 GiB window, for example `0x40_0000` and `0x60_0000` (DATA is `0x50_0000`, so reuse of `space_with` needs a variant that maps at two addresses).
- Expected full charge: root 1 + first page 3 + second page's new level-1 node 1 = **5**. Run with `Agent::new_root(1, 5)`: succeeds with `charged == 5` and `mem_used_pages() == 5`. This also pins the geometry assumption.
- Run with `Agent::new_root(1, 4)`: `Err(CloneError::Budget)`; assert `mem_used_pages() == 0`; assert the first parent page is `COW` and not `W`, the second page is untouched and still `W`; then write through the parent (fault, `handle_cow_fault` returns `Ok(true)`), assert it succeeded with `mem_used_pages() == 0` and the byte readable; assert the probe's `free()` after `settle` equals its pre-clone value within slab noise.
- Add `cow_clone_rejects_bad_ranges`: unaligned start and an end above `abi::USER_TOP`, both `Err(CloneError::Range)` with nothing charged.
- Calibration: `cow_clone_charge_matches_frames` compares `free()` before and after a successful clone (parent kept alive) against `charged * PAGE_SIZE`; a mismatch means the geometry assumption or the root accounting is wrong. This is also the first real use of the probe.

The existing ktests `cow_clone_isolates_writes_and_charges_the_writer` (Agent budget 4, one COW break charged 1) and `cow_fault_over_budget_stops_the_thread` (budget 0, `Exit::Budget`) stay valid: break charging is unchanged. Their `cow_child` helper (user.rs near 385) and the two ipc.rs callers (ipc.rs:874 and 972) must be updated to the new signature and `.unwrap()` the `Result` in tests only.

**`handle_cow_fault` failure modes.**
- Address at or above `abi::USER_TOP`: `Ok(false)` (vm.rs:71-73).
- `cursor_mut` `Err`: `Ok(false)` (vm.rs:78-80).
- `c.query().unwrap()` at 81: cannot fail once `cursor_mut` accepted the one-page range. Replace with `let Ok((_, item)) = c.query() else { return Ok(false) };` at no cost.
- Not a COW mapped RAM page: `Ok(false)` (genuine fault).
- Frame allocation failure: `AgentError::NoMemory` (vm.rs:101-103). Not injectable.
- Charge refused: `AgentError::Budget`.
- `c.jump(page).unwrap()` at 107 after `c.unmap(PAGE_SIZE)`: the page is inside the cursor's range, so it cannot fail; replace with an `expect`-free `if c.jump(page).is_err() { return Ok(false) }`? No: by then the old mapping is gone. Keep the invariant as a documented `unreachable`-style comment, or restructure to `protect`-style replace so there is no unmap-then-map window. Smallest change: keep the unwrap and say why in the comment (the range is `page..page + PAGE_SIZE`, `jump(page)` is the range start).
- `c.map(new.into(), prop)` after `unmap`: cannot hit "already mapped".
- The explicit `c.unmap(PAGE_SIZE)` frees the old frame through the TLB `frame_keeper` and `RcuDrop` (Pitfall 15), so a COW break leaves the old frame allocated until a grace period passes.

**The futex mapping.** [VERIFIED: kernel/src/futex.rs:88] `handle_cow_fault(space, t.agent(), va).map_err(|_| FutexError::Budget)?;` and the enum: `/// Breaking copy-on-write exceeded the agent's memory budget.` / `Budget,` with `Self::Budget => 12`. [VERIFIED: kernel/src/syscall.rs:71-79] `AgentError::Budget => Errno::ENOMEM,` / `AgentError::NoMemory => Errno::ENOMEM,` / `AgentError::Cap(c) => c.into(),`. Finding for the planner: the two errors `handle_cow_fault` can return are both `ENOMEM` (12) today, so the number a user sees is already right; what is wrong is that the cause is erased and the doc comment is false for `NoMemory`. The `Ok(false)` result is not a bug here: a read-only, non-COW page falls through to the query, which returns the frame (futex wait on a read-only word is legal), and an unmapped page returns `FutexError::Fault`. So implement D-05 as: `impl From<AgentError> for FutexError` with `Budget -> Budget`, `NoMemory -> NoMemory` (new variant, errno 12, doc "the allocator could not serve the copy"), `Cap(_) -> Fault` (unreachable; do not map to a budget error); change line 88 to `handle_cow_fault(...)?;` with `.map_err(FutexError::from)`; replace the two `unwrap()`s at futex.rs:91-92 with `map_err(|_| FutexError::Fault)`. The ktest drives `futex::wait` on a COW page with `Agent::new_root(1, 0)` and asserts `Err(FutexError::Budget)` and errno 12, and a second one that matches on `NoMemory` through `From` directly (the allocator branch is not forceable). If D-05's intent was a different errno for the NoMemory case, that is unsettled: Linux returns `ENOMEM` for a futex fault the kernel cannot satisfy, so keep 12. Out-of-scope neighbour: `user.rs:273-285` still maps any `handle_cow_fault` error to `Exit::Budget` with a `shortcut:` comment (user.rs:279); leave it, note it in the SUMMARY.

## Carried-over patterns (re-verified against the current tree)

### Pattern 1: Space and Thread objects, `thread_create` (KERN-04)
**What:** Add `Object::Space(Arc<Space>)` and `Object::Thread(Weak<Thread>)` (OD-14: Thread caps `Weak`, Space caps `Arc`, tombstone Space) to `cap::Object`. `thread_create(space_cap, ctx, tag)` resolves the Space cap with a CREATE-style right, builds a `Thread` through `Thread::new_tagged`, and spawns a task. A Thread cap carries a SIGNAL right that `interrupt(entry)` requires.
- `Object` today [VERIFIED: kernel/src/cap.rs:40-43]: `pub enum Object {` / `Endpoint(Arc<Endpoint>),` / `Agent(Arc<Agent>),` / `}`.
- Rights today [VERIFIED: kernel/src/cap.rs:15-19]: `pub const SEND: Self = Self(1);` / `pub const RECV: Self = Self(2);` / `pub const GRANT: Self = Self(4);` / `pub const ALL: Self = Self(7);`. `ALL` and `Rights::from_bits` must be extended together or a new right is silently masked (Pitfall 9).
- `Thread::new_tagged(agent: Arc<Agent>, tag: u64) -> Arc<Self>` is the kernel half of `thread_create`; `NEXT_TID` is `static NEXT_TID: AtomicU64 = AtomicU64::new(1);` and never reused. [VERIFIED: kernel/src/thread.rs:99, 131, 135]
- OSTD: `TaskOptions::new(f).data(x).spawn()`, `Task::current()`, `Task::data()`. [CITED: ostd-0.19.0/src/task/mod.rs, read in the earlier session]
- **Hazard:** `Thread -> Arc<Agent> -> CSpace -> Cap -> Object::{Space,Agent}` cycles. Break them by the explicit CSpace drain in `retire()` (D-01) and by making Thread caps `Weak`. `[ASSUMED]` that no other cycle exists; the `live_agents`/`Drop` counters and `Arc::strong_count` assertions in the ktests are the check.
- Cross-space thread creation: the new thread's `Thread.space` is the target Space's `VmSpace`; the post-schedule handler `activate_current_space` (sched.rs:43-48) activates per task. The new thread's agent is the Space owner's, so charging follows the Space.

### Pattern 2: Process teardown (KERN-08), two levels (D-01)
Process level (`Space::teardown`, idempotent, ordered):
1. Mark dying: new `thread_create` and calls into the Space fail.
2. For each thread: `interrupt` it and wait for task exit (or mark dead and drop from the run queue if parked).
3. Futex: remove the dead threads' waiters from `futex::TABLE` and wake the other waiters on keys that belonged to the Space. `TABLE` is `static TABLE: SpinLock<BTreeMap<usize, Bucket>>` keyed by physical address with no per-thread tracking. [VERIFIED: kernel/src/futex.rs:77] A per-thread waiter list is new work.
4. `DEADLINES`: drop entries whose waker belongs to a dead thread. [VERIFIED: kernel/src/thread.rs:36] `static DEADLINES: SpinLock<Vec<(u64, Arc<Waker>)>, LocalIrqDisabled>`. Private static: add a reader and a purge function.
5. Parked `ReplyWait`s of a dead server: answer each with an error so the caller wakes. The shortcut is stated at [VERIFIED: kernel/src/ipc.rs:293] `// shortcut: a server that never answers, or whose thread is gone, strands this caller`.
6. Unmap the owned `VmSpace`, then `owner.uncharge_pages(ledger.swap(0))`, then `owner.leave_space()`.
7. `FPU_OWNER`: [VERIFIED: kernel/src/thread.rs:91] `static FPU_OWNER: SpinLock<Option<SharedFpu>, LocalIrqDisabled>`. Clear it if it points at a dead thread's FPU, or accept the pin.
Agent level: `retire()` as in the D-01 and D-02 design above.
Frames free on the last `Frame` reference; CoW-shared frames free when both sides drop; explicit `unmap` frees RCU-delayed (D-03 section and Pitfall 15).

### Pattern 3: Idle task (KERN-07)
OSTD has no idle task. When nothing is queued the scheduler loop retries: [VERIFIED: ostd-0.19.0/src/task/scheduler/mod.rs:585] `ReschedAction::Retry => {` (returned at 484 and 541). That is a 100% busy spin on the vCPU.
**Fix, safe:** a kernel `Task` in the `BudgetScheduler` run queue whose body is `loop { ostd::task::halt_cpu(); }`. [VERIFIED: ostd-0.19.0/src/task/preempt/mod.rs:23] `pub fn halt_cpu() {`; the body (read earlier) disables local IRQs, checks `cpu_local::need_preempt()`, and otherwise calls `enable_local_and_halt()` then `might_preempt()`. No `unsafe` in `aok`, no OSTD patch.
**Scheduler protocol** (ostd `task/scheduler/mod.rs`, read earlier): `pick_next` is called only after `update_current` returned true; an empty queue means `update_current` returns false; a `Wait` with a non-empty queue means true; `dequeue_current` must call `schedule_info().cpu.set_to_none()` (sched.rs:119-123 does).
**Policy in `BudgetScheduler`** (sched.rs, 234 lines; no idle task today, run queue is `RunQueue { current, queue }` at 75-79):
- `eligible()` is `thread_of(task).is_none_or(|t| t.agent().has_cpu(now))` (sched.rs:54-56), so every kernel task is unbudgeted. The idle task must be told apart from other kernel tasks (the ktest runner is one). A marker type in `Task::data()` works. `[ASSUMED]`.
- `pick`: first eligible non-idle task; else idle. Never `None` and never the running task after `update_current` asked for a switch.
- `update_current`: idle current returns true once any non-idle task is eligible; a non-idle current with only idle left returns true on `Wait`/`Exit`, and false on `Tick` unless a better candidate exists. Idle must not count in `others`.
- `enqueue` currently always returns `None` (sched.rs:136-137); return `Some(CpuId::bsp())` when the current task is idle, so an IRQ wake sets `need_preempt` instead of waiting up to a tick.
**Test seam:** `install()` cannot run in a ktest (the runner is a task). Protocol tests are run-queue policy unit tests as `out_of_budget_agent_is_skipped_until_the_next_period` already is (sched.rs:182-212). The halt itself is checked by the boot check: count idle iterations over N ticks (about N versus millions).

### Pattern 4: Timeouts and timer-to-endpoint (KERN-09)
- Futex and `recv` timeouts pass `Some(deadline)` to `thread::block_until` (thread.rs:248). The futex site: [VERIFIED: kernel/src/syscall.rs:238] `// shortcut: no timeouts; add one with a timer callback when pthread_cond_timedwait`. `FutexError::TimedOut` and `ETIMEDOUT = 110` already exist.
- OSTD timer: [VERIFIED: ostd-0.19.0/src/timer/mod.rs:25, 34] `pub const TIMER_FREQ: u64 = 1000;` and `pub fn register_callback_on_cpu<F>(func: F)`. Callbacks run in IRQ context and cannot be unregistered, so the kernel keeps exactly one (`arm_deadline_timer` guarded by `DEADLINE_TIMER: AtomicBool`, thread.rs:37-42). Resolution is 1 ms.
- Tolerance to state in the plan: request +1 to +3 ticks at `-smp 1` with no competing CPU-bound thread in budget (OD-09). `[ASSUMED]`, to be confirmed by the first TCG run; tests assert a window, never equality.
- Timer to endpoint: the callback only wakes. The `Endpoint` queue lock is not IRQ-safe and `push` allocates, so a kernel timer task sleeps on a `WaitQueue`, is woken by the callback through `Waker::wake_up`, drains an armed-timer table in task context and calls `Endpoint::push`. Stamp tid 0 and the tag given at arm time with a distinct label that ring 3 cannot send (as `LABEL_CANCEL` is rejected from ring 3).

### Pattern 5: Exception and exit notification (KERN-10)
`user::deliver` carries the shortcut [VERIFIED: kernel/src/user.rs:209] `// shortcut: `cap::Object` has no thread type, so the message does not name the faulting thread`. OD-06 and OD-18 settle the shape: a fault is a kernel-made call to the set-once notify endpoint of the Space, naming `tid` and `tag`, verdict in reply `words[0]`; thread exit sends a notification too. `Exit {Code, Fault, Budget, Exception}` already classifies exits. The two-forked-siblings criterion needs distinct `tid` and `tag` on siblings that share a COW parent.

### Pattern 6: Saved-context signals and `sigreturn` (KERN-11)
On delivery, push `{ UserContext clone, FpuContext clone, FsBase }` onto a per-thread kernel `Vec`, then set rip/rdi to the handler (skipping the 128-byte red zone, OD-16). `sigreturn` is cap-less, pops the frame, and overwrites the live context, FPU and FS base. The kernel never writes to the user stack. Depth bound 8 (OD-01); each frame holds up to a 4 KiB XSAVE area.
- `UserContext: Clone`, `FpuContext: Clone` (`impl Clone for FpuContext`, ostd arch/x86/cpu/context/mod.rs:646-652), `FsBase::{save,load}` safe public methods (114-136). [CITED: ostd-0.19.0, read in the earlier session]
- Restore path: `syscall_return` picks sysret versus iret by comparing saved rcx/r11 to rip/rflags; a restored clone with rcx != rip takes iret and keeps every GPR. `RawUserContext` is private, so restore by assigning the whole `UserContext`.
- A context saved at a syscall boundary has rcx and r11 clobbered by `syscall`, as on Linux; document it.
- **FPU subtlety:** live FPU state may sit in the CPU, not in `Thread.fpu`. `FPU_OWNER` is one global (thread.rs:90 `// shortcut: one global owner because the kernel runs `-smp 1`; SMP needs a per-CPU owner.`). Before cloning at delivery, if `FPU_OWNER` is this thread's FPU, save the live state into `thread.fpu` first. On `sigreturn`, copy the saved state back and force a reload on the next `pre_user_run`.
- FS base is neither saved by the scheduler nor by `UserContext`; ring 3 can `wrfsbase`. Save after each run, load in `pre_user_run`, and include it in the frame.

### Pattern 7: `mmap`, `munmap`, `mprotect`, `brk` (AGNT-02)
Cap-less calls on the caller's own `VmSpace` (OD-02). Each mapped page calls `charge_pages(n)` first (D-04); `Budget` maps to `-ENOMEM` (`abi::errno::ENOMEM = 12`). Eager charging at map time, not on first touch. OSTD: `VmSpace::cursor_mut`, `CursorMut::{map, unmap, protect_next}`, `FrameAllocOptions::new().zeroed(true).alloc_frame()`. `munmap` uncharges what it unmapped. `SIGXCPU` for CPU exhaustion belongs to SIG-03, not here (OD-12).

### Pattern 8: AGNT-05 request ledger (OD-05 option a)
Each server (procmgr, vfs) keeps a per-client request ledger keyed by the kernel-stamped tag and answers `EAGAIN` over a documented limit. Server CPU spent for a client is exempt from the client's kernel budget because the server's own Agent runs the work. The kernel part of Phase 2 is documentation: a PROJECT.md edit stating kernel CPU limits do not cover server-side work, and the 02-03 `checkpoint:decision`. No kernel code.

### Anti-Patterns to Avoid
- **Pushing to an `Endpoint` from a timer callback.** IRQ context, non-IRQ-safe lock, allocation.
- **Relying on `Drop` of the Space to free everything.** Cycles through the CSpace keep objects alive; drain explicitly.
- **Dropping drained capabilities while holding the CSpace lock.** Re-entrant `Drop` chains (Endpoint, Space, Agent) then deadlock on a non-reentrant `SpinLock`.
- **Treating the free-frame probe as the primary signal or as an exact count.** RCU delay, the `ACTIVATED_VM_SPACE` pin and slab growth make it a gate with a tolerance; counters are primary.
- **Calling `Task::yield_now()` in a loop as a settle step.** With no second runnable task it switches nothing and RCU never advances.
- **Letting `sigreturn` take user-supplied register state.**
- **Letting the idle task park or take budget.**
- **Making a Space's page-table charge depend on data the kernel cannot recompute at teardown.** Record the charge in a ledger on the Space; do not recompute it by walking tables.
- **Revoking on retire.** D-01 forbids it; do not wire `Forwarder` revocation into `retire()`.
- **Inlining `syscall` through crates such as `rustix` in userland.**

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CPU halt in idle | inline asm `hlt` | `ostd::task::halt_cpu()` | Handles the IRQ-enable/need_preempt race; no `unsafe`. |
| Free-frame count | A tracked frame-meta module or a probe-by-exhaustion loop | `osdk_frame_allocator::load_total_free_size()` + `osdk_heap_allocator::load_total_slab_size()` | Safe, already linked into the base crate; the frame-meta route adds the first `unsafe` ledger row (D-03 forbids it). Exhaustion probing allocates the very memory it measures. |
| Saving user registers | field-by-field copy of `RawUserContext` | `UserContext::clone()` | `RawUserContext` is private; clone is complete. |
| Saving FPU/SSE/AVX | `fxsave` asm | `FpuContext::clone()`, `save()`, `load()` | XSAVE feature mask handled by OSTD. |
| FS/GS base | `wrfsbase` asm | `FsBase::{save,load}` | Safe wrappers exist. |
| Sleep/wake | spin on a flag | `WaitQueue`, `Waiter`, `Waker`, existing `block_until` | Cancel/close semantics already handled. |
| Timeouts | a second timer callback per wait | `DEADLINES` + the single callback | OSTD has no unregister. |
| Atomic budget check-and-add | load, compare, store | `try_update` as `charge_pages` does (agent.rs:104-110) | Lock-free and race-free. |
| Task spawn | raw `Task` struct | `TaskOptions::new(f).data(..).spawn()` | The only public constructor. |
| Settle loop peer | a hand-written sleeping task | `TaskOptions::new(\|\| {}).spawn()` plus `yield_now` until a flag is set | Forces real context switches, hence RCU grace periods. |

**Key insight:** OSTD exposes every hardware-touching primitive as a safe function. What remains is bookkeeping (who owns what, in what order it is released), which is where leaks and deadlocks live.

## Runtime State Inventory

Kernel-internal state that survives "the process is gone" unless teardown unwinds it. Source: reading the files listed under Sources.

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | `Agent.mem_used_pages` (`AtomicUsize`); the parent's `mem_limit_pages` charge and `cpu_carved_ticks` carve; per-Space charge ledger (new) | Uncharge on teardown; return the carve to the parent on `retire()` (code edit with the parent link). No persisted data, so no data migration. |
| Kernel registries | `futex::TABLE`; `thread::DEADLINES`; `FPU_OWNER`; thread and Space registries (new); armed-timer table (new); each server's `ReplyTable`; run queue `RunQueue { current, queue }` | Remove dead-thread entries from each; answer parked callers with an error; expose a counter reader for each (all are private statics today). |
| Capabilities | Each `CSpace` slot (up to `MAX_SLOTS = 65_536`); `Forwarder` targets hold no child registry | `drain()` and `close()` on retire. Derived copies in other agents are deliberately left (D-01). |
| Per-CPU hardware state | `ACTIVATED_VM_SPACE` holds a strong `Arc` of the last activated space | The last process stays resident until another space activates; activate a dummy space before the second probe read. |
| Messages in flight | Messages queued on an `Endpoint` carry `Vec<Cap>`; IPC staging buffers (now charged, D-04) | Dropped with the endpoint; charge returned when consumed or dropped; confirm no strong cycle through `Arc<ReplyWait>`. |
| OS-registered state, secrets, env vars, build artifacts | None. Verified by reading: no files, services or env vars are named by this phase's code. | None. |

**This is not a rename phase.** The table is kept because teardown has the same shape as the canonical question: after every Space and Agent is gone, which kernel registries still hold something?

## Common Pitfalls

### Pitfall 1: OSTD busy-spins and the CPU never halts
KERN-07 appears to work but the vCPU runs at 100% (`ReschedAction::Retry` loops). **Avoid:** idle task in our queue and a boot check that counts idle iterations.

### Pitfall 2: Arc cycles hide leaks
`Thread -> Arc<Agent> -> CSpace -> Cap -> Object::Space -> Thread`; `Drop` never runs and counts creep in the 10,000x loop. **Avoid:** drain the CSpace in `retire()`, `Weak` Thread caps, `Drop`-counted `live_agents`.

### Pitfall 3: `ACTIVATED_VM_SPACE` pins the dead space
A measurement right after teardown sees one process's frames still held. **Avoid:** activate a dummy space; D-03 tolerance of one footprint.

### Pitfall 4: Futex waiters of a dead space are never woken
`TABLE` has no back-reference from a thread to its bucket entries. **Avoid:** per-thread waiter tracking and a teardown sweep; test with a waiter on a word in a Space that is then killed.

### Pitfall 5: Stranded callers
A client blocked in `call` to a server that dies never returns, and an interrupt no longer frees it. **Avoid:** teardown answers every parked `ReplyWait` of the dead server; include the `Received(None)` kernel-task receive.

### Pitfall 6: IRQ-context work
Deadlock or allocation in IRQ context from a timer callback. **Avoid:** callbacks only call `Waker::wake_up`.

### Pitfall 7: FPU live state versus saved state at signal time
The frame captures stale XMM state. **Avoid:** flush if `FPU_OWNER` is this thread, then clone; reload on `sigreturn`.

### Pitfall 8: FS base not saved
After a switch or `sigreturn` musl TLS points at the wrong block. **Avoid:** save after each run, load in `pre_user_run`, include in the frame.

### Pitfall 9: Rights masking
A new rights bit silently vanishes at derive or on the wire. **Avoid:** extend `Rights::ALL`, `from_bits` and `bits` together; a ktest per right.

### Pitfall 10: Scheduler decision and switch are not atomic
OSTD's own FIXME (scheduler/mod.rs 594-599). With `-smp 1` the window is IRQs-off. **Avoid:** keep the scheduler lock `LocalIrqDisabled`; claim no SMP safety.

### Pitfall 11: Timer resolution assumed finer than 1 ms
**Avoid:** state the tolerance; tests assert a window.

### Pitfall 12: Fuzz KNOWN errno list
`fuzz_syscalls_never_panic` checks results against `const KNOWN: [isize; 11]` [VERIFIED: kernel/src/syscall.rs:465, used at 578]. A new errno (`ESRCH`, `ENOTSUP` are not in `abi::errno`) must extend `abi::errno` and `KNOWN` together. D-05's `CloneError` and `FutexError::NoMemory` use only EINVAL and ENOMEM and need no change.

### Pitfall 13: `retire()` drops caps under the CSpace lock
Dropping a drained `Cap` can run `Endpoint::drop` and `Space::drop`, which call back into `leave_space` or take the same lock. **Avoid:** take the `Vec<Cap>` out under the lock, release, then drop. `leave_space`/`retire` must be idempotent on a non-`Live` state.

### Pitfall 14: Carve returned while a grandchild is live
The parent can re-carve memory the grandchild still spends, so the sum of live limits exceeds the root's. **Avoid:** return the carve only at `children == 0` and cascade (Open decision 1).

### Pitfall 15: Explicit `unmap` frees late, whole-space drop frees at once
Explicit `CursorMut::unmap` hands frames to the TLB `frame_keeper` and RCU; dropping the whole `VmSpace` frees page-table nodes synchronously. A COW break (`unmap` then `map`) therefore leaves the old frame allocated until a grace period passes, and a probe taken right after a loop of breaks reads low. **Avoid:** settle before reading; never read the probe inside the loop.

### Pitfall 16: The settle loop does not switch
`yield_now` with no other runnable task returns `DoNothing`; RCU advances only in `switch_to_task` (task/processor.rs:45). A settle loop that only yields is a no-op. **Avoid:** a real peer task per round.

### Pitfall 17: The charge ledger drifts from reality
If a path charges but the ledger is not updated (or the reverse), the loop test shows a creeping `mem_used_pages` that no frame probe explains. **Avoid:** one `Payer` implementation that charges and records in the same call; teardown uncharges `ledger.swap(0)`, never a recomputed number.

### Pitfall 18: Partial-clone leaves the parent read-only
After `cow_clone` fails midway the parent's visited pages are COW and read-only. This is benign (sole-owner takeover, no charge, vm.rs:85) but a test that asserts "unchanged page flags" would fail. **Avoid:** assert behaviour (a write succeeds with no charge), not flags.

### Pitfall 19: Charging after allocating a multi-page staging buffer
`vec![0u8; len]` before the charge lets a hostile `len` allocate first. **Avoid:** charge first, refund on failure (D-04 table).

### Pitfall 20: Auto-retire destroys an agent between commands
D-02 consequence: an agent with zero processes is gone. A test that spawns an agent, runs a process, then expects the agent to answer the next call fails. **Avoid:** tests that need a persistent agent hold a root, or hold a Space open; document it in the plan.

### Pitfall 21: Counter readers for private statics
`DEADLINES`, `FPU_OWNER` and the futex `TABLE` are private. A counter module that reaches them needs `pub(crate)` readers added in their own modules; do not make the statics public.

### Pitfall 22: Long loop under TCG
10,000 iterations of process creation under TCG may take minutes. **Avoid:** `const ITERATIONS`, log the measured wall time, and keep a small-N variant as the default `cargo osdk test` filter if the full loop exceeds the runner timeout (the timeout in `OSDK.toml` was not checked; Open question 2).

## Code Examples

Sketches, not compiled. Signatures from OSTD 0.19.0 source and the current `aok` tree; every `aok` name not quoted above is a proposal.

### Fallible `cow_clone` with a payer and an unwind

```rust
// Proposal. Payer is implemented by Space (charge + ledger) and by Agent for tests.
pub trait Payer {
    fn charge_pages(&self, pages: usize) -> Result<(), AgentError>;
    fn uncharge_pages(&self, pages: usize);
}

pub enum CloneError { Range, Unsupported(Vaddr), Budget }

pub struct Cloned { pub space: VmSpace, pub charged: usize }

pub fn cow_clone(parent: &VmSpace, range: Range<Vaddr>, payer: &dyn Payer)
    -> Result<Cloned, CloneError>
{
    let mut charged = 0usize;
    let mut charge = |n: usize| -> Result<(), CloneError> {
        payer.charge_pages(n).map_err(|_| CloneError::Budget)?;
        charged += n;
        Ok(())
    };
    charge(1)?;                                  // child root (PML4)
    let child = VmSpace::new();
    let result = (|| {
        let guard = disable_preempt();
        let mut pc = parent.cursor_mut(&guard, &range).map_err(|_| CloneError::Range)?;
        let mut cc = child.cursor_mut(&guard, &range).map_err(|_| CloneError::Range)?;
        let mut prev: Option<Vaddr> = None;
        while let Some(va) = pc.find_next(range.end - pc.virt_addr()) {
            let (r, item) = pc.query().map_err(|_| CloneError::Range)?;
            let VmQueriedItem::MappedRam { frame, prop } = item else {
                return Err(CloneError::Unsupported(va));
            };
            if r.len() != PAGE_SIZE { return Err(CloneError::Unsupported(va)); }
            charge(nodes_for(prev, va))?;        // BEFORE the map
            // ... protect parent page RO+COW, cc.jump(va), cc.map(frame, cow_prop) ...
            prev = Some(va);
        }
        Ok(())
    })();
    // the TLB flush for `range` runs here on both Ok and Err
    match result {
        Ok(()) => Ok(Cloned { space: child, charged }),
        Err(e) => { payer.uncharge_pages(charged); drop(child); Err(e) }
    }
}

/// 3 for the first page (L3, L2, L1), then one per newly crossed 2 MiB / 1 GiB / 512 GiB window.
fn nodes_for(prev: Option<Vaddr>, va: Vaddr) -> usize {
    match prev {
        None => 3,
        Some(p) => (va >> 21 != p >> 21) as usize
            + (va >> 30 != p >> 30) as usize
            + (va >> 39 != p >> 39) as usize,
    }
}
```
The cursor method names (`find_next`, `virt_addr`, `protect_next`) must be taken from the existing body (vm.rs:27-59); the sketch shows only the error and charge structure. `nodes_for` relies on `[ASSUMED]` 512-entry nodes (21/30/39 shifts); the `5`/`4` budget ktest pins it.

### Free-frame probe and settle loop

```rust
// Proposal, #[cfg(ktest)] only. Both calls are safe pub fns (see Standard Stack for quotes).
fn free() -> usize {
    osdk_frame_allocator::load_total_free_size() + osdk_heap_allocator::load_total_slab_size()
}

fn settle(rounds: usize) {
    for _ in 0..rounds {
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        TaskOptions::new(move || d.store(true, Ordering::Release)).spawn().unwrap();
        while !done.load(Ordering::Acquire) { Task::yield_now(); }
    }
}
```

### Retire skeleton

```rust
// Proposal. Life is guarded by one SpinLock so enter_space / retire cannot interleave.
impl Agent {
    pub fn leave_space(self: &Arc<Self>) {
        let retire = {
            let mut l = self.life.lock();
            l.spaces = l.spaces.saturating_sub(1);
            l.spaces == 0 && self.parent.is_some() && l.state == State::Live
        };
        if retire { self.retire(); }
    }

    fn retire(self: &Arc<Self>) {
        let (caps, finish) = {
            let mut l = self.life.lock();
            if l.state != State::Live { return; }
            l.state = State::Retiring;
            let caps = self.cspace.lock().drain_and_close();  // lock released at end of block
            (caps, l.children == 0)
        };
        drop(caps);                                           // outside every lock
        if finish { self.return_carve(); }
    }

    fn return_carve(&self) {
        if let Some(p) = self.parent.as_ref().and_then(Weak::upgrade) {
            p.uncharge_pages(self.mem_limit_pages);
            p.cpu_carved_ticks.fetch_sub(self.cpu_quota_ticks, Ordering::AcqRel);
            p.child_retired();      // children -= 1; may complete a Retiring parent
        }
        self.life.lock().state = State::Retired;
    }
}
```
Lock order: `life` before `cspace`; never hold either while dropping a `Cap`. The nested `life` then `cspace` acquisition above is an ordering the planner must either keep everywhere or remove by draining after the `life` block.

## State of the Art

| Old Approach (B2 and Phase 1) | Current Approach (Phase 2) | Impact |
|-------------------------------|----------------------------|--------|
| Redirect rewrites rdi/rip, no saved frame | Kernel-held context + FPU + FS base stack, `sigreturn` | No user-stack writes; full register survival |
| Spinning scheduler fallback | Idle task + `halt_cpu` | CPU halts |
| FUTEX_WAIT without timeout | `block_until` with deadline | `pthread_cond_timedwait` possible |
| Exception message without thread identity | `tid`/`tag` stamped by kernel | procmgr can attribute faults |
| Carved budget never returned | Parent link + `retire()` + carve return | Fork loops do not exhaust the parent |
| `cow_clone` panics, charges nothing | `Result`, charged page-table nodes, unwinds (D-05) | Fork under a tiny budget fails with `ENOMEM`, not a kernel panic |
| `handle_cow_fault` error erased to `FutexError::Budget` | `From<AgentError> for FutexError` | Cause preserved |
| Leak evidence: counters only | Counters plus a free-frame gate (D-03) | One check independent of the teardown code |

**Deprecated/outdated:** none. docs.rs for OSTD 0.19.0 failed to build (STATE.md), so source is the only reference; every OSTD or toolchain bump is a deliberate task with the scheduler-protocol ktests as the pinned contract.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `aok` compiles with `osdk_frame_allocator` and `osdk_heap_allocator` as direct dependencies and shares the base crate's statics (not a second copy with zero counters) | Standard Stack, D-03 | The probe reads 0 or does not build; fallback is probe-by-exhaustion, or counters only. Wave 0 spike decides |
| A2 | Settle loop of 8 peer-task rounds drains all RCU-delayed frees in the loop | D-03 | Gate flakes; calibrate by logging free() after 1, 2, 4, 8, 16 rounds |
| A3 | Non-slab heap allocations (large `Vec`s) are not covered by the probe | D-03 | A leak of a large buffer escapes the probe, but the counters catch the kernel objects |
| A4 | x86-64 page-table nodes are 512-entry, shifts 21/30/39, so the node-charge formula and the 5/4 ktest budgets hold | D-05 | The success budget is wrong; the ktest fails and calibrates it |
| A5 | The 10,000x loop finishes within the runner timeout under TCG | D-03, Pitfall 22 | Reduce N for the default run; measured on first run |
| A6 | Timer tolerance of request +1 to +3 ticks at `-smp 1` | Pattern 4 | Timing tests flake |
| A7 | No Arc cycle exists beyond Thread/Agent/CSpace/Object | Pattern 1 | A leak survives; `live_agents` and `strong_count` assertions catch it |
| A8 | Idle task identified by a `Task::data()` marker | Pattern 3 | Implementation detail |
| A9 | `halt_cpu` under `-smp 1` wakes on the next tick and reschedules through `might_preempt` | Pattern 3 | Idle latency; the boot check shows it |
| A10 | Signal frame nesting bound of 8 | Pattern 6 | Deeper nesting dies (OD-01 already signed off) |
| A11 | Behaviour under SMP is out of scope | all | Project fixes `-smp 1` |
| A12 | A dummy-space activation before the second probe read releases the previous pin | D-03 | Tolerance of one footprint absorbs it (D-03 allows it anyway) |
| A13 | Returning `ENOMEM` for `FutexError::NoMemory` is what D-05 intends | D-05 | A different errno wanted; one-line change |

## Open Questions

1. **Do the OSDK allocator crates build as `aok` dependencies?** Both are in the base crate's `Cargo.toml` (lines 12 and 15) but absent from `kernel/Cargo.lock`. Recommendation: a Wave 0 spike task that adds them behind a ktest-only path and prints `free()`; if it fails, fall back to counters plus probe-by-exhaustion and record it.
2. **Runner timeout for the 10,000x loop.** `kernel/OSDK.toml` has a `[boot]` and `[qemu]` section; no timeout key was found by grep. Unknown whether the CI or `check.sh` has an outer timeout. Measure on first run.
3. **Does an exited thread's `Task` free its kernel stack promptly?** The previous `Arc<Task>` is dropped in `after_switching_to`; if a registry keeps an `Arc<Thread>` holding the Task, the stack lives on. Check with a `Weak` in a ktest.
4. **How far below the footprint does the dummy-space trick get the gate?** Decide the final tolerance after the first measured run (Claude's discretion per CONTEXT.md).
5. **Frames in a message's `Vec<Cap>` when a reply lands on a retired agent.** `install_caps` into a closed CSpace must fail cleanly and drop the caps; confirm the error maps to a standard errno (`EBADF` via `CapError::Revoked`).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `/dev/kvm` | ktests, boot check | no | n/a | None for execution. TCG runs through `kernel/osdk.sh` give slow but valid functional evidence; timing results carry the TCG caveat (STATE.md). `sh scripts/check.sh --no-kvm` runs static gates and builds only |
| Docker image `agentic-os-kernel-dev:0.19.0` | `kernel/osdk.sh` | yes (used this session to read the OSDK crates from the cargo volume) | 0.19.0 | `docker build -t agentic-os-kernel-dev:0.19.0 kernel` |
| OSTD 0.19.0 source | research | yes (cargo registry) | 0.19.0 | n/a |
| OSDK allocator crate source | probe | yes, inside docker volume `agentic-os-cargo-registry` only | 0.19.0 | `docker run` to read |
| `osdk_frame_allocator` / `osdk_heap_allocator` as `aok` dependencies | D-03 probe | unverified | 0.19.0 (`[ASSUMED]`) | Counters plus probe-by-exhaustion |

**Missing dependencies with no fallback:** KVM for KVM-grade timing evidence. All Phase 2 timing assertions are TCG-grade (D-03 and STATE.md blocker).
**Missing dependencies with fallback:** the allocator-crate dependency (counters only).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | OSTD ktest via `cargo osdk test` (QEMU, `-smp 1`, isa-debug-exit) plus the boot check via `cargo osdk run` |
| Config file | `kernel/OSDK.toml`, `kernel/rust-toolchain.toml` |
| Quick run command | `cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test <filter>'` |
| Full suite command | `sh scripts/check.sh` (unsafe_check first, then userland build/fmt/clippy, kernel fmt/clippy, `cargo osdk test`, `cargo osdk run` with grep gates); `--no-kvm` is static only |

The boot gate currently requires two lines [VERIFIED: scripts/check.sh:30] `for want in "aok: fpu preemption check passed" "aok: cpu budget check passed"; do` and fails on `FAILED` (line 33). Phase 2 adds `aok: idle halt check passed` and a timer line.

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command (filter) | File Exists? |
|--------|----------|-----------|----------------------------|--------------|
| KERN-04 | Holder creates and interrupts a thread in another space; non-holder refused | ktest + ring-3 bin | `thread_create`, `rights_` | Wave 0 |
| KERN-07 | `pick` falls back to idle; idle never counted as others | ktest unit | `sched_` | Wave 0 |
| KERN-07 | CPU halts (idle iterations about N over N ticks) | boot check | `check.sh` grep `aok: idle halt check passed` | Wave 0 |
| KERN-08 | 10,000x fork+exit: counters at baseline and free-frame gate (D-03) | ktest (long) | `teardown_` | Wave 0 |
| KERN-08 | Child agent retires on last Space, caps drained, carve returned, derived grants survive (D-01, D-02) | ktest | `retire_` (add `retire_keeps_derived_grants`, `retire_returns_carve`, `root_never_retires`, `retire_waits_for_children`) | Wave 0 |
| KERN-08 | Image, stack, staging and COW pages charged and uncharged (D-04) | ktest | `staging_`, `counts_` | Wave 0 |
| KERN-08 | Dead process's futex waiters woken; stranded caller answered | ktest | `kill_` | Wave 0 |
| KERN-08 | `cow_clone` partial failure unwinds; bad ranges rejected; charge matches frames (D-05) | ktest | `cow_clone_partial_failure_unwinds`, `cow_clone_rejects_bad_ranges`, `cow_clone_charge_matches_frames` | Wave 0 |
| KERN-08 | Futex COW failure returns the real cause (D-05) | ktest | `futex_cow_` | Wave 0 |
| KERN-08 | 10,000x loop fails if the probe is low by more than one footprint | ktest (long) | `frame_probe_` | Wave 0 |
| KERN-09 | Timed futex and `recv` expire within tolerance; timer message arrives | ktest + bin | `timeout_futex`, `timeout_recv`, `timer_msg` | Wave 0 |
| KERN-10 | Fault or exit in one of two forked siblings names the right tid and tag | ktest + bin | `note_` | Wave 0 |
| KERN-11 | All GPR and XMM survive redirect + `sigreturn`; user stack unchanged; nesting limit | ktest + bin | `sigreturn_` | Wave 0 |
| AGNT-02 | `mmap` past budget returns `ENOMEM` from ring 3, charging nothing | ring-3 bin + ktest | `vmem_` | Wave 0 |
| AGNT-05 | Ledger exists with a limit; PROJECT.md wording | doc check | `grep -c "server-side work" .planning/PROJECT.md` | manual |

### Sampling Rate
- **Per task commit:** `sh scripts/check.sh --no-kvm`, and the targeted ktest filter through `osdk.sh` (TCG here).
- **Per wave merge:** full `sh scripts/check.sh`.
- **Phase gate:** full suite green before `/gsd-verify-work`. Timing and the 10,000x probe numbers carry the TCG caveat.

### Wave 0 Gaps
- [ ] Spike: add the two allocator crates as ktest-only dependencies; print `free()`; confirm statics are shared (A1).
- [ ] `probe.rs`: `free()`, `settle()`, counter snapshot struct with a reader for every item in D-03.
- [ ] `pub(crate)` readers for `futex::TABLE`, `thread::DEADLINES`, `FPU_OWNER`, and the new registries.
- [ ] Scheduler-protocol ktests (idle fallback, `update_current` truth table, `enqueue` preemption hint).
- [ ] Two-address `space_with` variant for the 5/4 partial-clone ktest; update the `cow_clone` callers at ipc.rs:874 and 972 and the `cow_child` helper.
- [ ] Boot-check lines for idle halt and timer in `scripts/check.sh`.
- [ ] Extend `abi::errno` and `KNOWN` only if a plan adds an errno beyond EINVAL, ENOMEM, ETIMEDOUT.

Limit: the idle-halt check and anything needing the injected `BudgetScheduler` run only in the boot check.

## Security Domain

ASVS level 1 at the kernel boundary. V2/V3 (authentication, sessions) do not apply; authority is capability possession (V4 analogue). `security_enforcement` is not disabled.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V4 Access Control | yes | Capability check on every `thread_create`, signal, `sigreturn`; rights shrink only through `derive` (GRANT required); closed CSpace after retire |
| V5 Input Validation | yes | `read_args` bounds, user pointers only through `copy_from_user`/`copy_to_user`, range checks in `cow_clone`; fuzz ktest |
| V6 Cryptography | no | n/a |
| V7 Error handling / DoS | yes | Budgeted memory and CPU, nesting bound on signal frames, charged staging buffers, teardown frees everything |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Non-holder creates or signals a thread in another space | Elevation of privilege | Kernel resolves the Space/Thread cap with a specific right; no ambient lookup by tid |
| Stale capability to a destroyed Space/Thread | Elevation, Tampering | Teardown marks objects dead; ops return error; tids never reused |
| Fork+exit loop exhausting parent budget or memory | DoS | Uncharge, carve return, the 10,000x loop gate |
| Hostile large IPC length allocating before the charge | DoS | Charge first, refund on failure (Pitfall 19) |
| Reply installing capabilities into a retired agent's CSpace | Elevation, memory retention | `CSpace::close()` after drain; `install_caps` fails |
| Budget double-spend through a retiring parent with live grandchildren | Tampering with limits | Return carve at `children == 0` (Pitfall 14) |
| Dead delegator's grants outliving it (D-01) | Accepted by decision | Documented; revocation stays explicit; deferred idea |
| Unbounded signal nesting (up to 4 KiB per frame) | DoS | Depth bound 8 |
| Forged `sigreturn` | Tampering | Kernel-held frame; no register input; rflags masked by OSTD |
| Timer label spoofing from ring 3 | Spoofing | Reject the timer label in user `call` |
| IRQ-context lock inversion | DoS | Wake-only callbacks |

## Project Constraints (from CLAUDE.md)

- Rust on Asterinas OSTD 0.19.0, x86-64 only, `-smp 1`; toolchain `nightly-2026-07-21`; image `asterinas/osdk-dev:0.19.0-20261012`; never bump one without the other.
- `#![deny(unsafe_code)]` crate-wide; `unsafe` only in small named modules with `#[allow(unsafe_code)]`, a `// SAFETY: <ID>` comment, and a row in `docs/architecture/unsafe-ledger.md` (checked by `tools/unsafe_check.py`, first gate of `scripts/check.sh`). The Kernel table stays empty under D-03. An OSTD patch or an uncontainable `unsafe` is the signal to re-score the Redox fork in ADR 0002. **Nothing in this research triggers it.** The allocator-crate dependency is safe code.
- Agent features are additive; they never change POSIX semantics; denials map to standard errno (`ENOMEM`, `EINVAL`, `EBADF`, `ETIMEDOUT`, `EAGAIN`).
- All builds and ktests through `kernel/osdk.sh`; KVM host required for grade evidence; this host is TCG only.
- Docs: wiki claims cite PDF pages; reasoning not stated by a source is marked *(interpretation)*; `raw/` is gitignored and never edited.
- Commits: short imperative subjects; never `Co-Authored-By:` or `Claude-Session:` trailers.
- Deliberate gaps are marked `// shortcut:` comments; keep the convention.
- Edits go through a GSD workflow (this research is part of one).

## Open decisions needing the user

**Resolved 2026-10-26 after this research:** items 1, 2 and 3 are decided as the defaults below
(CONTEXT.md D-08, D-07, D-06); items 4 and 5 took their defaults (D-09). Item 2's sub-question on
`load_image` page tables is left to the planner (same charge basis unless a plan finds a reason).

CONTEXT.md settles D-01 to D-05 and signs off OD-01..OD-19. What it left open:

1. **Retirement with live descendants.** D-01 says the carve returns to the parent; it does not say what happens when the retiring child still has live child agents. Default in this research: return the carve only when the child's `children` count reaches zero, cascading upward (Pitfall 14). Alternative: return at once and let a grandchild's later retire clamp. The default costs one counter and a cascade path; the alternative allows budget double-spend.
2. **Charging basis for `cow_clone` (D-05).** Default: charge the child's page-table node frames only (root + computed nodes), so a tiny budget forces a mid-clone failure. The shared data frames stay charged to the parent, the known under-charge gap documented in 02-06. Alternative: reserve one page per mapping up front, which also fails fast but rewrites the existing `cow_clone_isolates_writes_and_charges_the_writer` expectation. Does the same charge basis apply to `load_image` spaces' page tables?
3. **OD-08 versus the allocator crates.** OD-08 said "no new crate". The free-frame probe needs `osdk_frame_allocator` and `osdk_heap_allocator` as direct `aok` dependencies, both flagged SUS by the legitimacy check (low downloads only; asterinas repo). Approve a `checkpoint:human-verify` and the dependency, or fall back to counters plus probe-by-exhaustion?
4. **Splitting `Exit::Budget` from NoMemory (user.rs:279 shortcut).** Outside D-05's listed scope. Default: leave it and record it in the SUMMARY.
5. **Errno for `FutexError::NoMemory`.** Default `ENOMEM` (12), the same number as today (A13).

## Sources

### Primary (HIGH confidence)
- OSTD 0.19.0 source at `/home/rakdos/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ostd-0.19.0/src/`: `task/scheduler/mod.rs`, `task/preempt/mod.rs`, `task/processor.rs`, `task/mod.rs`, `timer/mod.rs`, `sync/rcu/monitor.rs`, `mm/vm_space.rs`, `mm/page_table/{mod.rs,node/mod.rs,cursor/mod.rs}`, `mm/frame/allocator.rs`, `error.rs`, `arch/x86/mm/mod.rs`, `arch/x86/cpu/context/mod.rs`, `user.rs`.
- OSDK base crate allocator source (docker volume `agentic-os-cargo-registry`), `kernel/target/osdk/aok-run-base/Cargo.toml:12,15`.
- Repo, this session: `kernel/src/{vm,agent,cap,sched,futex,thread,user,ipc,syscall}.rs`, `kernel/OSDK.toml`, `scripts/check.sh`, `userland/abi/src/lib.rs`, `.planning/phases/02-*/{02-CONTEXT,02-OPEN-DECISIONS,02-VALIDATION}.md`, `02-0{6,7}-PLAN.md`, `docs/architecture/{b2-spike-report,unsafe-ledger}.md`.
- `gsd-tools query package-legitimacy check --ecosystem crates osdk-frame-allocator osdk-heap-allocator`: SUS on both (low downloads only).

### Secondary / Tertiary
- None. No web sources; docs.rs for OSTD 0.19.0 failed to build (STATE.md).

## Metadata

**Confidence breakdown:**
- Standard stack: MEDIUM. No new crate except the two allocator crates, whose build as direct dependencies is unverified (A1).
- Architecture / OSTD APIs: HIGH for signatures and semantics read in source; MEDIUM for scheduler-idle interaction (reasoned, not run).
- Retire and teardown design: MEDIUM. The object graph was reasoned and not compiled.
- `cow_clone` failure model: HIGH on what OSTD panics versus returns (source read); MEDIUM on the node-charge formula (A4).
- Free-frame probe and settle loop: MEDIUM. RCU mechanics read in source; sizing is assumed (A2).
- Timer tolerance: LOW, assumed.

**Research date:** 2026-10-26
**Valid until:** the OSTD or toolchain pin changes, or the plans are replanned and alter task structure.

