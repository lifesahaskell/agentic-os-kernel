# Phase 2: Process objects, teardown and timers - Pattern Map

**Mapped:** 2026-10-26 (rewrite; supersedes the 2026-10-25 map, which predated D-05, D-06, D-07, D-08)
**Files analyzed:** 22 (11 kernel modified, 3 kernel new, 2 userland libs, new test bins, 2 non-code, 1 manifest)
**Analogs found:** 21 / 22 (4 are role-match only; see No Analog Found)

All analog paths were checked with `git ls-files` (kernel/src/*.rs, userland/**, scripts/check.sh are tracked).
Line numbers are HEAD as of this map (01-07 has landed; the B2 `prog`/`PROGRAM` analogs are gone and must not be searched for). Lines that moved since the old map are the ones given here.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `kernel/src/sched.rs` (idle, pick fallback, enqueue hint) | scheduler | event-driven | itself (`RunQueue`, `BudgetScheduler`) | exact |
| `kernel/src/cap.rs` (`Object::{Space,Thread}`, Rights, `CSpace::{drain,close,live}`) | model | CRUD | itself | exact |
| `kernel/src/space.rs` NEW | model/registry | CRUD | `kernel/src/agent.rs` | role-match |
| `kernel/src/thread.rs` (registry, frames, FS base, interrupt) | model | event-driven | itself | exact |
| `kernel/src/timer.rs` NEW | service | event-driven | `thread.rs` `DEADLINES` + `arm_deadline_timer` | role-match |
| `kernel/src/agent.rs` (`Payer`, `uncharge_pages`, `Weak` parent, `Life`, retire) | model | CRUD | itself | exact |
| `kernel/src/vm.rs` (fallible `cow_clone`, `CloneError`, `&dyn Payer`) | service | CRUD | itself (`cow_clone` 27-59, `handle_cow_fault` 68-110) | exact |
| `kernel/src/futex.rs` (timeout, waiter sweep, errno mapping at :88) | service | request-response | itself | exact |
| `kernel/src/user.rs` (`deliver` tid/tag, exit note, frames; `cow_child` ktest helper) | service | request-response | itself | exact |
| `kernel/src/ipc.rs` (recv timeout, staging charge at :510, cow_clone callers :874/:972) | service | request-response | itself | exact |
| `kernel/src/syscall.rs` (new calls, errno map) | controller | request-response | itself, `dispatch` 293 | exact |
| `kernel/src/probe.rs` NEW, `#[cfg(ktest)]` | test support | batch | none in tree; `thread.rs` ktest leak asserts + RESEARCH | no analog |
| `kernel/Cargo.toml` (+ `Cargo.lock`) | config | n/a | itself (`[dependencies.ostd]` 9-10) | exact |
| `kernel/src/lib.rs` (pub mod list, idle boot check) | boot check | batch | `fpu_preemption_check` 46, `cpu_budget_check` 89 | exact |
| `scripts/check.sh` | config | batch | itself, `for want in` line 30 | exact |
| `userland/abi/src/lib.rs`, `userland/libsys/src/lib.rs` | config/utility | transform / request-response | themselves | exact |
| `userland/tests/src/bin/*.rs` new (fork_exit, thread_create, sigreturn, mmap, timeout) | test bin | request-response | `park_srv.rs`, `fpu.rs`, `sibling.rs` | role-match |
| `docs/architecture` AGNT-05 record, `.planning/PROJECT.md` wording | doc | n/a | none | none |

## Pattern Assignments

### `kernel/src/vm.rs`: fallible `cow_clone` (service, CRUD) - 02-06 (D-05, D-07)

**Analog:** itself. The current function is the thing being rewritten; keep its cursor walk, change its failure model.

**Current signature and unwrap sites** (vm.rs:24-59). Lines to replace: 27 (signature), 30, 31, 34, 46, 48 (`unwrap`), 36 (`panic!`), 38 (`assert_eq!`). The walk shape to keep:
```rust
pub fn cow_clone(parent: &VmSpace, range: Range<Vaddr>) -> VmSpace {
    let child = VmSpace::new();
    let guard = disable_preempt();
    let mut pc = parent.cursor_mut(&guard, &range).unwrap();
    let mut cc = child.cursor_mut(&guard, &range).unwrap();

    while let Some(va) = pc.find_next(range.end - pc.virt_addr()) {
        let (r, item) = pc.query().unwrap();
        let Some(VmQueriedItem::MappedRam { frame, mut prop }) = item else {
            panic!("cow_clone: MMIO in a user range at {va:#x}");
        };
        assert_eq!(r.len(), PAGE_SIZE, "cow_clone: huge page at {va:#x}");
        let frame: UFrame = (*frame).clone();
        // The source cursor cannot jump to `range.end`, so the last page ends the walk.
        let last = va + PAGE_SIZE >= range.end;
        if prop.flags.contains(PageFlags::W) {
            prop.flags = (prop.flags - PageFlags::W) | COW;
            pc.protect_next(PAGE_SIZE, |f, _| *f = (*f - PageFlags::W) | COW);
        } else if !last {
            pc.jump(va + PAGE_SIZE).unwrap();
        }
        cc.jump(va).unwrap();
        cc.map(frame, prop);
        if last || pc.virt_addr() >= range.end { break; }
    }
    pc.flusher().issue_tlb_flush(TlbFlushOp::for_range(range));
    pc.flusher().dispatch_tlb_flush();
    drop((pc, cc));
    child
}
```
**New shape (D-05, D-07):** `pub fn cow_clone(parent: &VmSpace, range: Range<Vaddr>, payer: &dyn Payer) -> Result<VmSpace, CloneError>`; `CloneError { Range, Unsupported(Vaddr), Budget }` plus `From<AgentError>`-style mapping. Replace each `.unwrap()` with `.map_err(|_| CloneError::Range)?`, the `panic!` and `assert_eq!` with `return Err(CloneError::Unsupported(va))`. Charge node frames through `payer.charge(n)` before `cc.map`: root first, then 3 for the first page, then 1 per newly crossed 2 MiB / 1 GiB / 512 GiB window (D-07). Track `charged: usize` as you go.

**Unwind rule that is easy to get wrong.** On `Err` after some pages are mapped, the parent has already been write-protected by `protect_next`; the parent keeps COW-sole pages (a later write fault takes the `sole` fast path in `handle_cow_fault` 85-96). So the error path must still issue the TLB flush (lines 55-56) before returning, then `payer.uncharge(charged)`, then drop `(pc, cc)` and `child`. Do not early-`return` with `?` in the middle of the loop while holding cursors without routing through one cleanup block; build the loop body as an inner closure or labelled block returning `Result`, run the flush and drop unconditionally, then uncharge on `Err`.

**Fault-path error style to copy** (`handle_cow_fault` vm.rs:68-110): returns `Result<bool, AgentError>`; defence-in-depth `let Ok(mut c) = space.cursor_mut(...) else { return Ok(false); };` (78-80) is the "range is outside user space is a declined fault, not a panic" idiom to reuse for `CloneError::Range`. The allocate-then-charge ordering (99-104) is only safe for one frame; the comment there says so.
```rust
let new = FrameAllocOptions::new()
    .alloc_frame()
    .map_err(|_| AgentError::NoMemory)?;
agent.charge_pages(1)?;
```
Per D-04, this charge must go through the `Payer` (the Space) so the ledger records it. The two remaining `unwrap()`s at 81 (`c.query()`) and 107 (`c.jump(page)`) are in D-05's scope ("replaces the unwrap() calls"); change them to `return Ok(false)`.

**`cow_clone` callers to update (all must take a payer and handle `Result`):**
- `kernel/src/ipc.rs:874` (ktest `siblings_are_distinguished_by_tid_and_tag`, closure `clone_of`):
```rust
let clone_of = |img: &Image| Image {
    space: Arc::new(cow_clone(&img.space, abi::IMAGE_BASE..abi::USER_STACK_TOP)),
    entry: img.entry, handler: img.handler, text: img.text.clone(),
};
```
- `kernel/src/ipc.rs:972` (ktest helper `bulk_round_trip`, same `Image { space: Arc::new(cow_clone(..)), entry, handler, text }` shape). Import at ipc.rs:696.
- `kernel/src/user.rs:385` (`cow_child`, ktest helper) and `user.rs:645` (`let _clone = cow_clone(...)`, a drop-only use). Import at user.rs:362.
```rust
fn cow_child(parent: &Image) -> Image {
    let child = Arc::new(cow_clone(&parent.space, abi::IMAGE_BASE..DATA + PAGE_SIZE));
    map_stack(&child).unwrap();
    Image { space: child, ..parent.clone() }
}
```
In ktests, `.unwrap()` the new `Result` and pass a root `Agent` (or a test `Payer` with a large budget) so the existing tests stay as written (D-07: `cow_clone_isolates_writes_and_charges_the_writer` at user.rs:408 stays). `Agent` must `impl Payer`.

**New ktest for the partial-clone failure** (D-05): copy the style of `cow_fault_over_budget_stops_the_thread` (user.rs:436-443): build a parent with `new_image(user_image!("cow_writer"), PageFlags::RW)`, a tiny-budget `Agent::new_root(1, N)` as payer, assert `cow_clone(..) == Err(CloneError::Budget)`, assert `payer.mem_used_pages() == 0` (the unwind refunded everything) and that the parent page is still writable after one fault. Also add range tests (unaligned range, range above `USER_TOP` -> `CloneError::Range`). `Unsupported` is inspection-only (RESEARCH table: not constructible in a ktest).

**ktest fixture to reuse** for vm.rs tests: `space_with(pages, fill, flags)` and `byte(space, at)` (vm.rs:194-215), DATA = 0x50_0000.

---

### `kernel/src/futex.rs`: error mapping, timeout, sweep (service, request-response) - 02-07 (D-05, D-09)

**Analog:** itself.

**The line to fix** (futex.rs:88), and the type it feeds (26-50):
```rust
handle_cow_fault(space, t.agent(), va).map_err(|_| FutexError::Budget)?;
```
```rust
pub enum FutexError { Again, Fault, Inval, Interrupted, TimedOut,
    /// Breaking copy-on-write exceeded the agent's memory budget.
    Budget }
impl FutexError { pub fn errno(&self) -> isize { match self { Self::Again => 11, ... Self::Budget => 12 } } }
```
Add `FutexError::NoMemory` (errno 12, D-09) and map by variant, mirroring `From<AgentError> for Errno` at syscall.rs:72-80 (where `Budget` and `NoMemory` both go to `ENOMEM` today):
```rust
impl From<AgentError> for Errno {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Budget => Errno::ENOMEM,
            AgentError::NoMemory => Errno::ENOMEM,
            AgentError::Cap(c) => c.into(),
        }
    }
}
```
So: `.map_err(|e| match e { AgentError::Budget => FutexError::Budget, AgentError::NoMemory => FutexError::NoMemory, AgentError::Cap(_) => FutexError::Fault })`. Note `handle_cow_fault` returns `Ok(false)` for a non-COW page and the result at line 88 is discarded by `?` (the bool is ignored); leave that. Existing `From<BlockError> for FutexError` (52-59) is the pattern for any new conversion. ktests: add one asserting a zero-budget agent waiting on a COW page gets `ENOMEM` (errno 12) and that `NoMemory` maps to the same number.

**Timeout and sweep:** `TABLE: SpinLock<BTreeMap<usize, Bucket>>` (futex.rs:77), `Waiter { woken, key, wq }` (61-68), `wait(t, space, va, expected)` (123). `pop`/`mark_woken`/`wake_all` (98-120) are the idioms for a per-thread sweep used by teardown. The "no timeouts" shortcut to delete is `syscall.rs:238`.

---

### `kernel/src/agent.rs`: retire path (model, CRUD) - 02-06 (D-01, D-02, D-08)

**Analog:** itself.

**Struct and builder to extend** (agent.rs:44-92). Fields today: `cred`, `cspace: SpinLock<CSpace>`, `mem_limit_pages`, `mem_used_pages: AtomicUsize`, `cpu_quota_ticks`, `cpu_carved_ticks: AtomicU64`, `cpu: SpinLock<CpuUse, LocalIrqDisabled>`. Add `parent: Option<Weak<Agent>>`, `life: SpinLock<Life>` with `Life { state, spaces, children }`, and a `live_agents` static counter (`+1` in `build`, `-1` in `impl Drop`). Pattern for a global counter: `static NEXT_ID: AtomicU64 = AtomicU64::new(1);` (agent.rs:56).

**`charge_pages`, copy for `uncharge_pages`** (agent.rs:102-111):
```rust
pub fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
    self.mem_used_pages
        .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(pages).filter(|&n| n <= self.mem_limit_pages)
        })
        .map(drop)
        .map_err(|_| AgentError::Budget)
}
```
`uncharge_pages`: same `try_update` with `used.saturating_sub(pages)`, plus `debug_assert!(used >= pages)` before it (RESEARCH: never wrap in release).

**`spawn_child` is the carve and its exact inverse** (agent.rs:143-180). It is `&self`; D-01/D-08 need `self: &Arc<Self>` to store the `Weak`. Every call site already holds an `Arc<Agent>`, so it is source-compatible. The carve:
```rust
self.cpu_carved_ticks
    .try_update(Ordering::AcqRel, Ordering::Acquire, |carved| {
        carved.checked_add(cpu_quota_ticks).filter(|&n| n <= self.cpu_quota_ticks)
    })
    .map_err(|_| AgentError::Budget)?;
if let Err(e) = self.charge_pages(mem_limit_pages) {
    self.cpu_carved_ticks.fetch_sub(cpu_quota_ticks, Ordering::AcqRel);
    return Err(e);
}
```
Retire's return is `parent.uncharge_pages(self.mem_limit_pages)` and `parent.cpu_carved_ticks.fetch_sub(self.cpu_quota_ticks, ..)`, only when `children == 0` (D-08), cascading to the parent when it is `Retiring` and reaches zero. Delete the `shortcut:` at agent.rs:145. The rollback-on-failure shape above is the idiom for "increment parent's `children` under its `Life` lock, undo if the carve fails".

**CSpace copy-before-drop hazard** (drain): the parent's CSpace guard in `spawn_child` is scoped with a bare block (agent.rs:153-159) so the lock drops before the budget steps. Copy that scoping in `retire`: take `cspace.lock().drain()` inside a block, release the lock, and only then drop the returned `Vec<Cap>` (dropping a `Cap` may drop an `Arc<Space>` whose `Drop` re-enters `leave_space`).

**ktests to copy** (agent.rs:190-256): `child_gets_attenuated_subset_and_carved_budget` is the template for carve-return asserts (`parent.mem_used_pages()` after retire returns to its pre-spawn value; `parent.spawn_child(&[], 7, 0)` then succeeds). `child_cannot_regrant_without_grant_right` is the template for a retired-agent rejection test. Ktests here use `Agent::new_root(1, 10)` and `cs.insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))`.

---

### `kernel/src/cap.rs`: `CSpace::{drain, close, live}` and new Object variants (model, CRUD) - 02-04, 02-06

**Analog:** itself.

**`CSpace` as it is** (cap.rs:137-204): `slots: Vec<Option<Cap>>`, `free: Vec<usize>`, `MAX_SLOTS = 65_536`. `insert` (162-172) pops `free` else pushes; `insert_all` (183-188) is all-or-nothing through `room()` (191-193); `remove` (195-203). Live slots = `slots.len() - free.len()`.

**Add:**
```rust
pub fn live(&self) -> usize { self.slots.len() - self.free.len() }
pub fn drain(&mut self) -> Vec<Cap> {      // caller drops the Vec AFTER releasing the lock
    self.free.clear();
    core::mem::take(&mut self.slots).into_iter().flatten().collect()
}
pub fn close(&mut self) { self.closed = true; }   // new field; set after drain
```
Gate `insert` and `insert_all` (and `room()`, which must return 0 when closed so the all-or-nothing check in `insert_all` fails first) on `closed`, returning `CapError::Revoked` (already maps to `EBADF`; `CapError` is at cap.rs:45-51 and the `From<CapError> for Errno` match is at syscall.rs:~60-67) or a new variant with the match extended. Remember `Default for CSpace` (143-147) and `CSpace::new()` (153-158) both need the new field.

**Rights** (13-37): `ALL = Self(7)` and `from_bits` masks with `ALL`, so adding CREATE=8 and SIGNAL=16 requires updating `ALL` and `from_bits` together or the bit is silently dropped.
**Object** (39-43): add `Space(Arc<Space>)`, `Thread(Weak<Thread>)` (OD-14: Thread caps are `Weak`; the old map said `Arc`, that was wrong). Imports at line 8.
**Resolve** (105-111): `cap.resolve(Rights::X)?` returns `CapError::Rights` or `Revoked`; reuse for the `thread_create` gate.
**Cycle hazard:** `Agent -> cspace -> Cap -> Object::Agent` is a strong cycle; `drain` is what breaks it.

---

### `kernel/src/probe.rs` NEW (test support, batch) - D-06, D-03

**No in-tree analog.** `probe` does not exist (grep for `osdk_`/`probe` across kernel/src and Cargo.toml returns nothing). `kernel/Cargo.lock` has no entry for either crate. Build it from three existing idioms:

**1. Dependency declaration style** (`kernel/Cargo.toml:6-13`):
```toml
[dependencies]
abi = { package = "aok-abi", path = "../userland/abi" }

[dependencies.ostd]
version = "0.19.0"
```
Add the two crates the same way, as plain registry dependencies at `0.19.0`: `osdk-frame-allocator = "0.19.0"` and `osdk-heap-allocator = "0.19.0"`. A `checkpoint:human-verify` precedes this edit (D-06, both are SUS on downloads only) and the Wave 0 spike runs first. `kernel/Cargo.toml` line 12-13 is `[workspace] exclude = ["target/osdk/base", "target/osdk/test-base"]`; the base crate under `kernel/target/osdk/` already depends on both crates, so the spike's job is to prove they unify to the same statics.

**2. Calls (RESEARCH, verified in the crate sources, not yet compiled here):**
```rust
fn free() -> usize {
    osdk_frame_allocator::load_total_free_size() + osdk_heap_allocator::load_total_slab_size()
}
```
Gate the module: `#[cfg(ktest)] pub mod probe;` in `lib.rs` next to the `pub mod` list (lib.rs:10-18), so production never links the calls.

**3. Spike ktest shape**, copied from the in-file ktest convention (`#[cfg(ktest)] mod test { use ostd::prelude::*; ... #[ktest] fn name() }`, e.g. vm.rs:180-189): allocate one `FrameAllocOptions::new().alloc_frame().unwrap()`, assert `free()` fell by `PAGE_SIZE`, drop it, `settle()`, assert it came back. Fallback if it fails (D-06): counters only, probe-by-exhaustion, recorded in the SUMMARY.

**Settle loop idiom** (RESEARCH, to implement): a bare `Task::yield_now()` does nothing with no peer runnable, so each round `TaskOptions::new(|| {}).spawn()` a no-op and yield until a flag it sets is observed. Spawn style to copy: `TaskOptions::new(move || { ... }).spawn().unwrap();` (ipc.rs:881-889). The `while Jiffies::elapsed().as_u64() < start + N { Task::yield_now() }` loop at `lib.rs:59-61` is the yield loop.

**Counter-snapshot analog:** `thread.rs:575` `assert!(DEADLINES.lock().is_empty(), "the deadline entry is removed")` and `agent.rs:220-225` (`parent.mem_used_pages()` before/after). `probe::counters()` should return a struct of readers (live threads, live and dropped Spaces, live agents, agent pages used, CSpace live, `futex::TABLE.len()` (futex.rs:77), `DEADLINES.len()` (thread.rs:36), armed timers, parked replies, `FPU_OWNER` (thread.rs:91)) and the loop asserts equality; these statics are private today, so each gets a `pub(crate)` reader.

---

### `kernel/src/sched.rs` (scheduler, event-driven) - 02-01

**Analog:** itself.

**Task-kind dispatch** (sched.rs:37-39, 54-56): kernel tasks are those without `Arc<Thread>` data. Idle needs a distinct marker; add a second `downcast_ref` beside `thread_of`.
```rust
fn thread_of(task: &Task) -> Option<&Arc<Thread>> { task.data().downcast_ref() }
fn eligible(task: &Task, now: u64) -> bool {
    thread_of(task).is_none_or(|t| t.agent().has_cpu(now))
}
```
**`pick`** (96-103): the `?` at line 97 returns `None` when nothing is eligible; idle fallback replaces it.
```rust
fn pick(&mut self, now: u64) -> Option<&Arc<Task>> {
    let i = self.queue.iter().position(|t| eligible(t, now))?;
    let next = self.queue.remove(i).unwrap();
    if let Some(prev) = self.current.replace(next) { self.queue.push_back(prev); }
    self.current.as_ref()
}
```
**`enqueue`** (127-138): keep the `set_if_is_none` handshake verbatim; change the final `None` to `Some(CpuId::bsp())` when current is idle. **`dequeue_current`** (119-123) already clears the cpu; keep for idle.
**Test style:** `mod test` at sched.rs:150; build `RunQueue::default()`, push `task(...)` helpers, assert `rq.pick(now)` / `rq.update(flags, now)` with `is()`; reuse `out_of_budget_agent_is_skipped_until_the_next_period` for truth-table ktests. `install()` cannot run in a ktest, so idle halt is a boot check only.

---

### `kernel/src/thread.rs` (model, event-driven) - 02-04, 02-05, 02-08

**Analog:** itself.

**Deadlines and the timer callback** (thread.rs:34-55, quoted in the timer.rs section below). `block_until` (248) arms `DEADLINES` (280) and removes its own entry on every exit (307: `DEADLINES.lock().retain(|(_, w)| !Arc::ptr_eq(w, &waker))`); copy that `retain`-by-`Arc::ptr_eq` idiom to drop a dead thread's entry in teardown.
**Statics** (36, 91, 99): `SpinLock<_, LocalIrqDisabled>` for anything touched from the timer callback; `NEXT_TID: AtomicU64` (99), tids never reused. **Constructor:** `new_tagged(agent, tag)` (131).
**FPU:** before cloning at signal delivery, flush `FPU_OWNER` if it points at this thread's FPU (declared 91, used 325). `shortcut:` at 90.
**Tests:** `block_until_times_out` (565-584) is the KERN-09 timeout template (`Jiffies::elapsed().as_u64() + N`, `assert!(DEADLINES.lock().is_empty())`); `xmm_survives_redirect` (about 598-608) is the sigreturn template.

---

### `kernel/src/timer.rs` NEW (service, event-driven) - 02-02

**Analog:** `thread.rs` 34-55; the callback only wakes, heavy work runs in a task.
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
Timer task: `TaskOptions::new(f).spawn()`, loop on `Waiter::new_pair()` woken by `Waker::wake_up`, drain an armed table, then `Endpoint::push_kernel`. Keep the `shortcut:` comment convention for the `-smp 1` registration (thread.rs:39). Reject `LABEL_TIMER` from ring 3 the way `LABEL_CANCEL` is rejected (grep `LABEL_CANCEL` in ipc.rs).

---

### `kernel/src/ipc.rs`: staging charge (service, request-response) - 02-07 (D-04)

**Analog:** itself, `read_bytes` (ipc.rs:509-518):
```rust
// shortcut: staging memory is not charged to the sender's agent, so a client can pin
// MAX_IPC_BYTES * QUEUE_LEN of kernel heap per endpoint; Phase 2 resource accounting charges it.
fn read_bytes(space: &VmSpace, src: UserSlice) -> Result<Vec<u8>, Errno> {
    let mut buf = vec![0u8; src.len()];
    if !buf.is_empty() { copy_from_user(space, &mut buf, src)?; }
    Ok(buf)
}
```
Charge `ceil(len / PAGE_SIZE)` through the sender's `Payer` before the `vec!`, refund on `copy_from_user` failure, uncharge when the message is consumed or dropped. Delete the `shortcut:` (510-511). `write_bytes` (522-528) already shows the "charged to `t`'s agent" idiom via `copy_to_user(space, t.agent(), ...)`.
**Capability transfer is a copy** (the fact D-01 rests on): `fn derive_caps(t, list)` at about ipc.rs:455-458, `cs.get(s)?.derive(m)`. `install_caps` is at about ipc.rs:471-480; this is where a closed CSpace must be rejected.
**Spawn-child call sites** (change to `self: &Arc<Self>` stays source-compatible): ipc.rs:708 and 866; also cap.rs:337 and sched.rs:185.

---

### `kernel/src/user.rs` (service, request-response) - 02-05, 02-08

**`deliver`** (user.rs:211): change the message to name `tid` and `tag`; delete the `shortcut:` at 209-210. `run_ctx` at 242, `start` at 152, `load_image` at 63. The image-loading line that pays frames, `FrameAllocOptions::new().alloc_frame().map_err(|_| LoadError::NoMemory)?`, is within `load_image` (about 92-94) and is where `Space::from_image` takes over charging. The COW call at `user.rs:273` (`Some(handle_cow_fault(space, thread.agent(), pf.addr))`) and the `shortcut:` at 279 ("Exit has no out-of-memory cause", D-09: record, do not split) are the exit-classification site.
Image style to use in new ktests: `user::load_image(user_image!("fpu"))` then `start(&t, &img, arg)` or the testkit `spawn_image` / `join` / `wait_blocked` (user.rs:361).

---

### `kernel/src/syscall.rs`, `lib.rs`, `scripts/check.sh`

**Dispatch** (syscall.rs:293): one match arm per syscall, `ret(x.map(|()| 0).map_err(Errno::from))`. `decode` bounds-checks user pointers. **Fuzz:** `const KNOWN: [isize; 11]` (syscall.rs:465) and `fuzz_syscalls_never_panic` (480): extend with any new errno together with `abi::errno`.
**Errno maps:** `From<FutexError> for Errno` (syscall.rs:~66-70) is `Errno(e.errno())`; `From<AgentError> for Errno` (72-80) quoted above.
**Boot check:** `fpu_preemption_check` (lib.rs:46) and `cpu_budget_check` (89): `println!("aok: <name> check {}", ...)`, aggregate, `poweroff(...)`. Idle-halt check follows that shape.
**check.sh** line 30: `for want in "aok: fpu preemption check passed" "aok: cpu budget check passed"; do`; append new strings; the `FAILED` gate at line 33 already covers them.
**lib.rs `pub mod` list** (10-18): add `space`, `timer`, and `#[cfg(ktest)] pub mod probe;`.

---

### `userland/abi`, `userland/libsys`, new test bins

abi: add `SYS_*` numbers and `errno::ESRCH` (3), `ENODEV` (19), `ENOTSUP` (95) beside `ENOMEM=12`, `ETIMEDOUT=110`. libsys wrapper template:
```rust
pub fn recv(ep: u64, args: &mut IpcArgs) -> Result<(), isize> {
    let r = raw::syscall6(SYS_RECV, [ep as usize, args as *mut IpcArgs as usize, 0, 0, 0, 0]);
    if r < 0 { Err(-r) } else { Ok(()) }
}
```
Keep `#![deny(unsafe_code)]`; asm stays in `raw.rs`/`rt.rs`/`fpu.rs` (ledger U-01..U-06; the Kernel table is empty and must stay so, D-03/D-06).
Bins: header `#![no_std] #![no_main] #![deny(unsafe_code)]`, `fn main(arg: u64) -> u64`, `libsys::entry!(main);` (or `entry!(main, handler)`); exit code = number of the first failing step, 0 on success (`park_srv.rs`). `fork_exit` copies `sibling.rs` (tiny exit-code bin) as the 10,000x workload; `sigreturn` copies `fpu.rs` (`hold`, `pattern`, handler returns `10 + i` on first mismatch). Registration: `userland/tests/build.rs` and `user_image!("name")`.

## Shared Patterns

### Payer (charge/uncharge through one trait)
**Source:** `agent.rs` `charge_pages` (102-111) and the new `uncharge_pages`. **Apply to:** `cow_clone` (`&dyn Payer`), `handle_cow_fault`, `read_bytes` staging, `mmap`, Space image load. Space impls it so its ledger records every charge and teardown does `owner.uncharge_pages(ledger.swap(0))`.

### Lock released before drop
**Source:** `agent.rs:153-159` (bare-block scoped lock). **Apply to:** `retire`, `Space::teardown`, any `CSpace::drain` (drained `Cap`s drop after the guard is gone).

### Registry / static state
**Source:** `thread.rs` 36-37, 91, 99. `SpinLock<_, LocalIrqDisabled>` for state touched from the timer callback; plain `SpinLock` for task-only state (futex `TABLE`). **Apply to:** thread registry, armed-timer table, space registry, live counters.

### `// shortcut:` comments
**Source:** `sched.rs:62`, `thread.rs:39,90`, `futex.rs:76`, `agent.rs:125,145`, `ipc.rs:141,293,510`, `user.rs:209,279`, `vm.rs:26`, `syscall.rs:238`. Delete the ones this phase closes: `agent.rs:145`, `ipc.rs:510`, `user.rs:209`, `syscall.rs:238`. `vm.rs:26` changes from "asserted" to "returns `CloneError::Unsupported`".

### Wake-only IRQ callbacks
**Source:** `thread.rs` 41-55. **Apply to:** timer-to-endpoint, deadline expiry, teardown wakes.

### Capability gate
**Source:** `cap.rs` 105-111, plus `user.rs` (`cap.resolve(Rights::SEND).ok()` in a let-else). **Apply to:** `thread_create` (Space cap, CREATE), signal (Thread cap, SIGNAL).

### Leak-counter assertions in ktests
**Source:** `thread.rs:575,584` and `agent.rs:220-225`. **Apply to:** the 10,000x loop: snapshot counters, run N cycles, `settle()`, assert equal, then the probe gate (`before - after <= footprint`).

### ktest conventions
`#[cfg(ktest)] mod test { use ostd::prelude::*; ... #[ktest] fn name() }` in the same file as the code. Name ktests to match the filters in 02-VALIDATION.

### Unsafe policy
`#![deny(unsafe_code)]` crate-wide; the Kernel ledger table is empty and stays so. The probe uses only safe `pub fn`s.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `kernel/src/probe.rs` | test support | batch | No free-frame, `osdk_*`, or settle-loop code exists; sections above assemble it from ktest conventions and RESEARCH. Wave 0 spike is the first task |
| Teardown routine (`space.rs`) | service | batch | No unmap, drain, or reap code exists. Idioms: `retain` by `Arc::ptr_eq` (`thread.rs:307`), scoped lock (`agent.rs:153-159`), `pop`/`wake_all` (`futex.rs:98-120`) |
| Signal frame push/pop | model | event-driven | Only the redirect in `user.rs` `run_ctx` exists; use RESEARCH Pattern 6 |
| AGNT-05 ledger/doc | doc | n/a | No server code yet; `.planning/PROJECT.md` and the `docs/architecture/` ADR format are the only guides |

## Metadata

**Analog search scope:** `kernel/src`, `kernel/Cargo.toml`, `userland/{abi,libsys,tests}`, `scripts/`, `docs/architecture/unsafe-ledger.md`.
**Files read this pass:** vm.rs 1-215, agent.rs (whole), futex.rs 1-130, cap.rs 1-210, ipc.rs 506-528 and 860-990, user.rs 355-455, sched.rs 94-138, thread.rs 34-55, syscall.rs 55-90, check.sh 25-40, unsafe-ledger.md head. Reused from the 2026-10-25 map without re-reading (line numbers re-checked by grep only): thread.rs 248-310 and 560-609, lib.rs 46-144, syscall.rs 293-600 bodies, userland bins and libsys.
**Not read:** the rest of RESEARCH.md past line 317 (Pitfalls 13-22, Open decisions), ipc.rs outside the ranges above.
**Pattern extraction date:** 2026-10-26
