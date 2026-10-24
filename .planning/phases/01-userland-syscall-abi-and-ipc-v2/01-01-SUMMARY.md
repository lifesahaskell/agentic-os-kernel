---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 01
subsystem: kernel-userland-abi
tags: [rust, no_std, ring3, osdk, ipc, syscall-abi, flat-image]

requires:
  - phase: B2 spike
    provides: capability microkernel on OSTD 0.19.0 (cspace, endpoints, agents, user-mode run loop)
provides:
  - userland/ cargo workspace (abi, libsys, tests) built inside the Docker image for x86_64-aok-user.json
  - Flat image format and kernel loader (user::load_image, user::start, user_image!)
  - Syscall ABI crate (numbers, errno, IpcArgs) shared by ring 3 and the kernel
  - SYS_CALL (0x100): one ring-3 call/reply IPC round trip with kernel-stamped sender credential
affects: [01-02, 01-03, 01-04, 01-05, 01-06, 01-07, phase-2, phase-3, phase-4]

plan_head_before: 107a6b814ff183445a7e1a700bac66dfc79d1310
plan_head_after: 784c501d5cbd28ae254431dc85ad613d52d32c3f

actuals:
  tokens: 8500
  tasks: 2
  commits: 2

tech-stack:
  added: [custom Rust target x86_64-aok-user.json, -Zbuild-std=core,compiler_builtins, llvm-objcopy flat image]
  patterns:
    - ring-3 programs report through exit code (0 = pass, n = failing step)
    - kernel server tasks in ktests reply through ReplyWait
    - ring-3 unsafe confined to libsys raw.rs and rt.rs

key-files:
  created:
    - userland/Cargo.toml
    - userland/x86_64-aok-user.json
    - userland/link.ld
    - userland/build.sh
    - userland/abi/src/lib.rs
    - userland/libsys/src/lib.rs
    - userland/libsys/src/raw.rs
    - userland/libsys/src/rt.rs
    - userland/tests/src/bin/tracer.rs
  modified:
    - kernel/osdk.sh
    - kernel/Cargo.toml
    - kernel/src/user.rs
    - kernel/src/ipc.rs
    - .gitignore

key-decisions:
  - "abi path dependency (package aok-abi, path ../userland/abi) resolves under OSDK; the #[path] fallback was not needed"
  - "Message loses Copy (now Clone) because it carries an Arc<ReplyWait>; ReplyWait compares by pointer"
  - "sys_call rejects non-zero buffer and capability fields with EINVAL until a later plan implements them"

requirements-completed: [KERN-03, IPC-01]

coverage:
  - id: D1
    description: "Rust no_std ring-3 tracer built for x86_64-aok-user.json loads from an include_bytes! flat image, runs SSE math and a .bss write, gets ENOSYS for an unknown syscall, and exits 0"
    requirement: "KERN-03"
    verification:
      - kind: integration
        ref: "kernel: ./osdk.sh sh -c '../userland/build.sh && cargo osdk test' (aok::user::test::tracer_runs_rust_ring3_image)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Malformed image headers (bad magic, short input, entry beyond rx_end, rx_end beyond mem_end) are rejected without a panic"
    requirement: "KERN-03"
    verification:
      - kind: unit
        ref: "aok::user::test::load_image_rejects_malformed_headers"
        status: pass
    human_judgment: false
  - id: D3
    description: "A ring-3 SYS_CALL on an empty slot returns EBADF, and a call on a real endpoint blocks until a kernel-side server replies; the server observes the kernel-stamped sender credential"
    requirement: "IPC-01"
    verification:
      - kind: integration
        ref: "aok::user::test::tracer_runs_rust_ring3_image (sender == client.agent().credential(), label 7, words[0] 41 -> 42)"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-10-24
status: complete
---

# Phase 1 Plan 01: Walking skeleton Summary

**A Rust no_std ring-3 program loads from a flat image built by a custom SSE-enabled target inside the Docker image, runs SSE and .bss, and completes a `SYS_CALL` round trip to a kernel-side server whose received message carries the kernel-stamped sender credential.**

## Performance

- **Duration:** about 10 min of recorded session time (the session was compacted once)
- **Completed:** 2026-10-24
- **Tasks:** 2
- **Files modified:** 22 in the plan diff (803 insertions, 46 deletions)

## Accomplishments

- `userland/` workspace (`abi`, `libsys`, `tests`) builds through `userland/build.sh` into `userland/out/tracer.bin` (696 bytes, magic `AOKIMG01`); `kernel/osdk.sh` was widened so the kernel build sees it.
- `user::load_image` maps RX text, zero-filled RW data/`.bss`, and 32 stack pages; it rejects malformed headers with `LoadError::{Header, Layout}`.
- `SYS_CALL = 0x100` with the 168-byte `IpcArgs`: `Endpoint::call` queues a `Message` with an `Arc<ReplyWait>` and blocks; `sys_call` reads args through the active `VmSpace` and writes reply words back.
- 29 ktests pass (27 B2 plus 2 new); `cargo fmt --check` and `cargo osdk clippy` are clean; `cargo osdk run` prints `aok: cpu budget check passed`.

## Task Commits

1. **Task 1: tracer image loads, runs SSE and .bss, exits 0** - `61689f1` (feat)
2. **Task 2: ring-3 call/reply IPC round trip with kernel-stamped sender** - `784c501` (feat)

**Plan metadata:** recorded in the docs commit that follows this file.

## Files Created/Modified

- `userland/x86_64-aok-user.json`, `link.ld`, `build.sh`, `.cargo/config.toml`, `rust-toolchain.toml` - target, linker script emitting the 5-quad header, build driver.
- `userland/abi/src/lib.rs` - syscall numbers (24/60/202 kept, 0x100 new), errno, `IpcArgs`, image constants.
- `userland/libsys/src/{lib,raw,rt}.rs` - `syscall6`, `entry!` macro with `_start`/`_aok_handler`, `exit`, `sched_yield`, `call`, panic handler.
- `userland/tests/src/bin/tracer.rs` - SSE, `.bss`, ENOSYS, EBADF probe on slot 63, real call expecting `words[0] == 42`.
- `kernel/src/user.rs` - loader, `start`, `run_ctx`, `user_image!`, `testkit`, `SYS_CALL` dispatch, two ktests.
- `kernel/src/ipc.rs` - `ReplyWait`, `Endpoint::call`, `sys_call`.
- `kernel/osdk.sh`, `kernel/Cargo.toml`, `.gitignore` - mount, `abi` dependency, ignore `userland/target` and `userland/out`.

## Recorded outcomes requested by the plan

- **`abi` path dependency:** worked under OSDK as `abi = { package = "aok-abi", path = "../userland/abi" }`; the `#[path]` fallback was not used.
- **Target JSON keys changed from stock `x86_64-unknown-none`:** `code-model` kernel to small; `features` to `+sse,+sse2` (dropped `-mmx,-sse*,+soft-float`); dropped `rustc-abi` softfloat, `metadata`, `relro-level`, `supported-sanitizers`; `position-independent-executables` and `static-position-independent-executables` to false; added `relocation-model: static`. Kept `rust-lld`, `panic-strategy: abort`, red zone disabled.
- **Tracer image size:** 696 bytes (312 bytes after Task 1, before the IPC calls).
- **Docker mount:** `kernel/osdk.sh` now mounts the repo root at `/work` with `-w /work/kernel` (was the `kernel/` directory), and chowns only `/work/kernel /work/userland`.

## Decisions Made

- `Message` is `Clone` rather than `Copy` because it holds the reply handle; `ReplyWait` equality is pointer equality so existing `Message` assertions keep working.
- `sys_call` returns EINVAL for non-zero `send_buf/send_len/recv_buf/recv_len/ncaps`; buffers and capability transfer are later plans.
- `read_args` decodes `IpcArgs` from bytes with `u64::from_le_bytes`, no transmute, so the kernel stays free of `unsafe`.

## Deviations from Plan

None - plan executed as written. One self-inflicted defect was fixed inside Task 2 before commit: `SYS_CALL` was missing from the `abi` import in `kernel/src/user.rs`, so the match arm compiled as a catch-all binding (rustc warned "unreachable pattern") and routed every syscall into `sys_call`, which killed QEMU silently with exit 0 under `--no-reboot`. Adding the import fixed it; nothing from the broken state was committed.

## Issues Encountered

- QEMU under `--no-reboot` reports exit 0 on a triple fault, so a crashed ktest run looks like a clean exit. Read the `test result:` line, not the exit code.
- `ostd::println!` does not capture inline format arguments; use positional arguments.

## Known Stubs

None. The EINVAL on non-zero buffer/capability fields is a documented, intentional limit of this slice.

## Threat Flags

None. `SYS_CALL` reads and writes user memory only through the fallible `VmSpace` reader/writer and returns EFAULT on failure; the sender credential is stamped from the caller's agent, never from user input.

## Self-Check: PASSED

- Files present: `userland/out/tracer.bin`, `userland/abi/src/lib.rs`, `kernel/src/ipc.rs`, `kernel/src/user.rs`.
- Commits `61689f1` and `784c501` are ancestors of HEAD.
- Acceptance greps: `SYS_CALL` in `kernel/src/user.rs` (2), `pub fn sys_call` (1), `pub struct ReplyWait` (1), tracer contains `42` and the slot-63 probe; `unsafe` grep prints nothing under `userland/tests/src` and lists only `raw.rs` and `rt.rs` under `userland/libsys/src`.
- Not checked: the plan-level verification commands beyond the ktest run, fmt, clippy and `cargo osdk run`; `userland/` clippy was not run.
