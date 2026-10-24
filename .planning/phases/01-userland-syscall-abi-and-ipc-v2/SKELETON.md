# Walking Skeleton — agentic-os-kernel

**Phase:** 1
**Generated:** 2026-10-24

This project has no database, UI or deployment. The skeleton is the thinnest kernel slice that
touches every layer Phase 1 builds on: a ring-3 Rust crate compiled inside the Docker image, linked
into a flat image, embedded in the kernel with `include_bytes!`, loaded into its own address space,
run in ring 3, and observed by a ktest.

## Capability Proven End-to-End

A Rust `no_std` ring-3 program, built for `x86_64-aok-user.json` in `agentic-os-kernel-dev:0.19.0`
and loaded from a flat image, executes an SSE instruction, writes a `.bss` static, makes one
`call`/reply IPC round trip to a kernel-side server, and exits 0; the ktest
`tracer_runs_rust_ring3_image` sees `Exit::Code(0)`.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| User target | One custom target, `userland/x86_64-aok-user.json` (SSE on, code-model small, static relocation, `rust-lld`, panic abort, red zone off). Stock `x86_64-unknown-none` is never used for user code. | Stock target is soft-float with the kernel code model; user code must share an ABI with musl later (STACK.md, ROADMAP decision "one custom user target JSON") |
| Toolchain | `nightly-2026-07-21` (kernel pin), `-Zbuild-std=core,compiler_builtins` with `compiler-builtins-mem`, `llvm-objcopy` from `llvm-tools-preview` | One toolchain for kernel and user side; already in the image |
| Build host | All builds inside `agentic-os-kernel-dev:0.19.0` through `kernel/osdk.sh`, which mounts the repo root at `/work` | `userland/` must be visible to the kernel build (`include_bytes!`, `abi` path dependency) |
| Image format | Flat binary from `objcopy -O binary`, base `0x40_0000`, a 5-quad `.aok_header` first (magic, entry, handler, rx_end, mem_end) emitted by `userland/link.ld` | No ELF parser in the kernel for Phase 1; the header carries what objcopy drops (`.bss` size, RX/RW split) |
| Bootstrap delivery | `include_bytes!` of `userland/out/<bin>.bin` into the kernel. No cpio or initramfs in Phase 1. | User decision 2026-10-24 |
| Syscall ABI | Linux-like registers: `rax` = number, `rdi, rsi, rdx, r10, r8, r9` = args, negative errno in `rax`; one `#[repr(C)] IpcArgs` for multi-value IPC calls. B2 numbers 24/60/202 kept; new calls from `0x100`; numbers never reused; unknown numbers return ENOSYS | User decision 2026-10-24 (accepted as proposed) |
| Shared ABI crate | `userland/abi`: `no_std`, no `unsafe`, used by ring-3 crates and by the kernel through a path dependency | One definition of numbers, errno values, `IpcArgs` and the image header |
| Ring-3 runtime | `userland/libsys`: the only ring-3 crate allowed `unsafe` (the `syscall` asm block, the `_start`/handler `global_asm!`), each block ledgered in `docs/architecture/unsafe-ledger.md` (userland section) | SAFE-01 covers ring-3 code too (user decision 2026-10-24) |
| Test reporting | Ring-3 test programs report through their exit code (0 = pass, n = failing step) and, for data, through IPC to a kernel-side test server | No shared-memory protocol needed for results |
| IPC copy path | Kernel stages message bytes and copies them in each side's own syscall context through `copy_to_user`/`copy_from_user` | `VmSpace` reader/writer only operate on the active space; reusing the CoW-breaking copy keeps one write path into user memory (KERN-06) |
| Directory layout | `userland/{abi,libsys,tests}` cargo workspace; test programs are `userland/tests/src/bin/<name>.rs`; build output `userland/out/` (gitignored) | Servers arrive in Phase 2+ as further workspace members |

## Stack Touched in Phase 1

- [ ] Project scaffold: `userland/` workspace, target JSON, linker script, `build.sh`, Docker mount widening (01-01)
- [ ] Kernel loader: `user::load_image` maps RX text, RW data plus zeroed `.bss`, and a user stack (01-01)
- [ ] Ring-3 runtime: `libsys` syscall stub, `_start`, panic handler (01-01)
- [ ] One real syscall and one real IPC round trip from ring 3 (01-01)
- [ ] CI gates: `tools/unsafe_check.py`, `scripts/check.sh`, `.github/workflows/check.yml` (01-02)
- [ ] Local full run command: `scripts/check.sh` (KVM host); CI runs `scripts/check.sh --no-kvm` when `/dev/kvm` is absent

## Out of Scope (Deferred to Later Slices)

- ELF loading, `goblin`, the exec server (Phase 4)
- cpio/initramfs bootstrap image (Phase 3 bootstrap)
- Process objects, teardown, timers beyond the `block_until` deadline (Phase 2)
- musl, `libaok`, BusyBox, os-test (Phase 6)
- Saved-context signal frames (Phase 2); Phase 1 redirect rewrites `rdi`/`rip` only
- Null-IPC cost ceiling: report-only in Phase 1 (user decision 2026-10-24)

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its decisions:

- Phase 2: Space and Thread capabilities, saved-context signals, leak-free exit, timers
- Phase 3: root-authority bootstrap, `server-rt`, vfs with per-domain namespaces
- Phase 4: static exec and procmgr (fork/spawn/wait)
- Phase 5: signals, sessions and tty
- Phase 6: musl + `libaok` on the syscall ABI fixed here, BusyBox, conformance CI, Linux-relative timings
