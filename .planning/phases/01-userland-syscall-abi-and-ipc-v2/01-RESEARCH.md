# Phase 1: Userland, syscall ABI and IPC v2 - Research

**Researched:** 2026-10-24
**Domain:** Rust capability microkernel on Asterinas OSTD 0.19.0 (x86-64, `-smp 1`): ring-3 `no_std` toolchain, syscall decode, user-mode FPU/SSE, call/reply IPC
**Confidence:** MEDIUM-HIGH. OSTD facts and the toolchain recipe were read or run this session. The FPU protocol, cancel-then-wait and the frame-level copy order are designed from verified primitives but not yet executed; they are spike items.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**FPU/SSE gate (KERN-01, SAFE-01)**
- **D-01:** The safe OSTD route gets one spike plan after the OSTD source read. If it fails, go straight to the contained unsafe route. No open-ended safe-route search.
- **D-02:** Containment rule: all FPU/SSE `unsafe` lives in a single module (for example `kernel/src/fpu.rs`) under `#[allow(unsafe_code)]`, capped at about 3 blocks, each with a `// SAFETY:` comment and an entry in `docs/architecture/unsafe-ledger.md`. A CI grep fails if `unsafe` or `allow(unsafe_code)` appears in any other file. `#![deny(unsafe_code)]` stays crate-wide.
- **D-03:** Gate proof: an XMM value survives a yield, and two threads holding different XMM values do not corrupt each other across preemption (per-thread save, which musl needs). AVX and x87 stress are not required for the gate.
- **D-04:** If both the safe and the contained-unsafe routes fail, the phase stops and Corey decides between an OSTD patch, the ADR 0002 Redox re-score, or a scope cut. No automatic switch. — **Reversibility:** one-way — abandoning the OSTD base discards the B2 kernel and restarts B3 on another foundation.

### Claude's Discretion
Not discussed in this session. Research and planning decide, and flag anything that needs Corey:
- IPC cost target: the roadmap criterion records null-IPC N, median and p99 only. Treat it as report-only unless Corey sets a ceiling. No baseline exists yet (Phase 6 compares against Linux).
- User code layout: where ring-3 crates live, flat bootstrap image versus cpio initramfs (a researcher recommendation, not signed off), and how ring-3 test results are reported. Fallback for the initramfs is `include_bytes!` of a cpio.
- Syscall ABI shape: register convention, error return, versioning, and what fuzz-safe covers (numbers, pointers, lengths, capability handles).

### Deferred Ideas (OUT OF SCOPE)
None - discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| KERN-01 | FPU/SSE state per thread across context switches and signal delivery; test keeps an XMM value across a yield and a signal redirect | OSTD enables SSE/XSAVE at boot and exposes a safe `FpuContext`; owner-tracking save in `pre_user_run` (Pattern 3). Redirect rewrites only GPRs. |
| SAFE-01 | Unsafe policy, `docs/architecture/unsafe-ledger.md`, CI grep | `tools/unsafe_check.py` design (Pattern 9). The safe FPU route needs zero ledger entries. |
| KERN-02 | `syscall.rs` typed decode layer with a fuzz ktest | Pattern 4. `is_in_user_space` is the pointer gate. |
| KERN-03 | Rust `no_std` user code for `x86_64-aok-user.json`, flat bootstrap image, replaces hand-assembled B2 programs | Recipe built and disassembled this session (Pattern 1). |
| KERN-05 | CSpace grows past 64 | Pattern 8; `cspace_slots` ktest must be rewritten. |
| KERN-06 | One `copy_to_user` that breaks CoW and retries; first `read` after fork succeeds | Fallible copy returns a partial count, never resolves the fault (Pattern 6). |
| KERN-12 | Shared `block_until`, fail-point ktest, 1000x loop | Built on `Waiter::wait_until_or_cancelled` (Pattern 5). |
| IPC-01 | `call`/`reply`/`reply_recv` | Pattern 7. |
| IPC-02 | Cancel semantics: interrupted `call` stays blocked until reply; blocked `recv` aborts at once | Pattern 7. |
| IPC-03 | Kernel stamps `tid` and `tag` on every message | Pattern 7. |
| IPC-04 | Bulk copy with CoW break on the destination | Pattern 6. |
| IPC-05 | Capability transfer with rights attenuation | Pattern 7; reuses `Cap::derive`. |
| IPC-06 | Null-IPC round trip recorded with N, median, p99 | Pattern 10; `ostd::arch::read_tsc`. |
| TEST-06 | B2 ktests and boot budget check stay green; clippy and fmt in CI | `scripts/check.sh` as the single CI contract (Pattern 9). No CI exists yet. |
</phase_requirements>

## Summary

The highest-risk unknown, user-mode FPU/SSE, is much smaller than STATE.md feared. OSTD 0.19.0 already turns the hardware on at boot (CR0/CR4/XCR0, `fninit`) and ships a public, safe `ostd::arch::cpu::context::FpuContext` with `save()` and `load()`. What it does not do is switch that state between tasks: nothing in OSTD x86 other than the definition itself references `FpuContext`. So the OS must do the switch, and it can do it in safe Rust: a `FpuContext` per `Thread`, loaded from `UserModeHooks::pre_user_run`, with a global "owner" pointer so the previous owner is saved only when ownership changes. Kernel code is built soft-float, so the hardware registers hold only ring-3 state between user runs. The expected outcome is that the safe route passes and `SAFE-01` needs no ledger entry for FPU. This is a design from verified primitives, not a run result; the D-01 spike must prove it with two threads and a negative control.

The user toolchain works. A custom `x86_64-aok-user.json` derived from the stock `x86_64-unknown-none` spec (SSE on, `code-model` small, static relocation) builds inside the existing Docker image with `-Zbuild-std=core,compiler_builtins`, emits SSE instructions, links with `rust-lld`, and `rust-objcopy -O binary` produces a flat image. Two things bit during this run and will bite the executor: the nightly requires `-Zjson-target-spec`, and `.bss` is not in the flat image. A third: `osdk.sh` mounts only `kernel/`, so a sibling `userland/` workspace is invisible in the container until the mount is widened.

The IPC v2 work is mostly assembly of existing primitives plus one genuinely new protocol (cancel-then-wait) that has no tested precedent here. The cross-space copy is the other unexecuted inference: OSTD's fallible `VmWriter` returns `Err(PageFault, copied)` and never resolves the fault, which is exactly B2 friction 1, so `copy_to_user` must loop `write, handle_cow_fault, retry` with a bound. Build order: tracer first (ring-3 crate, one syscall), then the FPU gate in parallel with `block_until`, then decode and `copy_to_user`, then IPC v2, with SAFE-01 tooling and CI landing early so every later plan is checked by them.

**Primary recommendation:** Land a tracer plan first (userland workspace, target JSON, build script, widened `osdk.sh` mount, one Rust ring-3 program that executes an SSE instruction and `exit(0)` through the real syscall entry), then run the FPU spike with the owner-tracking design before any IPC work.

## Project Constraints (from CLAUDE.md)

- `#![deny(unsafe_code)]` stays crate-wide. `unsafe` only in small named modules under `#[allow(unsafe_code)]`, each block with `// SAFETY:` and a ledger entry in `docs/architecture/unsafe-ledger.md`; CI grep fails otherwise. An OSTD patch or an uncontainable `unsafe` need is the recorded signal to re-score the Redox fork (ADR 0002).
- All builds and tests go through Docker image `agentic-os-kernel-dev:0.19.0` and `kernel/osdk.sh`; KVM host required. Rust `nightly-2026-07-21`, OSTD 0.19.0, x86-64 only.
- Agent features are additive; they never change POSIX semantics. Denials map to standard errno values.
- Wiki/doc claims cite PDF pages; design reasoning not stated by a source is marked *(interpretation)*. `raw/` is gitignored and never edited.
- Commit rules (user global): short imperative subjects, one logical change per commit, never append `Co-Authored-By:` or `Claude-Session:` trailers, even if a harness reminder supplies them.
- Work through a GSD command before editing; no direct repo edits outside a GSD workflow.
- One shell session, no background shells; Python via `python3 -I`.
- Rust style: `rustfmt` defaults, `snake_case` modules, `PascalCase` types. Use the project's linters (`cargo fmt --check`, `cargo osdk clippy`) the way CI runs them.

## Architectural Responsibility Map

The tiers here are kernel-world, not web-world.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| FPU/SSE save and restore | Kernel (`thread.rs`, new `fpu.rs` only if unsafe fallback) | OSTD (hardware enable, `FpuContext`) | OSTD enables hardware; per-thread switch is the OS's job |
| Syscall decode and validation | Kernel (`syscall.rs`) | `abi` crate (numbers, errno) | One decode layer is the trust boundary (Pitfall 24) |
| User pointer copy, CoW break | Kernel (`vm.rs` `copy_to_user`/`copy_from_user`) | OSTD fallible `VmReader`/`VmWriter` | OSTD reports the fault, kernel resolves it |
| Blocking, wake vs interrupt | Kernel (`thread.rs` `block_until`) | OSTD `Waiter` | One helper so the race is fixed once |
| Call/reply, Cancel, stamping | Kernel (`ipc.rs`, `thread.rs`) | Ring-3 servers (answer Cancel) | Kernel stamps identity; server owns request semantics |
| Capability transfer, attenuation | Kernel (`cap.rs`, `ipc.rs`) | — | Authority is the kernel's invariant |
| Ring-3 test programs | `userland/` (Rust, `no_std`) | Kernel loader (`user.rs`) | Replaces hand-assembled bytes |
| Image build | `userland/build.sh` in Docker | Kernel `include_bytes!` | Keeps cargo-in-cargo out of `build.rs` |
| Unsafe policy, CI | `tools/unsafe_check.py`, `scripts/check.sh` | `.github/workflows` | Mechanical enforcement of SAFE-01 and TEST-06 |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ostd | 0.19.0 | Kernel base | Pinned: `ostd = "0.19.0"` [VERIFIED: kernel/Cargo.toml, read this session] |
| Rust toolchain | `nightly-2026-07-21`, components `rust-src`, `rustc-dev`, `llvm-tools-preview` | Kernel and ring-3 crates | [VERIFIED: kernel/rust-toolchain.toml] `channel = "nightly-2026-07-21"`. rustc in image is 1.99.0-nightly (2026-07-20), LLVM 22.1.8 [VERIFIED: ran `rustc -vV` in image] |
| `x86_64-aok-user.json` | n/a (in-repo) | Ring-3 target | Built and disassembled this session (Pattern 1) |
| `rust-lld`, `rust-objcopy`, `llvm-objdump`, `llvm-readobj` | ship with the toolchain | Link, flatten, inspect | [VERIFIED: listed `$(rustc --print sysroot)/lib/rustlib/*/bin` in image] `llvm-objcopy`, `llvm-objdump`, `llvm-readobj`, `rust-lld`, `rust-objcopy` present |
| cargo-osdk | 0.19.0 | Build, run, ktest | [VERIFIED: kernel/Dockerfile] `cargo install cargo-osdk --version 0.19.0 --locked` |
| Python 3 (`-I`) | 3.12.3 in image | `tools/unsafe_check.py`, result post-processing | Repo convention (`tools/wikigraph.py`) |

### Supporting
None. Phase 1 needs zero external crates. The user-side crates in `.claude/CLAUDE.md` (goblin, zerocopy, spin, hashbrown, heapless, talc, bitflags) belong to later phases. Wire format is plain `[u64; N]` words and `#[repr(C)]` structs in the in-repo `abi` crate.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Flat image via `include_bytes!` | cpio initramfs via `[boot] initramfs` | Not needed in Phase 1; see Open Question 2. OSDK delivers it, but needs `cpio`, which is not in the image |
| Owner-tracking FPU switch | Pre-schedule handler + `live` flag | Same safety, needs `Task::current()` inside the handler (unverified). Keep as fallback |
| Path dependency on `abi` | `#[path]` module include | Fallback if `cargo osdk test` mishandles a path dep outside `kernel/` |

**Installation:** no package installs. Toolchain and image already exist.

**Version verification:** Versions above were read from files or by running tools in the image this session. No registry query was made because no crate is added.

## Package Legitimacy Audit

No external packages are installed in this phase, so the `package-legitimacy check` gate has nothing to evaluate and was not run. If a plan adds a crate (for example `zerocopy`), it must be tagged `[ASSUMED]`, run through `gsd_run query package-legitimacy check --ecosystem crates <name>`, and carry a `checkpoint:human-verify` task.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 host: scripts/check.sh  (also the CI contract)
   |
   |-- tools/unsafe_check.py ---------------> fails if `unsafe` outside ledger modules
   |-- ./osdk.sh sh -c 'cd ../userland && ./build.sh'
   |        cargo build -Zjson-target-spec -Zbuild-std=core,compiler_builtins
   |        --target x86_64-aok-user.json  --> ELF --rust-objcopy -O binary--> userland/out/*.bin
   |-- ./osdk.sh cargo fmt --check / cargo osdk clippy
   `-- ./osdk.sh cargo osdk test  (ktests in QEMU/KVM, include_bytes! of userland/out/*.bin)

 ktest / kernel_main
   |  user::load_image(bytes) : map image RX/RW pages, bss pages, stack, set rsp
   v
 ring-3 Thread  (Rust no_std, uses SSE)
   | syscall (rax=nr, rdi,rsi,rdx,r10,r8,r9)
   v
 UserMode::execute loop  [ostd 0.19.0 arch/x86/cpu/context/mod.rs:363-]
   might_preempt()  -> pre_user_run(guard)  -> run()   <-- FPU load here
   |
   v ReturnReason::UserSyscall
 syscall.rs  decode(nr, args) -> Result<Syscall, Errno>   (fuzz boundary)
   |
   +-- Exit / Yield / Futex                      (B2, re-routed through decode)
   +-- Call(h, &IpcArgs) --> ipc.rs --block_until--> Endpoint.callers
   |       kernel stamps {Credential, tid, tag}          |
   |       copy_from_user(send_buf) at recv time          v
   +-- ReplyRecv(reply_h, ...) <---- server thread: recv returns (Message, reply handle)
   |       copy_to_user(caller reply_buf) at reply time (breaks CoW, retries)
   +-- Cancel (kernel-originated) --> server queue; caller stays blocked until reply
   `-- CapDerive / CapDrop / transfer-in-message (rights attenuated by Cap::derive)

 results: ring-3 exit code (0 pass, n = failing step) + a results page the ktest reads back
```

### Recommended Project Structure
```
kernel/src/
  syscall.rs        # NEW: Syscall enum, decode(), Errno mapping, UserPtr/UserSlice
  fpu.rs            # ONLY if the safe route fails (D-02); otherwise FPU code lives in thread.rs
  thread.rs         # block_until, tid/tag, per-thread FpuContext, owner tracking
  ipc.rs            # call/reply/reply_recv/cancel, Message{tid,tag}
  vm.rs             # copy_to_user, copy_from_user, cross-space copy
  cap.rs            # growable CSpace
userland/           # NEW sibling workspace, its own [workspace] root
  x86_64-aok-user.json
  link.ld
  build.sh          # builds all bins, flattens to out/*.bin
  abi/              # no_std: syscall numbers, errno, IpcArgs, Message layout (shared with kernel)
  libsys/           # no_std: raw syscall stubs (the only ring-3 module with `unsafe`)
  tests/            # bins: tracer, fpu_a, fpu_b, ipc_server, ipc_client, fork_cow ...
  out/              # generated, gitignored
scripts/check.sh    # fmt, clippy, unsafe_check, userland build, ktests; CI calls only this
tools/unsafe_check.py
docs/architecture/unsafe-ledger.md
```
The kernel depends on `abi` by path (`../userland/abi`). Fallback if `cargo osdk` mishandles that: `#[path = "../../userland/abi/src/lib.rs"] mod abi;`.

### Pattern 1: Ring-3 target, link and flatten (VERIFIED by running)
**What:** Start from the stock spec and change only what user code needs.
Stock spec, verbatim from `rustc -Zunstable-options --print target-spec-json --target x86_64-unknown-none` run in the image: `"code-model": "kernel"`, `"features": "-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float"`, `"rustc-abi": "softfloat"`, `"position-independent-executables": true`, `"static-position-independent-executables": true`.

Working user spec (built and linked this session):
```json
{
  "arch": "x86_64", "code-model": "small", "cpu": "x86-64",
  "crt-objects-fallback": "false",
  "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
  "disable-redzone": true, "features": "+sse,+sse2",
  "linker": "rust-lld", "linker-flavor": "gnu-lld",
  "llvm-target": "x86_64-unknown-none-elf", "max-atomic-width": 64,
  "panic-strategy": "abort", "plt-by-default": false,
  "position-independent-executables": false, "relocation-model": "static",
  "static-position-independent-executables": false,
  "stack-probes": { "kind": "inline" }, "target-pointer-width": 64
}
```
Dropped from stock: `rustc-abi` (softfloat), the `-sse*`/`+soft-float` feature list, `metadata`, `relro-level`, `supported-sanitizers`. `disable-redzone` stays `true` until saved-stack signal frames (Phase 2) skip the 128-byte red zone *(interpretation)*.

Build command that succeeded in the image (`compiler_builtins v0.1.160`, `core v0.0.0`, 10 s):
```sh
RUSTFLAGS="-C link-arg=-Tlink.ld -C link-arg=--no-dynamic-linker" \
cargo +nightly-2026-07-21 build --release -Zjson-target-spec \
  -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem \
  --target x86_64-aok-user.json
rust-objcopy -O binary target/x86_64-aok-user/release/utest out/utest.bin
```
Without `-Zjson-target-spec` the build stops with: `error: \`.json\` target specs require -Zjson-target-spec to be added to the cargo invocation` [VERIFIED: ran it]. Disassembly of the result [VERIFIED: `llvm-objdump -d`]: entry `0x400000`, `movsd (%rcx), %xmm0`, `mulsd (%rax), %xmm0`, `ucomisd`, then `movl $0x3c, %eax; syscall`. Program headers [VERIFIED: `llvm-readobj -l`]: `PT_LOAD` at `0x400000` FileSize 78 flags R+X; `PT_LOAD` at `0x400058` **FileSize 0, MemSize 4096** (the `.bss` stack). The flat file is 88 bytes: `.bss` is absent.

Linker script (worked): `. = 0x400000;` then `.text` (with `*(.text._start)` first), `.rodata`, `.data`, `.bss`, and `/DISCARD/` for `.eh_frame*`, `.comment`, `.note*`. For W^X, put `. = ALIGN(4096);` before `.data` so the loader can map text RX and data RW on separate pages. This page-alignment is untested; the tracer plan verifies it.

**Loader consequences:** `user::load` today maps one RX text frame and one RW data frame and **no stack** [VERIFIED: kernel/src/user.rs:76-93, `(code, text, PageFlags::RX), (prog::DATA, data, PageFlags::RW)`]. The new loader must: map the image bytes page by page, map extra zeroed RW pages for `.bss` (size passed beside the image, for example a 16-byte trailer or a const in `abi`), map a stack region, and set `rsp`. Prefer a kernel-mapped stack at a fixed top address over a `.bss` stack.

### Pattern 2: Syscall ABI shape (discretion; recommended)
- Register convention: Linux-like, unchanged from B2. `rax` = number, `rdi, rsi, rdx, r10, r8, r9` = args; result in `rax`; error = negative errno in the range -4095..-1 *(interpretation, matches how `user.rs` already returns `-ENOSYS as usize`)*. B2 reads it that way [VERIFIED: kernel/src/user.rs:197-205]: `ReturnReason::UserSyscall => match ctx.rax() {`, `SYS_EXIT => return Exit::Code(ctx.rdi()),`, `_ => ctx.set_rax(-ENOSYS as usize),`.
- Numbers: keep B2's `SYS_SCHED_YIELD = 24`, `SYS_EXIT = 60`, `SYS_FUTEX = 202` [VERIFIED: user.rs:22-24] so un-migrated programs keep running during the port. New calls start at `0x100`. Linux numbers stop at `libaok` (roadmap decision), so nothing else may assume Linux numbering.
- Syscalls with more than five payload values do not fit registers. `call`, `recv`, `reply`, `reply_recv` take `(handle, ptr_to_IpcArgs)` where `IpcArgs` is one `#[repr(C)]` struct holding label, words, and `(ptr, len)` for send and reply buffers. The kernel does one fixed-size `copy_from_user` of the struct. This keeps the decode layer to one shape. A register fast path is a later optimisation only if the measured null-IPC cost justifies it.
- Versioning: none in Phase 1. A syscall number is never reused; unknown numbers return `ENOSYS`.
- Fuzz-safe covers: syscall number, every pointer (user-range check plus alignment where the type needs it), every length (capped, default 64 KiB per ARCHITECTURE.md), every capability handle (bounds and type), flag bits (unknown bits return `EINVAL`).

### Pattern 3: Per-thread FPU/SSE, safe route (design; spike required)

**OSTD facts (all VERIFIED, ostd-0.19.0/src, read this session):**
- Hardware is enabled by OSTD. `arch/x86/mod.rs:199-201`: `cr4 |= Cr4Flags::OSFXSR | Cr4Flags::OSXMMEXCPT_ENABLE | Cr4Flags::PAGE_GLOBAL;` and `:196` `cr0 |= Cr0Flags::NUMERIC_ERROR | Cr0Flags::MONITOR_COPROCESSOR;`. `:219` `xcr0 |= XCr0Flags::SSE;` (AVX and AVX512 bits added when present, `:220-225`). `:229` `cpu::context::enable_essential_features();`.
- `arch/x86/cpu/context/mod.rs:739-` `enable_essential_features` records `XSTATE_MAX_FEATURES` and `XSAVE_AREA_SIZE` (`assert!(xsave_area_size <= MAX_XSAVE_AREA_SIZE)`, `MAX_XSAVE_AREA_SIZE = 4096`), clears `Cr0Flags::TASK_SWITCHED | Cr0Flags::EMULATE_COPROCESSOR`, and runs `fninit`.
- The save area is public and safe. `context/mod.rs:582` `pub struct FpuContext { xsave_area: Box<XSaveArea>, area_size: usize }`; `:602` `pub fn save(&mut self)` (uses `_xsave64(mem_addr, XFEATURE_MASK_USER_RESTORE)` when XSAVE is present, else `_fxsave64`); `:615` `pub fn load(&self)`; `:640` `impl Default for FpuContext`; plus `new()`, `as_bytes`, `as_bytes_mut`, `Clone`. `:731` `const XFEATURE_MASK_USER_RESTORE: u64 = 0b1110_0111;`.
- Path: `ostd::arch::cpu::context::FpuContext` (`lib.rs:30-35` `#[cfg_attr(target_arch = "x86_64", path = "arch/x86/mod.rs")] pub mod arch;`, `arch/x86/mod.rs:6` `pub mod cpu;`, `arch/x86/cpu/mod.rs:5` `pub mod context;`). The kernel already imports `ostd::arch::cpu::context::{CpuException, PageFaultErrorCode, UserContext}`.
- OSTD does not switch it. A grep of `FpuContext` across `src/` hits only the definitions in each arch's `context` file (x86 at `:582`, plus arm, riscv, loongarch). No scheduler or `execute` code references it. Absence in a grep is not proof of a design intent, but together with the `execute` loop below it means no automatic switch exists in the paths read.
- The hook is public and safe. `user.rs:134` `fn pre_user_run(&self, _guard: &DisabledLocalIrqGuard) {}` ("Local IRQs are disabled and will only be enabled after entering user space", `:130-133`). `execute` order, `context/mod.rs:367-371`: `crate::task::scheduler::might_preempt();` then `let guard = crate::irq::disable_local();` then `hooks.pre_user_run(&guard);`. Preemption therefore happens at the top of the loop, before `pre_user_run`.
- Kernel never touches XMM: OSTD and the kernel are soft-float. Stock spec `"features": "-mmx,-sse,-sse2,...,+soft-float"` with `"rustc-abi": "softfloat"` [VERIFIED: ran the print command in image]. While the kernel runs, the FPU registers still hold the last ring-3 state. *(inference from the above; the spike's negative control tests it)*
- Also public and safe, needed for musl TLS in Phase 4, same mechanism: `FsBase::{new, addr, save, load}` (`context/mod.rs:112-135`) and `GsBase::{save, load}(&DisabledLocalIrqGuard)` (`:140-`). Out of scope for Phase 1; do not forget them in the Phase 4 plan.

**Recommended protocol (owner tracking, single CPU):** `Thread` gets `fpu: SpinLock<FpuContext, LocalIrqDisabled>`. A kernel-global `FPU_OWNER: SpinLock<Option<Arc<Thread>>, LocalIrqDisabled>` names the thread whose ring-3 state is in the hardware. In `pre_user_run(&self, _)`: if owner is not `self`, `owner.fpu.save()`, then `self.fpu.load()`, then owner = `self`. No scheduler hook, no `Task::current()`, no per-thread flag. A first run has no owner and just loads the default context (`mxcsr = 0x1F80`, `control = 0x037F` per the summary of `XSaveArea::new`; not re-verified, treat as `[ASSUMED]`). This is safe Rust only, so SAFE-01 needs no FPU ledger entry. SMP would need a per-CPU owner; v1 is `-smp 1` [VERIFIED: kernel/OSDK.toml `-smp 1`].

**Fallback if owner tracking fails the spike:** `inject_pre_schedule_handler(handler: fn(&DisabledLocalIrqGuard))` [VERIFIED: task/mod.rs:41] is called at the top of `before_switching_to` (`task/processor.rs:95-98`, `if let Some(handler) = PRE_SCHEDULE_HANDLER.get() { handler(irq_guard); }`) while the outgoing task is still current *(unverified: that `Task::current()` returns the outgoing task inside it)*. Save there when the current thread has a `live` flag set by `pre_user_run`. If both fail, D-02 applies: the only unsafe candidates are raw `xsave`/`xrstor` or CR0 writes, in `kernel/src/fpu.rs`.

**Gate test (D-03), with the discriminating parts spelled out:** a single thread yielding proves nothing, because with one thread the hardware state is never overwritten. The test must run two ring-3 threads, `fpu_a` and `fpu_b`, each loading a distinct 128-bit pattern into several XMM registers, looping with `sched_yield` and a busy section long enough to be preempted by the tick, and checking its registers every iteration (exit code = failing step). Add a negative control: a ktest-only switch that disables the swap, asserting the test then fails. Without that control a pass could be an accident of scheduling order. The signal-redirect half of KERN-01: redirect rewrites only `rdi` and `rip` [VERIFIED: user.rs:231-234 `ctx.set_rdi(ctx.rip()); ctx.set_rip(handler);`], so an XMM value is unaffected; the test asserts that rather than assuming it. Saved-context signal frames are Phase 2.

### Pattern 4: Syscall decode layer (KERN-02)
**What:** `syscall.rs` owns `enum Syscall` with one variant per call, `fn decode(nr: usize, a: [usize; 6]) -> Result<Syscall, Errno>`, and the newtypes `UserPtr<T>`, `UserSlice`, `Handle`. Every validated value is a type, so handlers cannot use a raw `usize`. Pointer check: `ostd::mm::is_in_user_space(vaddr, len)` [VERIFIED: ostd mm/mod.rs:65-67] `vaddr.saturating_add(len) <= MAX_USERSPACE_VADDR`. `VmSpace::reader/writer` repeat that check and also require the space to be the active one: `if current_page_table_paddr() != self.pt.root_paddr() || !super::is_in_user_space(vaddr, len) { return Err(Error::AccessDenied); }` [VERIFIED: mm/vm_space.rs:160-165, 178-183]. The `pt.root_paddr()` check means copies on behalf of a *different* space cannot use `VmSpace::reader/writer` at all, which is why cross-space copy goes at frame level (Pattern 6).
**Fuzz ktest:** deterministic xorshift64 with a fixed, logged seed (no `rand` crate); N (start at 100k) random `(nr, args)` tuples plus a targeted table (nr in `{0, 24, 60, 202, 0x100.., u64::MAX}`, ptr in `{0, 1, MAX_USERSPACE_VADDR - 1, MAX_USERSPACE_VADDR, usize::MAX}`, len in `{0, 1, usize::MAX, 1<<32}`, handle in `{0, 63, 64, u32::MAX}`). Assert: no panic, return is `Ok` or a documented errno. Run it through `decode` plus the handler on a real thread/space, not through `decode` alone, so handler-side arithmetic overflow is covered. Pitfall 23 also requires one test per new operation through the real `syscall` entry (a ring-3 program), not only direct calls.

### Pattern 5: `block_until` (KERN-12)
**Existing shape:** `Thread::block_on(wq, cond) -> Result<R, Interrupted>` built on `Waiter::new_pair` plus `wait_until_or_cancelled` (kernel/src/thread.rs). OSTD semantics [VERIFIED: ostd sync/wait.rs:209-231]:
```rust
loop {
    if let Some(res) = cond() { return Ok(res); };
    if let Err(e) = cancel_cond() {
        // Close the waker and check again to avoid missing a wake event.
        self.waker.close();
        return cond().ok_or(e);
    }
    self.wait();
}
```
OSTD already re-checks `cond()` after a cancel, so a wake that races an interrupt is not lost *inside this function*. The B2 hand-rolled recheck in `futex::wait` ("A wake that raced the interrupt wins") duplicates it, and PITFALLS Pitfall 2 records that deleting such a recheck still passes 27/27. The helper therefore has to be the only place this logic exists, with the deadline and the fail-point hook inside it:
```rust
pub fn block_until<R>(&self, wq: &WaitQueue, mut cond: impl FnMut() -> Option<R>,
                      deadline: Option<Tick>) -> Result<R, BlockError /* Interrupted | TimedOut */>
```
Fail-point: a `#[cfg(ktest)]` static `FAILPOINT: AtomicU8` checked at three named points (after cond false and before sleeping; after wake and before returning; between interrupt flag set and cancel check). The test sets the point, forces the other event (wake or `interrupt`) at exactly that moment, and asserts the documented winner. The 1000x loop runs each interleaving 1000 times in one ktest. Migrate `Thread::block_on`, `futex::wait` and `Endpoint::recv` onto it; `grep -n "wait_until_or_cancelled" kernel/src` must then show exactly one hit.

### Pattern 6: `copy_to_user` and cross-space copy (KERN-06, IPC-04)
**OSTD fact:** the fallible copy resolves nothing. `mm/io/mod.rs:353-357`: `if copied_len < copy_len { Err((Error::PageFault, copied_len)) } else { Ok(copied_len) }`. A kernel write to a CoW page after `cow_clone` therefore returns a short count and the caller sees a fault, which is B2 friction 1.
**Existing templates:** `futex::lookup` already does `is_in_user_space`, `handle_cow_fault`, then queries the frame (kernel/src/futex.rs). `vm::handle_cow_fault(space, agent, addr) -> Result<bool, AgentError>` copies with `new.writer().write(&mut old.reader())` and takes over the frame when `frame.reference_count() == 1` (kernel/src/vm.rs). `reference_count` can overcount for one RCU grace period (B2 friction 2), so the `sole` fast path may take the copy path unnecessarily; that is safe, only slower.
**Recommended `copy_to_user(space, dst, src_bytes)`:** for each page the range touches: validate range, call `handle_cow_fault(space, agent, page_addr)` first (cheap when not CoW: returns `Ok(false)`), then write; on a short write, call `handle_cow_fault` for the faulting address and retry, **at most twice per page**, then return `EFAULT`. Unbounded retry livelocks on a genuinely unmapped page. `copy_from_user` mirrors it without the CoW break. Cross-space (server to caller's buffer) goes at frame level: look up the destination frame through the destination `VmSpace` cursor, break CoW there, then `UFrame` writer/reader between frames, as `handle_cow_fault` already does. Never rely on `VmSpace::writer` for a space that is not currently active (the `root_paddr` check above rejects it).
**TLB:** `VmSpace::activate` returns early if the space is already active (`if last_ptr == Arc::as_ptr(self) { return; }`, vm_space.rs:126-134) and otherwise ends in `self.pt.activate()`; the CR3 write is `Cr3Flags::empty()` (arch/x86/mm/mod.rs:140-143) and `enable_cpu_features` sets `OSFXSR | OSXMMEXCPT_ENABLE | PAGE_GLOBAL | OSXSAVE | FSGSBASE` but not PCID (arch/x86/mod.rs:199-215). So a space switch flushes non-global TLB entries; global kernel entries survive. This answers the roadmap's "does `VmSpace::activate` keep TLB entries" for same-CPU, no-PCID. Whether `cow_clone`'s remote shootdown is awaited is moot at `-smp 1` [VERIFIED: OSDK.toml].
**Regression test (Pitfall 4):** fork a space with `cow_clone`, then deliver an IPC message with a multi-page payload into a buffer in the child. Compare byte for byte, then check the parent's pages are unchanged.

### Pattern 7: IPC v2 on the existing primitives (IPC-01, -02, -03, -05)
Existing: `Message { sender: Credential, words: [u64; 4] }`, `Endpoint { queue: SpinLock<VecDeque<Message>>, wq: WaitQueue }` with `QUEUE_LEN = 16`, `IpcError { Cap, WrongType, Full, Interrupted }`, cspace-slot resolution in `sys_send`/`sys_recv` (kernel/src/ipc.rs). Design reference is `.planning/research/ARCHITECTURE.md` Patterns 1, 2, 4 (read in full); build from it:
- `Message` gains `tid: u64` and `tag: u64`, stamped by the kernel from the sending `Thread`, never from user words. Each `Thread` gets a `tid` from an `AtomicU64` (as `agent::NEXT_ID` does) and a `tag` set once at creation. In Phase 1 the tag is assigned by whoever spawns the thread (kernel test harness now, procmgr in Phase 4); inheritance on clone is a Phase 2 decision.
- `call(ep, &IpcArgs)`: the caller's message and buffer descriptors live in the caller's `Thread` (not a queue slot), the endpoint holds a list of blocked callers, so `Full` stops being client-visible for calls. `recv` returns the message plus a reply handle (kernel table, per server thread, with a generation counter so a stale handle cannot hit a reused slot). `reply(handle, ...)` wakes the caller. `reply_recv` is `reply` followed by `recv` without returning to user mode in between.
- Cancel-then-wait: `Thread::interrupt` on a thread blocked in `call` queues a kernel `Cancel(reply handle)` for the server (its own slot, reserved so it cannot be dropped as `Full`) and the caller stays blocked until `reply` runs. A blocked `recv` still aborts at once. If the caller is killed, pending replies to it are dropped by the kernel. The server must answer a Cancel; the ktest server does, and a second ktest checks that a server that never answers leaves the caller blocked (documented behavior, `SIGKILL` bypass is Phase 2).
- Capability transfer: a message may carry up to N handle slots; the kernel resolves each in the sender's cspace, requires `GRANT`, derives with `Cap::derive` (rights can only shrink; B2 enforces this), and installs into the receiver's cspace, returning the new handles in the received message. Ktest: send a `SEND|GRANT` cap attenuated to `SEND`; the receiver cannot derive or use `RECV`; receiver gets `EPERM`-class errno (Linux errno chosen in `abi`).
- Test servers: use a kernel-side server task for protocol-level ktests (matches the existing `spawn_user`/`handler_pair` helpers) and one ring-3 server for the tracer and for null-IPC.

### Pattern 8: Growable CSpace (KERN-05)
Today: `CSpace { slots: Vec<Option<Cap>> }`, `SLOTS = 64`, `insert` uses `position(is_none)` (kernel/src/cap.rs). Change: `Vec` grows on demand, free slots tracked by a free list (O(1) insert), hard cap `MAX_SLOTS = 65_536` `[ASSUMED]` returning `ENOSPC`-class error. The ktest `cspace_slots`, which asserts `Full` after 64 inserts, is rewritten to assert 200 inserts and derives succeed and that the cap still fails cleanly. The agent's `pub cspace: SpinLock<CSpace>` lock is held across `insert`; the Vec may allocate, so do not insert while holding an IPC queue lock.

### Pattern 9: SAFE-01 tooling and TEST-06 CI
- `tools/unsafe_check.py` (run with `python3 -I`, no dependencies): strip `//` and `/* */` comments and string literals, then match tokens `unsafe` (including `unsafe(` attributes, `unsafe extern`, `unsafe impl`, `unsafe fn`, `unsafe {`) and `allow(unsafe_code)` in `kernel/src/**/*.rs` and `userland/**/src/**/*.rs`. Fail if a hit is in a file not listed in the ledger, or if a ledger entry has no matching block in its file. A word grep alone false-positives on `// SAFETY:` prose and on edition-2024 `#[unsafe(no_mangle)]` spellings.
- Ledger format `docs/architecture/unsafe-ledger.md`: one table row per block (file, line anchor, what it does, why no safe alternative, the invariant). Phase 1 expects the kernel table to be empty on the safe FPU route, and a short "ring-3 userland" section for the `asm!` syscall stubs and `rdtsc` in `libsys`. Whether ring-3 `unsafe` belongs under SAFE-01 is Open Question 3.
- `scripts/check.sh` runs, in CI order: `tools/unsafe_check.py`, userland build, `./osdk.sh cargo fmt --check`, `./osdk.sh cargo osdk clippy`, `./osdk.sh cargo osdk test`. CI is a thin wrapper around this. There is no `.github/workflows` directory today [VERIFIED: `ls /home/rakdos/repos/agentic-os-kernel/.github/workflows` failed: No such file or directory], so TEST-06's "in CI" requires creating it.

### Pattern 10: Null-IPC measurement (IPC-06)
Use `ostd::arch::read_tsc()` (`arch/x86/mod.rs:147-152`, `pub fn read_tsc() -> u64 { ... _rdtsc() }`) and `tsc_freq()` (`:140`). Measure from ring-3 with `rdtsc` in `libsys`: client does `call` on an echo server doing `reply_recv`, records cycles per iteration into a fixed array, sorts with `slice::sort_unstable`, writes `{n, median, p99, tsc_freq_unknown}` into a results page at a fixed address; the ktest reads the page back after join and prints it. Starting parameters `[ASSUMED]`: 1,000 warm-up, N = 10,000. The number is only meaningful under KVM; under TCG it is not a cost, so the ktest records and prints but asserts only `n` and `median > 0`. Report-only, per the CONTEXT discretion note. Print the TSC frequency beside the cycles so the figure converts to time.

### Anti-Patterns to Avoid
- **Cargo inside `build.rs`:** a kernel `build.rs` that shells out to `cargo build` the user crate under `cargo osdk` nests two nightly `-Zbuild-std` builds. Use `userland/build.sh` run before `cargo osdk`.
- **Testing the FPU gate with one thread:** passes with no mechanism.
- **Direct handler calls in tests:** Pitfall 23; every new op gets one test through the real syscall entry.
- **Second copy of ABI constants:** numbers, errno and `IpcArgs` live only in `abi`.
- **Unbounded `handle_cow_fault` retry.**

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| FPU state container | A raw 512/4096-byte buffer with inline asm | `ostd::arch::cpu::context::FpuContext` | Public, safe, handles XSAVE vs FXSAVE and masks (`context/mod.rs:582-`) |
| Sleep/wake race | A new waiter loop | `Waiter::wait_until_or_cancelled` inside one `block_until` | OSTD already closes the waker and rechecks (`sync/wait.rs:209-231`) |
| User range check | Custom bounds arithmetic | `ostd::mm::is_in_user_space` | Saturating add, matches OSTD's own limit |
| CoW break | A second CoW routine | `vm::handle_cow_fault` | Already tested by the B2 ktests |
| Rights attenuation | A transfer-specific check | `Cap::derive` | Same invariant as `sys_derive` |
| Sorting for median/p99 | Hand sort | `slice::sort_unstable` (in `core`) | No allocator needed |
| ELF parsing in the kernel | A loader | Flat image only in Phase 1 | goblin and exec server are Phase 4 |
| Syscall instruction wrapper in each test | Per-program asm | `libsys` | One audited unsafe module |

**Key insight:** every hard primitive this phase needs already exists either in OSTD or in B2. The new code is glue plus two protocols (owner-tracking FPU, cancel-then-wait). Prefer extending B2 modules over adding abstractions.

## Common Pitfalls

### Pitfall 1: `osdk.sh` cannot see `userland/`
**What goes wrong:** the container mounts only `kernel/` at `/work`, so `include_bytes!("../../userland/out/x.bin")` and the `abi` path dependency fail to resolve.
**Why:** [VERIFIED: kernel/osdk.sh] `docker run --rm --device /dev/kvm -v "$here":/work -w /work`, where `here` is the `kernel/` directory.
**How to avoid:** mount the repo root at `/work`, set `-w /work/kernel`, and restrict the trailing `chown -R` to `/work/kernel /work/userland` (not `raw/`, which holds large PDFs). Interface stays `cd kernel && ./osdk.sh ...`, so every documented command is unchanged. This is the first tracer task.
**Warning signs:** `couldn't read ../../userland/out/...` or `failed to load manifest for dependency abi`.

### Pitfall 2: nightly rejects the JSON target
**What goes wrong:** `error: \`.json\` target specs require -Zjson-target-spec`.
**How to avoid:** always pass `-Zjson-target-spec` [VERIFIED: ran it]. Put it and the `-Zbuild-std` flags in `userland/.cargo/config.toml` (`[unstable] json-target-spec = true, build-std = [...]`) so editors and clippy agree. Not run this session; the CLI form was.

### Pitfall 3: `.bss` and the stack are missing from the flat image
**What goes wrong:** the image is 88 bytes, `.bss` is 4096 NOBITS bytes [VERIFIED: `llvm-readobj -l`]. A loader that maps only the file bytes faults on the first stack push or static write.
**How to avoid:** kernel maps image pages plus a declared bss size plus an explicit stack, sets `rsp` and `rip`. The current loader maps no stack [VERIFIED: user.rs:76-93].

### Pitfall 4: FPU gate passes for the wrong reason
**What goes wrong:** one thread, or two threads that never preempt, pass with no save/restore.
**How to avoid:** two threads, distinct patterns in several XMM registers, a busy section longer than one tick (the tick preempts, `enable_preemption_on_cpu`), and a negative-control run with the swap disabled that must fail.

### Pitfall 5: ring-3 `syscall` asm without clobbers
**What goes wrong:** the `syscall` instruction overwrites `rcx` (return `rip`) and `r11` (`rflags`). A stub that does not declare them lets the compiler keep live values there. `[ASSUMED]` (x86-64 architecture behavior from training; the tracer exercises it).
**How to avoid:** every `libsys` stub uses `lateout("rcx") _, lateout("r11") _` and treats `rax` as the result.

### Pitfall 6: cancel with no server-side protocol (PITFALLS 1)
**How to avoid:** the ktest server answers Cancel; a second test proves the documented behavior when it does not. Reserve a queue slot for kernel-originated Cancel so `Full` cannot drop it.

### Pitfall 7: reply to a dead or recycled waiter
**What goes wrong:** the caller was killed or its slot reused; a late `reply` writes into a stranger.
**How to avoid:** reply handles carry a generation. `reply` to a gone caller returns `ESRCH`-class success-with-drop to the server, never an error that wedges it; copy-out is skipped.

### Pitfall 8: ktest scheduler install
**What goes wrong:** `inject_scheduler` panics if any task exists, so ktests cannot call it [VERIFIED by reading sched.rs in this task's earlier pass: `install_space_switch()` is idempotent for ktests]. New code that needs scheduler setup must go through `install_space_switch` in tests.

### Pitfall 9: edition-2024 and `deny(unsafe_code)` in user crates
`#[unsafe(no_mangle)]` and `static mut` references are lints/errors under edition 2024 `[ASSUMED]`; my probe used `#[unsafe(no_mangle)]` and compiled. Keep ring-3 `unsafe` confined to `libsys` and the start stub, `#![deny(unsafe_code)]` in each test bin, and route stack/statics through the kernel mapping to avoid `static mut`.

### Pitfall 10: path dependency across workspaces under `cargo osdk`
`cargo osdk test` builds generated base crates under `target/osdk/`. A path dependency to `../userland/abi` is expected to resolve from the kernel manifest, but this was not run. The tracer plan tests it first; fallback is the `#[path]` include.

## Code Examples

### Flat-image build script (verified pieces)
```sh
#!/bin/sh
# userland/build.sh  (run inside the image: ./osdk.sh sh -c 'cd ../userland && ./build.sh')
set -eu
RUSTFLAGS="-C link-arg=-Tlink.ld -C link-arg=--no-dynamic-linker" \
  cargo build --release -Zjson-target-spec \
  -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem \
  --target x86_64-aok-user.json
mkdir -p out
for b in target/x86_64-aok-user/release/*; do
  [ -f "$b" ] && [ -x "$b" ] && rust-objcopy -O binary "$b" "out/$(basename "$b").bin"
done
```
[VERIFIED: the cargo and objcopy lines ran this session on a one-file crate; the loop is untested.]

### Tracer ring-3 program (compiled and disassembled this session)
```rust
#![no_std]
#![no_main]
use core::arch::{asm, global_asm};

global_asm!(
    ".section .text._start,\"ax\"", ".global _start", "_start:",
    "lea rsp, [rip + {stack} + 4096]", "call {main}", "ud2",
    stack = sym STACK, main = sym user_main,
);
#[unsafe(no_mangle)]
static mut STACK: [u8; 4096] = [0; 4096];

extern "C" fn user_main() -> ! {
    let c = core::hint::black_box(1.5f64) * core::hint::black_box(2.25f64);
    let code = if c > 3.0 { 0 } else { 1 };
    unsafe { asm!("syscall", in("rax") 60, in("rdi") code, options(noreturn)) }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
```
Production version replaces the `.bss` stack with the kernel-provided one and `asm!` with `libsys`. Ktest side: `spawn_user` the image, `join`, `assert_eq!(exit, Exit::Code(0))`.

### Kernel FPU hook (design, not yet compiled)
```rust
// thread.rs
use ostd::arch::cpu::context::FpuContext;
static FPU_OWNER: SpinLock<Option<Arc<Thread>>, LocalIrqDisabled> = SpinLock::new(None);

impl UserModeHooks for Thread {
    fn has_kernel_event(&self) -> bool { /* unchanged */ }
    fn pre_user_run(&self, _g: &DisabledLocalIrqGuard) {
        let me = self.arc();                       // however Thread exposes its Arc
        let mut owner = FPU_OWNER.lock();
        if owner.as_ref().is_some_and(|o| Arc::ptr_eq(o, &me)) { return; }
        if let Some(prev) = owner.as_ref() { prev.fpu.lock().save(); }
        self.fpu.lock().load();
        *owner = Some(me);
    }
}
```
Source for the APIs: ostd `user.rs:126-134`, `arch/x86/cpu/context/mod.rs:582-640`. The `Arc<Thread>` access and exit-time owner release are details for the spike.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| B2 hand-assembled byte arrays | Rust `no_std` bins, flat image | This phase | Tests scale; SSE usable |
| Fire-and-forget `send` into a 16-slot queue | `call`/`reply` with kernel-held reply handle | This phase | Servers can park requests |
| Interrupt aborts any blocked IPC | Cancel-then-wait for `call`; abort stays for `recv` | This phase | `read` interrupted loses no data |
| 64-slot CSpace | Growable with free list | This phase | Servers hold many caps |
| Stock `x86_64-unknown-none` for user code | `x86_64-aok-user.json` | This phase | Stock is soft-float, kernel code model |

**Deprecated/outdated:**
- docs.rs for ostd 0.19.0 does not build (STATE.md); all OSTD claims here come from the crate source.
- The wording "interrupt aborts blocked IPC" is replaced by Cancel semantics (roadmap decision).

## Runtime State Inventory

Not a rename or migration phase; omitted.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Owner-tracking FPU swap in `pre_user_run` is sufficient on `-smp 1`, because kernel code is soft-float and leaves XMM untouched | Pattern 3 | Corrupted XMM across preemption; fall back to the pre-schedule `live`-flag design, then the D-02 unsafe module |
| A2 | `Task::current()` inside the pre-schedule handler returns the outgoing task (fallback design only) | Pattern 3 | Fallback unusable; owner-tracking remains |
| A3 | `FpuContext::default()` yields `mxcsr = 0x1F80`, `control = 0x037F` | Pattern 3 | First user run starts with masked/unmasked exceptions differing from expectation |
| A4 | The `syscall` instruction clobbers `rcx`/`r11` (architecture fact from training) | Pitfall 5 | Rare register corruption in ring-3 stubs |
| A5 | `cargo osdk test` accepts a path dependency outside `kernel/` | Pitfall 10 | Use the `#[path]` include fallback |
| A6 | `.github` hosted runners expose `/dev/kvm`, or a self-hosted runner exists | Pattern 9 | CI cannot run ktests; local `scripts/check.sh` stays the gate (also STATE.md blocker) |
| A7 | Page-aligning `.data` in `link.ld` lets the loader map text RX and data RW separately | Pattern 1 | W^X not enforceable; tracer plan verifies |
| A8 | `MAX_SLOTS = 65_536` and fuzz N = 100k, null-IPC N = 10,000 with 1,000 warm-up | Patterns 4, 8, 10 | Tunable; no external dependency |
| A9 | TSC is stable and meaningful under QEMU/KVM with `-cpu Icelake-Server` | Pattern 10 | Null-IPC cycles unreliable; record, do not gate |
| A10 | `disable-redzone: true` is the right default for user code until Phase 2 signal frames | Pattern 1 | Slightly slower code; reversible |
| A11 | Kernel-assigned `tag` in Phase 1 is set by the spawner and not inherited on clone | Pattern 7 | Phase 2 process objects may want inheritance; revisit there |
| A12 | The Phase 1 user binaries need no `alloc` | Pattern 1 | Add `alloc` to `-Zbuild-std` and a bump allocator if a test needs it |

## Open Questions

1. **IPC cost ceiling (CONTEXT discretion, needs Corey if he wants one).**
   - Known: criterion records N, median, p99 only; no baseline until Phase 6.
   - Unclear: whether a regression guard is wanted.
   - Recommendation: report-only; print numbers in the ktest log and in the phase summary.
2. **Flat image versus cpio initramfs (CONTEXT discretion; recommendation needs sign-off).**
   - Known: OSDK takes `boot.initramfs` (`BootScheme.initramfs: Option<PathBuf>`, cargo-osdk-0.19.0 `src/config/scheme/boot.rs`; CLI `--initramfs`). GRUB command per protocol: `module --nounzip` (Multiboot), `module2 --nounzip` (Multiboot2), `initrd` (Linux) (`src/commands/build/grub.rs:130-165`). OSTD hands it back as `boot_info().initramfs: Option<&'static [u8]>` (`boot/mod.rs:32-38`), and the multiboot2 path reads the first module tag (`arch/x86/boot/multiboot2/mod.rs:72-85`). The file is stored as `boot/initramfs.cpio.gz` but `--nounzip` suggests raw bytes are delivered. `cpio` is not in the image [VERIFIED: `which` printed no cpio].
   - Unclear: which boot protocol `grub-rescue-iso` selects by default here; not read.
   - Recommendation: `include_bytes!` of flat images in Phase 1 (zero new tooling). Revisit initramfs when Phase 3/4 needs multiple files.
3. **Does SAFE-01 cover ring-3 `unsafe`?** The `asm!` syscall stubs and `rdtsc` are `unsafe` by language rule. Recommendation: include `userland/` in the grep with its own ledger section, so the policy is uniform, and state that ring-3 `unsafe` does not weaken the kernel claim.
4. **CI runner with KVM.** Known: STATE.md says KVM on the CI host is assumed, not checked. Recommendation: build `scripts/check.sh` first; Corey picks the runner.
5. **Serial RX.** Known: the NS16550A driver exposes a polled `Ns16550aUart::recv(&mut self) -> Option<u8>` (`console/uart_ns16650a.rs:106`) behind `ostd::arch::serial::SERIAL_PORT`; interrupt-driven RX was not investigated. Not needed in Phase 1 (no tty until Phase 5). Recommendation: drop it from Phase 1 scope.
6. **`option-b-microkernel.md` section 7 and ADR 0002 were not read** this pass beyond the headings; PITFALLS and ARCHITECTURE cite them. Low risk for Phase 1.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker + image `agentic-os-kernel-dev:0.19.0` | All builds | yes | image present on host | — |
| `/dev/kvm` | ktests, null-IPC | yes (passed through by `osdk.sh`; the sandbox runs here under WSL2) | — | TCG is too slow and makes cycles meaningless |
| rustc nightly | Kernel and ring-3 | yes | 1.99.0-nightly (2026-07-20), LLVM 22.1.8 | — |
| `rust-lld`, `rust-objcopy`, `llvm-objdump`, `llvm-readobj` | Link, flatten, inspect | yes | toolchain-bundled | binutils `objcopy` is also in the image |
| `ld.lld` on PATH | — | no | — | Not needed: `rust-lld` is the linker named in the target spec |
| `cpio` | initramfs only | no | — | Not needed in Phase 1 |
| clang | Phase 4 C builds | 18.1.3 in image | — | LLVM 21+ is a Phase 4 concern; Phase 1 builds no C |
| Python 3 | `unsafe_check.py` | yes | 3.12.3 | — |
| `xxd` | inspection only | no | — | `od -An -tx1` |
| crates.io network for `-Zbuild-std` | First userland build | yes (built this session; `compiler_builtins 0.1.160` fetched/compiled) | — | Registry volume `agentic-os-cargo-registry` caches it |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** `cpio` (deferred), `xxd` (use `od`).

## Security Domain

`security_enforcement` is on; ASVS level 1. This phase is a kernel trust boundary, so the applicable controls are input validation and access control.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (identity is kernel-stamped, not authenticated) |
| V3 Session Management | no | — |
| V4 Access Control | yes | Capability handles with rights; `Cap::derive` attenuation; kernel stamps `{Credential, tid, tag}` |
| V5 Input Validation | yes | `syscall.rs` typed decode; `is_in_user_space`; length caps; fuzz ktest |
| V6 Cryptography | no | — |

### Known Threat Patterns for a capability microkernel

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Forged sender identity in a message | Spoofing | Kernel stamps `sender`, `tid`, `tag`; user words never populate them |
| Handle confusion (wrong type, out of range, stale reply handle) | Elevation of privilege | Typed `Handle`, bounds and type check at decode, generation counter on reply handles |
| Rights amplification via transfer | Elevation of privilege | Transfer goes through `Cap::derive`; ktest sends a narrowed cap and tries to use the removed right |
| Kernel read/write through a hostile pointer | Tampering / Information disclosure | Range check plus fallible `VmReader`/`VmWriter`; short copy returns `EFAULT` |
| Pointer arithmetic overflow | Tampering | `is_in_user_space` uses `saturating_add`; length caps before any multiplication |
| Resource exhaustion via CSpace growth or caller floods | Denial of service | `MAX_SLOTS` cap; blocked callers bounded by thread count; charge pages to the agent budget |
| FPU state leak between agents | Information disclosure | Per-thread `FpuContext` is saved and replaced on owner change, so one thread's XMM values are never visible to another |
| Wedged server blocks callers | Denial of service | Documented: Cancel-then-wait trades this for no data loss; kill path is Phase 2 |

## Sources

### Primary (HIGH confidence)
- OSTD 0.19.0 crate source, extracted from the `agentic-os-cargo-registry` docker volume (`/root/.cargo/registry/src/*/ostd-0.19.0`) into the session scratchpad. Files read: `src/arch/x86/mod.rs` (full), `src/arch/x86/cpu/context/mod.rs` (FpuContext `:582-`, `execute` `:363-`, `FsBase`/`GsBase` `:112-`, `enable_essential_features` `:739-`), `src/user.rs`, `src/task/mod.rs`, `src/task/processor.rs`, `src/sync/wait.rs`, `src/mm/mod.rs`, `src/mm/vm_space.rs`, `src/mm/io/mod.rs`, `src/boot/mod.rs`, `src/arch/x86/boot/multiboot2/mod.rs`, `src/arch/x86/serial.rs`, `src/console/uart_ns16650a.rs`, `src/arch/x86/mm/mod.rs`. Same version the kernel builds against (`kernel/Cargo.toml`).
- cargo-osdk 0.19.0 source (same volume): `src/commands/build/{mod,grub}.rs`, `src/config/{mod,scheme/boot}.rs`.
- Repo: `kernel/src/{lib,thread,sched,user,ipc,cap,agent,vm,futex}.rs`, `kernel/{Cargo.toml,OSDK.toml,rust-toolchain.toml,osdk.sh,Dockerfile}`, `docs/architecture/b2-spike-report.md`, `.planning/{REQUIREMENTS,ROADMAP,STATE}.md`, `01-CONTEXT.md`, `.planning/research/{ARCHITECTURE,PITFALLS}.md`.
- Commands run in the image: `rustc -vV`, `rustc -Zunstable-options --print target-spec-json --target x86_64-unknown-none`, toolchain `bin` listing, the full ring-3 build, `llvm-objdump -d`, `llvm-readobj` (file headers and program headers). Scratch crate: `.../scratchpad/utest/`.

### Secondary (MEDIUM confidence)
- `.planning/research/STACK.md` (via `.claude/CLAUDE.md`): version pins for later-phase crates; not re-verified and not used for Phase 1 decisions.

### Tertiary (LOW confidence)
- Training knowledge for the `syscall` clobbers, edition-2024 attribute syntax, and runner `/dev/kvm` availability; listed in the Assumptions Log.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH. Every tool and version was read or run in the image.
- Architecture: MEDIUM. The FPU protocol, cancel-then-wait and frame-level copy are designed from verified primitives, not yet run.
- Pitfalls: MEDIUM-HIGH. Items 1-3 were hit while researching; others come from PITFALLS.md and the OSTD source.

**Research date:** 2026-10-24
**Valid until:** 2026-11-23 (OSTD 0.19.0 and the toolchain are pinned; the risk is in the unexecuted designs, not in version drift)

## Suggested plan breakdown (input for the planner)

MVP tracer first, then the stop/go gate, then the rest. Each plan ends green on `scripts/check.sh` as far as it exists.

| # | Plan | Reqs | Depends | Notes |
|---|------|------|---------|-------|
| 1 | Tracer: widen `osdk.sh` mount, `userland/` workspace, target JSON, `link.ld`, `build.sh`, `abi` crate, new loader (image + bss + stack), ring-3 program using SSE and `exit(0)` through the real syscall entry; decide path-dep vs `#[path]` | KERN-03 | none | First proof of the whole chain; resolves Pitfalls 1-3, 10 |
| 2 | SAFE-01 tooling and CI contract: ledger file, `tools/unsafe_check.py`, `scripts/check.sh`, `.github/workflows` stub | SAFE-01, TEST-06 | 1 | Lands before more code so it checks everything after |
| 3 | FPU spike (D-01): owner-tracking, two ring-3 threads, negative control, redirect case | KERN-01 | 1 | Stop/go gate. On failure go to the D-02 module; on double failure stop (D-04) |
| 4 | `block_until` with deadline and fail-point; migrate `block_on`, `futex::wait`, `Endpoint::recv`; 1000x loop | KERN-12 | none (parallel with 3) | Touches thread, ipc, futex |
| 5 | `syscall.rs` decode layer, `copy_to_user`/`copy_from_user` with bounded CoW retry, fuzz ktest | KERN-02, KERN-06 | 1 | Re-routes B2 syscalls through decode |
| 6 | Growable CSpace | KERN-05 | none | Rewrite `cspace_slots` |
| 7 | IPC v2 core: `tid`/`tag` stamp, `call`/`recv`/`reply`/`reply_recv`, reply handles, Cancel-then-wait; one ring-3 call/reply (the MVP IPC slice) | IPC-01, IPC-02, IPC-03 | 4, 5 | Cancel has no tested precedent |
| 8 | Bulk copy at frame level with destination CoW break; capability transfer with attenuation | IPC-04, IPC-05 | 5, 6, 7 | CoW-after-clone regression test |
| 9 | Port remaining hand-assembled B2 programs to Rust bins; null-IPC measurement with N, median, p99 | KERN-03, IPC-06, TEST-06 | 7 | B2 ktests and boot budget check stay green |

Plan 3 is the only plan that can stop the phase; keep it small and early.
