# Phase 2: Process objects, teardown and timers - Pattern Map

**Mapped:** 2026-10-25
**Files analyzed:** 19 (9 kernel modified, 2 kernel new, 1 userland lib, 5 test bins, 2 non-code)
**Analogs found:** 19 / 19 (3 are role-match only; see No Analog Found)

All analog paths were confirmed git-tracked (`git ls-files kernel userland scripts`). Line numbers are HEAD (01-06 landed, 01-07 not run).

**01-07 caveat.** 01-07 deletes the B2 loader, `prog`, `run`, `user::load` and the ktest `PROGRAM` array. Analogs marked **[01-07 REMOVES]** must not be copied. Specifically `sched.rs` ktest `each_task_resumes_in_its_own_address_space` (uses `user::load`, `user::run`, `prog::YIELDER`), `lib.rs:96-102` (`cpu_budget_check` spinners using `prog::SPINNER`/`HANDLER`) and `syscall.rs` tests built on `PROGRAM`. Use the Rust-bin style instead: `user_image!("name")` + `user::load_image` + `user::start` (shown in `lib.rs:45-57`) and the bins under `userland/tests/src/bin/`.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `kernel/src/sched.rs` (idle, pick fallback, enqueue hint, protocol ktests) | scheduler | event-driven | itself (`RunQueue`, `Scheduler for BudgetScheduler`, `mod test` task 1-207) | exact |
| `kernel/src/cap.rs` (`Object::{Space,Thread}`, new Rights) | model | CRUD | itself, `Object` 40-43, `Rights` 13-37 | exact |
| `kernel/src/space.rs` NEW | model/registry | CRUD | `kernel/src/agent.rs` (Arc object, atomics, charge/uncharge, ktests) | role-match |
| `kernel/src/thread.rs` (registry, signal-frame stack, FS base, interrupt) | model | event-driven | itself (`State` 76-85, `FPU_OWNER` 91, `NEXT_TID` 99) | exact |
| `kernel/src/timer.rs` NEW (timer task, armed table) | service | event-driven | `thread.rs` `DEADLINES` + `arm_deadline_timer` 34-55 + `block` 267-310 | role-match |
| `kernel/src/agent.rs` (`uncharge_pages`, carve return, parent link) | model | CRUD | itself, `charge_pages` 101-109, `spawn_child` 144-175 | exact |
| `kernel/src/futex.rs` (timeout, per-thread waiter tracking) | service | request-response | itself, `TABLE` 77, `wait` 123 | exact |
| `kernel/src/vm.rs` (unmap_all, mmap charge) | service | CRUD | itself, `handle_cow_fault` 63 | exact |
| `kernel/src/user.rs` (`deliver` tid/tag, exit notify, signal frame, FS base) | service | request-response | itself, `deliver` 233-268, `run_ctx` 276+ | exact |
| `kernel/src/syscall.rs` (thread_create, sigreturn, timer_arm, mmap, timeouts) | controller | request-response | itself, `dispatch` 292-322 | exact |
| `kernel/src/ipc.rs` (recv timeout, reply-table drain) | service | request-response | itself (read 1-780 only) | exact |
| `kernel/src/lib.rs` (idle halt boot check) | boot check | batch | `fpu_preemption_check` 45-83, `cpu_budget_check` 88-144 | exact |
| `scripts/check.sh` (new grep lines) | config | batch | itself, line 30 | exact |
| `userland/abi/src/lib.rs` (syscall nos, errnos, ESRCH) | config | transform | itself (`errno`, `SYS_*`) | exact |
| `userland/libsys/src/lib.rs` (wrappers) | utility | request-response | itself, `recv` 39-45 | exact |
| `userland/tests/src/bin/*.rs` new bins (fork_exit, thread_create, sigreturn, mmap_over, timeout) | test bin | request-response | `park_srv.rs`, `fpu.rs`, `sibling.rs` | role-match |
| `docs/architecture/...` AGNT-05 record, `.planning/PROJECT.md` wording | doc | n/a | none | none |

## Pattern Assignments

### `kernel/src/sched.rs` (scheduler, event-driven) - slice A

**Analog:** itself.

**Task-kind dispatch** (lines 37-39, 54-56): kernel tasks are those without `Arc<Thread>` data. Idle needs a distinct marker; add a second `downcast_ref` beside `thread_of`.
```rust
fn thread_of(task: &Task) -> Option<&Arc<Thread>> { task.data().downcast_ref() }
fn eligible(task: &Task, now: u64) -> bool {
    thread_of(task).is_none_or(|t| t.agent().has_cpu(now))
}
```
**Protocol core to modify** (81-104): `update` computes `others` over `queue`; idle must be excluded from `others`, `pick` must fall back to idle instead of `?` returning None (line 97), and idle current returns true when any non-idle is eligible.
```rust
fn pick(&mut self, now: u64) -> Option<&Arc<Task>> {
    let i = self.queue.iter().position(|t| eligible(t, now))?;
```
**enqueue** (126-138): keep the `set_if_is_none` handshake verbatim; change the final `None` to `Some(CpuId::bsp())` when current is idle.
**dequeue_current** (119-123) already calls `schedule_info().cpu.set_to_none()`; keep for idle.
**Test style** (164-207): build `RunQueue::default()`, push `task(...)` helpers, assert with `rq.pick(now)`/`rq.update(flags, now)` and `is()`. Copy `task()`, `is()`, and `out_of_budget_agent_is_skipped_until_the_next_period` for the truth-table ktests (idle fallback, idle not in "others", Tick preempts idle, enqueue hint). `install()` cannot run in a ktest (header 24-25), so idle halt is a boot check only.
**Do not copy** `each_task_resumes_in_its_own_address_space` (209-236) **[01-07 REMOVES]**.

### `kernel/src/cap.rs` (model, CRUD) - slice B

**Rights** (13-37): add bits (e.g. `CREATE = Self(8)`, `SIGNAL = Self(16)`) and update `ALL` and `from_bits` together (Pitfall 9); `from_bits` masks with `ALL`, so a missed `ALL` silently drops the bit.
**Object** (39-43): add `Space(Arc<Space>)`, `Thread(Arc<Thread>)`. Imports at line 8 grow accordingly.
**Resolve pattern** (105-111): `cap.resolve(Rights::X)?` returns `CapError::Rights` or `Revoked`; reuse for the `thread_create` authority check.
**Cycle hazard:** `Object::Agent` already forms the cycle `Agent -> cspace -> Cap -> Object::Agent`; teardown must drain the CSpace.

### `kernel/src/space.rs` NEW (model, CRUD) - slice B/D

**Analog:** `kernel/src/agent.rs` for shape (Arc-wrapped object, atomic counters, `charge`/`uncharge`, in-file `#[cfg(ktest)] mod test`). Contents: `Arc<VmSpace>`, owner `Arc<Agent>`, `SpinLock<Vec<Weak<Thread>>>`, dying flag. Register in `lib.rs` `pub mod` list (lines 10-18).

### `kernel/src/thread.rs` (model, event-driven) - slices B, C, E

**Statics pattern** (34-37, 91, 99): module-level `SpinLock<..., LocalIrqDisabled>` for registries; follow for the thread registry and any saved-frame bookkeeping.
**State struct** (76-85): add `signal_frames: Vec<Frame>` and FS base here, under the existing `state` lock.
**Constructor:** `new_tagged(agent, tag)` (131-135) with `NEXT_TID.fetch_add(1, Relaxed)`; keep tids never reused.
**block_until / block** (248-310): the timeout seam. Futex and `recv` pass `Some(due)` as `deadline: Option<u64>`; it already arms `DEADLINES` and removes its own entry on every exit (306-308). For teardown add a removal of a dead thread's entry (retain by `Arc::ptr_eq`, same idiom as line 307).
**FPU** (87-96): before cloning at signal delivery, flush `FPU_OWNER` if it points at this thread's FPU (Pitfall 7). `FPU_SWAP_DISABLED` is the ktest-only switch pattern.
**Test style** (`block_until_times_out`, 564-596): `Jiffies::elapsed().as_u64() + N` deadline, `assert!(DEADLINES.lock().is_empty())`. Copy for KERN-09 timeout ktests and for the "DEADLINES.len() equals baseline" teardown assertion. `xmm_survives_redirect` (598-608) with `spawn_fpu`/`join` is the template for the sigreturn GPR/XMM ktest.

### `kernel/src/timer.rs` NEW (service, event-driven) - slice C

**Analog:** `thread.rs` 34-55. Callback only wakes; heavy work in a task.
```rust
fn arm_deadline_timer() {
    if DEADLINE_TIMER.swap(true, Ordering::AcqRel) { return; }
    register_callback_on_cpu(|| {
        let now = Jiffies::elapsed().as_u64();
        DEADLINES.lock().retain(|(due, w)| {
            let pending = *due > now;
            if !pending { w.wake_up(); }
            pending
        });
    });
}
```
Timer task: `TaskOptions::new(f).spawn()` (as `lib.rs:39`), loops on `Waiter::new_pair()` woken by `Waker::wake_up`, drains an armed table, then `Endpoint::push`. Reuse the `shortcut:` comment convention for the `-smp 1` registration. Reject the timer label from ring 3 like `LABEL_CANCEL` (ipc.rs; grep it). Alternatively fold into `thread.rs` as the research allows.

### `kernel/src/agent.rs` (model, CRUD) - slice D

**Mirror `charge_pages`** (101-109) for `uncharge_pages`:
```rust
self.mem_used_pages.try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
    used.checked_add(pages).filter(|&n| n <= self.mem_limit_pages)
})
```
Use `checked_sub`/saturating for uncharge. **Carve return:** inverse of `spawn_child` 158-169, which does `cpu_carved_ticks.try_update(...)` then `charge_pages(mem_limit_pages)` and rolls back with `fetch_sub`. The new teardown does `cpu_carved_ticks.fetch_sub(quota)` and `uncharge_pages(limit)` on the parent; replace the `shortcut:` at line 143. Add a `Weak<Agent>` parent field (only the parent id is in `cred` today, line 170-173). Ktests at 188+ (`parent.mem_used_pages()` assertions, 218-223) are the template for carve-return asserts.

### `kernel/src/futex.rs` (service, request-response) - slices C, D

`TABLE: SpinLock<BTreeMap<usize, Bucket>>` (77), `wait(t, space, va, expected)` (123), waiter has a key field written under `TABLE` (63). Add: deadline argument passed through to `t.block_until`, a per-thread waiter list, and a `wake_all_for(thread)`/space sweep used by teardown. `FutexError::TimedOut` exists. Replace the "no timeouts" shortcut; the decode gate at `syscall.rs:343-344` currently asserts `ENOSYS` for a timeout and that ktest must change.

### `kernel/src/vm.rs` (service, CRUD) - slices D, E

`handle_cow_fault(space, agent, addr)` (63) charges `agent.charge_pages(1)` per copy: mirror for `mmap` (charge before map, `AgentError::Budget` maps to `ENOMEM`), and add `unmap_all` returning the page count for `uncharge_pages`. Keep the 4 KiB-only `shortcut:` (26).

### `kernel/src/user.rs` (service, request-response) - slices B, E

**`deliver`** (233-268): change `ep.send(thread, [vector, addr, rip, err])` so the message names `thread.tid`/`tag` (numeric option recommended in RESEARCH decision 6); delete the `shortcut:` (237-238). Keep the `fatal()` closure and `clear_exception_reply`/`wait_exception_reply` flow.
**Exit notification:** reuse `Exit{Code,Fault,Budget,Exception}` classification; send on thread exit through the same `exception_handler()` endpoint path.
**Redirect** (`run_ctx` ~326, `take_redirect`): replace `ctx.set_rdi(ctx.rip()); ctx.set_rip(handler)` with push-frame (`ctx.clone()`, FPU clone, FS base) then the same set.
**Image loading style to use (not B2):** `user::load_image(user_image!("fpu"))` then `user::start(&t, &img, arg)` (`lib.rs:45-57`). **[01-07 REMOVES]** `user::load`, `user::run(.., code + prog::X)`.

### `kernel/src/syscall.rs` (controller, request-response) - slices B, C, E

**Dispatch** (292-322): one match arm per syscall, `ret(x.map(|()| 0).map_err(Errno::from))`.
```rust
Syscall::Recv { ep, args } => ret(ipc::sys_recv_args(t, space, ep, args).map(|()| 0)),
```
New variants `ThreadCreate`, `Sigreturn`, `TimerArm`, `Mmap`, plus timeout fields on `FutexWait`/`Recv`. `decode` bounds-checks user pointers (`EFAULT` tests at 349-359); follow it. **Fuzz:** `KNOWN: [isize; 11]` (464) and `fuzz_syscalls_never_panic` (479): extend with any new errno together with `abi::errno` (Pitfall 12).

### `kernel/src/lib.rs` + `scripts/check.sh` (boot check, batch) - slice A/C

**Analog:** `fpu_preemption_check` (45-83) and `cpu_budget_check` (88-144): `println!("aok: <name> check {}", if ok {"passed"} else {"FAILED"})`, aggregate and `poweroff(if ok && .. { ExitCode::Success } else { ExitCode::Failure })`. Idle halt check: spawn nothing runnable, count idle-loop iterations (an `AtomicU64` bumped in the idle body) over N ticks via the `while Jiffies::elapsed().as_u64() < start + N { Task::yield_now() }` loop (59-61), expect about N. Chain it as `let idle_ok = idle_halt_check();` beside `fpu_ok`. **[01-07 REMOVES]** the `prog::SPINNER` spinners at 96-108.
**check.sh line 30:** add the new strings to the `for want in "aok: fpu preemption check passed" "aok: cpu budget check passed"` list. FAILED gate on line 33 already covers the new lines.

### `userland/abi/src/lib.rs` and `userland/libsys/src/lib.rs`

abi: add `SYS_*` numbers and `errno::ESRCH` next to existing `errno::{ENOMEM=12, ETIMEDOUT=110}`. libsys wrapper template (39-45):
```rust
pub fn recv(ep: u64, args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(SYS_RECV, [ep as usize, args as *mut IpcArgs as usize, 0, 0, 0, 0]);
    if r < 0 { Err(-r) } else { Ok(()) }
}
```
Keep `#![deny(unsafe_code)]`; asm stays in `raw.rs`/`rt.rs`/`fpu.rs` (ledger U-01..03). 01-07 adds `abi::TEST_DATA` and `libsys::test_page`; use them rather than raw addresses.

### New test bins `userland/tests/src/bin/*.rs` (test bin, request-response)

**Header and entry** (`park_srv.rs` 6-13, 48; `fpu.rs` 7-9, 45): `#![no_std] #![no_main] #![deny(unsafe_code)]`, `fn main(arg: u64) -> u64`, `libsys::entry!(main);` or `libsys::entry!(main, handler);` for a redirect/signal handler. Exit code = number of the first failing step, 0 on success (`park_srv.rs` doc and returns 1..6). Bins:
- `fork_exit`: copy `sibling.rs` (15 lines, tiny exit-code bin) as the workload for the 10,000x loop; the loop driver is a kernel ktest.
- `thread_create`, `mmap_over`, `timeout`: `park_srv.rs` step-numbered returns, `recv`/`reply` style wrappers, `errno` compare as `reply(&mut b) != Err(errno::EBADF)`.
- `sigreturn`: copy `fpu.rs`: `hold(seed, ..)`, `pattern(seed)`, `handler_xmm()`, handler returns 0 or `10 + i` on first mismatching register; extend with GPR pattern and a stack-bytes check.
Registration: `userland/tests/build.rs` (7 lines) and `user_image!("name")` in the kernel; confirm both after 01-07.

## Shared Patterns

### Registry / static state
**Source:** `thread.rs` 34-37, 91, 99. `SpinLock<_, LocalIrqDisabled>` for anything touched from the timer callback; plain `SpinLock` for task-only state (futex `TABLE`). **Apply to:** thread registry, armed-timer table, space registry.

### `// shortcut:` comments
**Source:** `sched.rs:62`, `thread.rs:39,90`, `futex.rs:76`, `agent.rs:123,143`. Every deliberate gap gets one; delete the ones phase 2 closes (`agent.rs:143`, `user.rs:237`, `syscall.rs:343` comment).

### Wake-only IRQ callbacks
**Source:** `thread.rs` 41-55. **Apply to:** timer-to-endpoint, deadline expiry, teardown wakes.

### Capability gate
**Source:** `cap.rs` 105-111 plus `user.rs` 251-254 (`cap.resolve(Rights::SEND).ok()` in a let-else). **Apply to:** `thread_create` (Space cap, CREATE), signal (Thread cap, SIGNAL).

### Leak-counter assertions in ktests
**Source:** `thread.rs:575,584` (`DEADLINES.lock().is_empty()`), `agent.rs:218-223` (`mem_used_pages()` before/after). **Apply to:** `teardown_loop`: snapshot counters (`mem_used_pages`, CSpace live slots, `futex::TABLE.len()`, `DEADLINES.len()`, registries, `Arc::strong_count`), run N cycles, assert equal.

### ktest conventions
`#[cfg(ktest)] mod test { use ostd::prelude::*; ... #[ktest] fn name() }` in the same file as the code. Filters in RESEARCH Validation (`cargo osdk test thread_create`) map to these function names, so name ktests to match.

### unsafe policy
`#![deny(unsafe_code)]` crate-wide; ledger table is empty. No analog code in the kernel uses `unsafe`; keep it that way (RESEARCH finds none needed).

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| Teardown routine (`space.rs` or `thread.rs`) | service | batch | No unmap, drain, or reap code exists; ordered steps come from RESEARCH Pattern 2. Closest idioms: `retain` by `Arc::ptr_eq` (`thread.rs:307`), `CSpace` slot ops in `cap.rs` |
| Signal frame push/pop | model | event-driven | Only the B2 redirect (`user.rs` ~326) exists; use RESEARCH Pattern 6 API list |
| Timer task / timer-to-endpoint | service | event-driven | Only the wake-only callback exists; no kernel service-task loop yet. Nearest: the `lib.rs:39` `TaskOptions::new(..).spawn()` and `thread.rs` `Waiter::new_pair` |
| AGNT-05 ledger/doc | doc | n/a | No server code yet (procmgr/vfs are later phases); `.planning/PROJECT.md` and the `docs/architecture/` ADR format are the only guides |

## Metadata

**Analog search scope:** `kernel/src`, `userland/{abi,libsys,tests}`, `scripts/`.
**Files read:** sched.rs (full), cap.rs 1-130, thread.rs 30-99/245-315/560-609, lib.rs 30-145, agent.rs 95-175, syscall.rs 290-360, user.rs 232-277, libsys/lib.rs, bins fpu.rs/park_srv.rs, check.sh. Not read in full: ipc.rs, futex.rs bodies, vm.rs bodies, abi, libsys rt/fpu, other bins; line cites for those come from grep or RESEARCH.
**Pattern extraction date:** 2026-10-25
