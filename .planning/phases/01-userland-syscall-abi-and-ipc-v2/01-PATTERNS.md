# Phase 1: Userland, syscall ABI and IPC v2 - Pattern Map

**Mapped:** 2026-10-24
**Files analyzed:** 22 (new or modified)
**Analogs found:** 15 / 22 (all analog paths verified git-tracked: kernel/src/*, kernel/osdk.sh, tools/wikigraph.py)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `kernel/src/syscall.rs` (new) | decode layer | request-response | `kernel/src/user.rs` `run` match (L197-205) + `sys_futex` arg decode | role-match |
| `kernel/src/thread.rs` (mod: `block_until`, tid/tag, `FpuContext`, `pre_user_run`) | model/hook | event-driven | itself: `block_on` (L125-147), `UserModeHooks` impl (L149-) | exact |
| `kernel/src/ipc.rs` (mod: call/reply/reply_recv/Cancel, tid/tag) | service | request-response | itself: `Endpoint`, `sys_send`/`sys_recv` | exact |
| `kernel/src/vm.rs` (mod: `copy_to_user`/`copy_from_user`) | utility | file-I/O (memory copy) | `vm::handle_cow_fault` (L50-85), `futex::lookup` | role-match |
| `kernel/src/cap.rs` (mod: growable CSpace) | model | CRUD | itself: `CSpace` (L124-) | exact |
| `kernel/src/futex.rs` (mod: migrate onto `block_until`) | service | event-driven | `Thread::block_on` | exact |
| `kernel/src/user.rs` (mod: loader w/ bss+stack, dispatch via `syscall::decode`) | service | request-response | itself: `load` (L76-93), `run` (L185-235) | exact |
| `kernel/src/fpu.rs` (only if safe route fails) | utility | transform | none | none |
| `kernel/src/lib.rs` (mod: `pub mod syscall;`) | config | n/a | itself L9-18 | exact |
| ktests in `thread.rs`/`ipc.rs`/`user.rs`/`syscall.rs` | test | request-response | `user.rs` `mod test` (L238-): `spawn_user`, `join`, `wait_blocked`, `handler_pair` | exact |
| `kernel/osdk.sh` (mod: mount repo root) | config | n/a | itself | exact |
| `kernel/Cargo.toml` (mod: `abi` path dep) | config | n/a | itself | exact |
| `userland/abi/` (new crate) | model | transform | none (Rust `no_std` shared consts); `ipc::Message` for layout | partial |
| `userland/libsys/` (new crate) | utility | request-response | RESEARCH tracer asm | none in repo |
| `userland/tests/*` (bins: tracer, fpu_a/b, ipc_*, fork_cow) | test | request-response | B2 hand-assembled `user.rs::prog` (L40-60) | partial |
| `userland/{x86_64-aok-user.json,link.ld,build.sh,.cargo/config.toml}` | config | batch | RESEARCH Pattern 1 / Code Examples | none in repo |
| `tools/unsafe_check.py` (new) | utility | batch | `tools/wikigraph.py` (docstring+`--self-test`, `python3 -I`, exit 1 on failure) | role-match |
| `scripts/check.sh` (new) | config | batch | `kernel/osdk.sh` (`set -eu` POSIX sh) | role-match |
| `docs/architecture/unsafe-ledger.md` (new) | doc | n/a | RESEARCH Pattern 9 format | none |
| `.github/workflows/*` (new) | config | batch | none | none |

## Pattern Assignments

### `kernel/src/thread.rs` (block_until, FPU, tid/tag)

**Analog:** itself. Replace `block_on` with the single `block_until`; keep this shape.

**Imports** (L7-15):
```rust
use alloc::sync::Arc;
use ostd::{
    mm::{Vaddr, VmSpace},
    sync::{LocalIrqDisabled, SpinLock, WaitQueue, Waiter, Waker},
    user::UserModeHooks,
};
use crate::{agent::Agent, cap::Cap};
```
Add `ostd::arch::cpu::context::FpuContext`, `DisabledLocalIrqGuard`.

**Core blocking pattern** (L125-147) - publish waker under the same lock `interrupt` takes, then cancel check reads `redirect`:
```rust
let (waiter, waker) = Waiter::new_pair();
self.state.lock().blocked = Some(waker);
let res = waiter.wait_until_or_cancelled(
    || { wq.enqueue(waiter.waker()); cond() },
    || match self.state.lock().redirect { Some(_) => Err(Interrupted), None => Ok(()) },
);
self.state.lock().blocked = None;
res
```
Add deadline and `#[cfg(ktest)]` failpoints inside this function. Exit criterion: `grep -n wait_until_or_cancelled kernel/src` = 1 hit.

**State struct** (L27-37): add `fpu`, `tid`, `tag` beside `State`; `tid` from `AtomicU64` like `agent::NEXT_ID`. Hook goes in the existing `impl UserModeHooks for Thread` (L149-): `has_kernel_event` stays, add `pre_user_run` per RESEARCH "Kernel FPU hook".

### `kernel/src/ipc.rs` (call/reply/Cancel/stamp)

**Analog:** itself.

**Message + error shape** (L14-32): extend `Message` with `tid: u64, tag: u64`; keep `IpcError` + `From` impls, add variants rather than a new error type.
```rust
pub struct Message { pub sender: Credential, pub words: [u64; 4] }
```
**Stamp point** (L60-64) - kernel fills identity from the sending Thread, never user words:
```rust
q.push_back(Message { sender: from.agent().credential(), words });
drop(q);
self.wq.wake_one();
```
**Cap resolution** (L77-82) - reuse for every handle arg:
```rust
fn endpoint(t: &Thread, slot: usize, need: Rights) -> Result<Arc<Endpoint>, IpcError> {
    match t.agent().cspace.lock().get(slot)?.resolve(need)? {
        Object::Endpoint(ep) => Ok(ep), _ => Err(IpcError::WrongType) } }
```
**Recv** (L68-70): `me.block_on(&self.wq, || self.queue.lock().pop_front())` becomes `block_until`. Drop queue lock before `wake_one` (L63) and never take it while holding `cspace` (cap.rs insert may allocate).
**Cap transfer:** `Cap::derive(mask)` (cap.rs L101) is the only attenuation path.

### `kernel/src/vm.rs` (copy_to_user / copy_from_user)

**Analog:** `handle_cow_fault` (L50-85) and `futex::lookup`.

**Imports** (L6-13): `FrameAllocOptions, PAGE_SIZE, UFrame, io::util::HasVmReaderWriter, vm_space::VmQueriedItem`, `task::disable_preempt`.
**CoW probe, returns Ok(false) when not CoW (cheap pre-break):**
```rust
pub fn handle_cow_fault(space: &VmSpace, agent: &Agent, addr: Vaddr) -> Result<bool, AgentError> {
    let page = addr & !(PAGE_SIZE - 1);
    let guard = disable_preempt();
    let mut c = space.cursor_mut(&guard, &(page..page + PAGE_SIZE)).unwrap();
```
**Frame-to-frame copy** (L81): `new.writer().write(&mut old.reader());`. Cross-space copy uses the same frame-level reader/writer; `VmSpace::reader/writer` reject non-active spaces. Loop: per page `handle_cow_fault` then write; retry max 2 per page; then EFAULT. Charge via `agent.charge_pages`.
**Test template:** B2 `cow_clone` ktests in `user.rs` mod test.

### `kernel/src/cap.rs` (growable CSpace)

**Analog:** itself (L124-160).
```rust
pub struct CSpace { slots: Vec<Option<Cap>> }
pub const SLOTS: usize = 64;
pub fn insert(&mut self, cap: Cap) -> Result<usize, CapError> {
    let i = self.slots.iter().position(Option::is_none).ok_or(CapError::Full)?;
```
Replace `position` scan with free list + `MAX_SLOTS`; keep `get` returning `CapError::NoSlot`. Rewrite ktest at L223 (`for _ in 0..CSpace::SLOTS`).

### `kernel/src/user.rs` (loader, dispatch)

**Analog:** itself.

**Loader to replace** (L76-93) - per-page `cursor_mut(...).map(frame.into(), PageProperty::new_user(flags, CachePolicy::Writeback))`:
```rust
for (va, frame, flags) in [(code, text, PageFlags::RX), (prog::DATA, data, PageFlags::RW)] {
    let mut c = space.cursor_mut(&guard, &(va..va + PAGE_SIZE)).unwrap();
    c.map(frame.into(), PageProperty::new_user(flags, CachePolicy::Writeback));
}
```
Generalize to N image pages + bss + stack; set rsp via `ctx.set_rsp`.
**Dispatch to route through `syscall::decode`** (L197-205):
```rust
ReturnReason::UserSyscall => match ctx.rax() {
    SYS_EXIT => return Exit::Code(ctx.rdi()),
    SYS_SCHED_YIELD => { Task::yield_now(); ctx.set_rax(0); }
    SYS_FUTEX => ctx.set_rax(sys_futex(thread, space, ctx) as usize),
    _ => ctx.set_rax(-ENOSYS as usize),
},
```
Error convention: `-ENOSYS as usize` (L28 `const ENOSYS: isize = 38`). Move constants to `abi`. Keep numbers 24/60/202.
**Arg registers** (`sys_futex`): `ctx.rdi(), rsi(), rdx(), r10(), r8(), r9()`.
**CoW fault in loop** (L207-230) stays: `handle_cow_fault` on WRITE page fault; `Err(_) => return Exit::Budget`.

### ktests (all modules)

**Analog:** `user.rs` `mod test` (L238-):
```rust
fn spawn_user(t: &Arc<Thread>, space: &Arc<VmSpace>, entry: Vaddr) -> ExitSlot {
    install_space_switch();
    ... TaskOptions::new(move || *e.lock() = Some(run(&t2, &space, entry))).data(t.clone()).spawn().unwrap();
fn join(e: &ExitSlot) -> Exit { loop { if let Some(x) = e.lock().take() { return x; } Task::yield_now(); } }
fn wait_blocked(t: &Thread, e: &ExitSlot)
fn handler_pair() -> (Arc<Thread>, Arc<Thread>)
```
Module gate `#[cfg(ktest)] mod test`, `#[ktest]` attribute. Never call `inject_scheduler` in tests; use `install_space_switch`. Ring-3 tests: `spawn_user(include_bytes!("../../userland/out/x.bin"))`, `assert_eq!(join(&e), Exit::Code(0))`.

### `kernel/osdk.sh` (widen mount)

**Analog:** itself (whole file, 9 lines):
```sh
here=$(cd "$(dirname "$0")" && pwd)
exec docker run --rm --device /dev/kvm -v "$here":/work -w /work \
  -v agentic-os-cargo-registry:/root/.cargo/registry \
  agentic-os-kernel-dev:0.19.0 \
  sh -c '"$@"; s=$?; chown -R '"$(id -u):$(id -g)"' /work; exit $s' sh "$@"
```
Change: mount `$here/..` at `/work`, `-w /work/kernel`, chown only `/work/kernel /work/userland`.

### `tools/unsafe_check.py`, `scripts/check.sh`

**Analog:** `tools/wikigraph.py`: shebang `#!/usr/bin/env python3`, module docstring with `Usage: python3 -I tools/... ` and `--self-test`, `from pathlib import Path`, stdlib only, exit 1 on violation. `scripts/check.sh`: `#!/bin/sh` + `set -eu` like `osdk.sh`; order per RESEARCH Pattern 9.

### `userland/*`, `fpu.rs`, `.github/workflows`

No in-repo analog. Planner uses RESEARCH.md: Pattern 1 (target JSON, link.ld, build command), "Flat-image build script", "Tracer ring-3 program", Pitfall 5 (`lateout("rcx") _, lateout("r11") _`). B2 hand-assembled `user.rs::prog` (L40-60) is only the behavioral spec the Rust bins replace (YIELDER/SPINNER/HANDLER).

## Shared Patterns

### Crate-wide safety
**Source:** `kernel/src/lib.rs` L6 `#![deny(unsafe_code)]`. Every new kernel module inherits it; no `allow` outside a ledgered module.

### Lock discipline
**Source:** `thread.rs` L8-13, `ipc.rs` L60-64. `SpinLock<_, LocalIrqDisabled>` for state touched from hooks; drop guards before `wake_*`.

### Errors
**Source:** `ipc.rs` L22-32 (`IpcError` + `From`), `user.rs` L28 negative-errno return. One `Errno` in `abi`; denials map to Linux errno values.

### Shortcut comments
**Source:** `vm.rs` L17 `// shortcut: <limit>, <when to upgrade>`.

### Build path
All commands via `cd kernel && ./osdk.sh ...`; userland built by `userland/build.sh` before `cargo osdk`, never from `build.rs`.

## No Analog Found

| File | Role | Reason |
|---|---|---|
| `kernel/src/fpu.rs` | utility | Only created if safe `FpuContext` route fails (D-02); no unsafe exists in repo |
| `userland/**` | crates/config | First ring-3 Rust code; use RESEARCH Pattern 1 and examples |
| `.github/workflows/*`, `docs/architecture/unsafe-ledger.md` | CI/doc | None exist |

## Metadata

**Analog search scope:** `kernel/src/*.rs`, `kernel/osdk.sh`, `tools/`. `scripts/` and `.github/` do not exist.
**Files scanned:** 10 (read in part; research file read to line 456 of 550, Open Questions tail not read)
**Pattern extraction date:** 2026-10-24
