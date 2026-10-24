<!-- GSD:project-start source:PROJECT.md -->

## Project

**agentic-os-kernel**

A research effort toward an operating system where LLM agents are kernel objects: principals the
OS itself schedules, accounts for and constrains, on a new Rust capability microkernel built on
Asterinas OSTD. The OS must also run ordinary POSIX software unchanged, through a libc and
userspace servers (procmgr, vfs, exec). It is built by one person plus agents as a research
artifact. Sources: [scoping.md](../docs/architecture/scoping.md),
[option-b-microkernel.md](../docs/architecture/option-b-microkernel.md),
[ADR 0002](../docs/architecture/adr/0002-rust-microkernel.md).

**Core Value:** The protection boundary knows what an agent is: authority is exactly the set of capability
handles an agent holds, and the kernel stamps its identity on every message, while unmodified
POSIX programs still run.

### Constraints

- **Team and schedule**: one person plus agents, no deadline — research artifact, not a product (answers scoping section 9 questions 1 and 5; revisit if either changes)
- **Tech stack**: Rust on Asterinas OSTD 0.19.0, x86-64 only for v1 — the spike validated this base
- **Safety**: `#![deny(unsafe_code)]` stays crate-wide. `unsafe` is allowed only where no safe workaround exists, confined to small named modules, each block ledgered in `docs/architecture/unsafe-ledger.md` and revisited in a later hardening pass (decided 2026-10-24). Needing an OSTD patch, or an `unsafe` need that cannot be contained, is the recorded signal to re-score the Redox fork in ADR 0002
- **Build/test**: Docker image `agentic-os-kernel-dev:0.19.0` and `kernel/osdk.sh`; KVM host required
- **Compatibility**: agent features are additive; they never change the semantics of existing POSIX interfaces, and denials map to standard errno values
- **Docs**: wiki claims cite PDF pages; design reasoning not stated by a source is marked *(interpretation)*; `raw/` is gitignored and never edited

<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->

## Technology Stack

## Decisions at a glance

| Question | Decision | Confidence |
|---|---|---|
| libc | **musl 1.2.6 plus a carried patch series**, targeting a Linux-syscall-shaped shim `__aok_syscall(n, a1..a6)` | MEDIUM-HIGH |
| Syscall shim | `libaok`: Rust `no_std` staticlib linked into every client program; translates Linux syscall numbers to kernel syscalls and server IPC | MEDIUM |
| ELF loading | Exec server in Rust `no_std`, `goblin` 0.10.7 (`elf64`, `endian_fd`, no default features); static ELF only (ET_EXEC and static-PIE), no `ld.so` | MEDIUM-HIGH |
| Server framework | No existing framework fits. A small in-repo `aok-rt` crate: `no_std` + `alloc`, custom target JSON, receive loop over the B2 endpoint | MEDIUM |
| BusyBox | **1.38.0** (13 May 2026), static, `allnoconfig` plus an explicit applet list, `ash` as the shell | HIGH on version, MEDIUM on config |
| Conformance CI | os-test (pinned commit) as headline; Open POSIX Test Suite (LTP `20260930`) and musl libc-test as secondary; host-side harness; per-suite allowlists | MEDIUM |
| Cross-toolchain | Host clang/lld (LLVM 21+) building musl and C programs for `x86_64-linux-musl` against a private sysroot; Rust `nightly-2026-07-21` with a custom x86-64 user target and `-Zbuild-std` | MEDIUM |

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why |
|---|---|---|---|
| musl libc | 1.2.6 plus upstream master fixes (see Pitfall list in the section below) | C library for BusyBox, test suites, and all POSIX clients | Highest published os-test score (94%, `rust-kernels-and-adherence.md` Part 3), so it sets the libc ceiling. relibc's Redox row (84%) loses points in libc-only suites: Redox `namespace` is 21% against Linux 84% (`microkernel-posix.md` section 7). Small, single-purpose syscall layer. |
| `libaok` (new, in-repo) | n/a | Syscall shim: Linux syscall number in, kernel syscall or server IPC out | musl's only OS dependency is the raw `syscall` instruction. Verified today: current musl has no `SYSCALL_NO_INLINE` hook; the instruction appears in `arch/x86_64/syscall_arch.h` (14 hits) and six asm files: `thread/x86_64/{__set_thread_area,__unmapself,clone,syscall_cp}.s`, `signal/x86_64/restore.s`, `process/x86_64/vfork.s`. That is the whole patch surface. Linux-shaped boundary also keeps relibc's Linux backend and Rust `x86_64-unknown-linux-musl` std programs possible later. |
| Rust nightly | `nightly-2026-07-21` (existing kernel pin; local `nightly` is 1.100.0, LLVM 23.1.0) | Servers, `libaok`, exec | One toolchain for kernel and user side. Do not mix in relibc's `nightly-2026-05-24`. |
| Custom Rust target `x86_64-aok-user.json` | n/a | Build `no_std` user programs | Verified today: stock tier-2 `x86_64-unknown-none` is soft-float, `-sse`, `code-model: kernel`, red zone off. Unusable for user code that shares a ABI with C/musl. Start from `rustc -Zunstable-options --print target-spec-json --target x86_64-unknown-none`, then enable `+sse,+sse2`, drop `soft-float`, set `code-model` small, `relocation-model` static, keep `rust-lld` and `panic-strategy: abort`. Build with `-Zbuild-std=core,alloc,compiler_builtins -Zbuild-std-features=compiler-builtins-mem`. |
| BusyBox | 1.38.0 | Shell (`ash`) and core applets | Latest release (busybox.net, 13 May 2026). Static, single multi-call binary, no `/proc` needed with a trimmed config. Moss and Kerla both reached BusyBox with musl (`rust-kernels-and-adherence.md` Part 1). |
| Sortix os-test | no tags exist; pin commit `8e790a45188ac67b550ba62cdde4103b253f5ce8` (2026-10-19, branch `main`) | Headline adherence number | Only current (POSIX.1-2024) free suite; cross-compile documented; ISC. Bump the pin deliberately, because the denominator moves. |
| Open POSIX Test Suite | inside LTP release `20260930`, path `testcases/open_posix_testsuite` | Interface-level secondary suite | Broad (1,753 files under `conformance/interfaces`), per-test standalone, result codes 0/1/2/4/5. Dormant upstream, so the pin will not move. GPL-2.0: fetch at CI time, never vendor into this repo. |
| musl libc-test | pin a commit from `https://repo.or.cz/libc-test.git` at integration time (last commits Aug 2026) | libc-level regression gate | MIT, standalone tests, produces `REPORT`. Tests the libc and shim more than the kernel; use `src/api`, `functional`, `regression`. |

### Supporting Libraries (user side, `no_std`)

| Library | Version | Purpose | When to Use |
|---|---|---|---|
| goblin | 0.10.7 | ELF64 parsing in the exec server | Same crate and feature set `redox-rt` uses (`default-features = false`, `elf64`, `endian_fd`): precedent for exactly this job. If it pulls too much, a 150-line hand parser for `PT_LOAD`, `PT_TLS`, `PT_INTERP` (reject) is acceptable. |
| zerocopy | 0.8.63 | `#[repr(C)]` message and syscall-struct encoding | Wire format for server IPC. Prefer fixed layouts over serde. |
| spin | 0.12.3 | Locks in single-threaded-per-server code that still needs `static` state | Only if a server grows more than one thread. |
| hashbrown | 0.17.1 | `no_std` maps (pid table, fd tables, namespaces) | Servers with `alloc`. |
| heapless | 0.9.3 | Fixed-capacity queues inside `libaok` before the allocator exists | Early boot of a client. |
| talc | 5.1.1 | Global allocator in servers | Updated 2026-09. `linked_list_allocator` 0.10.6 is the simpler fallback; either is fine. Do not hand-roll before measuring. |
| bitflags | 2 | Flag types for open/mmap/signal masks | Same as relibc workspace. |

### Development Tools

| Tool | Purpose | Notes |
|---|---|---|
| clang + lld, LLVM 21 or newer, inside the Docker image | Build musl, BusyBox, test suites | Host here has clang 18 and gcc 13.3, too old to standardise on. The Docker image is where the version is fixed; add the package there, not on the host. |
| GNU make 4.3+ | os-test, libc-test, BusyBox, OPTS builds | os-test supports both GNU and BSD make. |
| `cpio` (newc) | Build initramfs | OSTD exposes the boot initramfs; wire via `[boot]` in `kernel/OSDK.toml`. Confirm the exact key in OSDK 0.19 docs in the first plan; I did not verify it. |
| QEMU (already in image) | Run suites | Existing `-smp 1`, `-m 2G`, serial on stdio. |
| Python 3 (`python3 -I`) | Host harness: split serial output into per-test files, diff against allowlists | No dependencies. |

## What the toolchain looks like end to end

## musl patch series (the whole OS-specific libc surface)

## Conformance harness wiring

## Alternatives Considered

| Category | Recommended | Alternative | Why Not (now) |
|---|---|---|---|
| libc | musl + `libaok` | **relibc** (0.2.5, MIT, `nightly-2026-05-24`) | relibc's platform trait `Pal` has about 120 functions, and the Redox backend leans on Redox's `proc` scheme and `redox-rt` (`fork_impl` asks the kernel's proc scheme for the CoW duplicate). A third backend is a rewrite of both, not a port. Its measured libc-bound scores trail musl. Revisit if the musl patch series exceeds roughly ten patches or the shim stops tracking Linux semantics. Because `libaok` is Linux-shaped, relibc's Linux backend can be tried later at low cost. |
| Syscall interception | libc-side shim | Kernel traps `syscall` and forwards as a fault-IPC to a personality server (gVisor, Starnix, seL4 unknown-syscall style) | Unmodified musl and Linux binaries would run, but servers then need cross-address-space access to client memory, a capability the kernel does not have (B2 friction 1: kernel writes into user memory already skip CoW). Linux binary compatibility is not a B3 goal. Keep as a later option. |
| Libc language | C libc (musl) | Rust-only userspace | The kernel is the safety claim (`#![deny(unsafe_code)]`); userland C does not breach it. Servers are Rust. |
| ELF parsing | goblin 0.10.7 | `object` 0.40.0 | `object` is larger and aimed at read/write of many formats. `xmas-elf` 0.10.0 and `elf` 0.8.0 are less used in this niche. |
| Server framework | in-repo `aok-rt` | Adapt `redox-scheme` / `libredox`; Fuchsia FIDL; seL4 Microkit | All assume their kernel's IPC and object model. Adapting costs more than the roughly 300 lines the receive loop needs. |
| Rust servers on `std` | `no_std` + `alloc` | `std` via custom target and `-Zbuild-std` | procmgr, vfs and exec implement what a `std`/libc port would call. Bootstrap cycle. |
| Cross compiler | clang/lld sysroot | musl-cross-make GCC | Second compiler, no benefit on same-arch builds. |
| Application gate | BusyBox testsuite | bash `run-all`, coreutils `make check` | Later (`option-b-microkernel.md` section 5). They need far more surface. |

## What NOT to Use

| Avoid | Why | Use Instead |
|---|---|---|
| Stock `x86_64-unknown-none` for user code | Soft-float, no SSE, kernel code model (verified today). Output cannot link or share an ABI with musl-compiled C. | Custom `x86_64-aok-user.json` |
| Any crate that inlines `syscall` (`rustix`, `syscalls`, `sc`, `libc` crate on linux targets) in user code | Bypasses the shim | `libaok` entry points only |
| musl 1.2.6 tarball unpatched | Three open CVEs listed on the musl front page today | Pinned post-fix commit |
| Open Group VSX/VSC-PCTS in CI | Results unpublishable, licence expires, 2016 standard (`rust-kernels-and-adherence.md` Part 3) | os-test + OPTS + libc-test |
| Vendoring LTP/OPTS into this repo | GPL-2.0 | Fetch at pinned tag in CI |
| `ld.so` / `dlopen` in B3 | Drags in `PT_INTERP`, shared mappings, TLS descriptors | Static linking |
| BusyBox applets that read `/proc` or `/sys` (`ps`, `top`, `free`, `mount` helpers) | Servers do not provide them | Leave out of `allnoconfig` list |
| `CONFIG_NOMMU=y` in BusyBox | We have an MMU and CoW fork; NOMMU forces `vfork` paths and changes behaviour | `CONFIG_NOMMU=n` |

## BusyBox configuration

## Stack Patterns by Variant

- Re-score relibc with a new `agentic` Pal backend.
- Because `libaok` is Linux-shaped, the relibc Linux backend (which uses `sc` for raw syscalls) is the cheaper experiment than the Redox backend.
- Add the trap-and-forward path as a second front end onto the same `libaok` dispatch table.

## Installation

# Rust side (inside the Docker image; toolchain pinned by rust-toolchain.toml)

# user/ workspace (new), Cargo.toml dependencies

# goblin     = { version = "0.10.7", default-features = false, features = ["elf64", "endian_fd"] }

# zerocopy   = "0.8.63"

# hashbrown  = "0.17.1"

# talc       = "5.1.1"

# bitflags   = "2"

# C side (Dockerfile additions)

## Version Compatibility

| Package A | Compatible With | Notes |
|---|---|---|
| OSTD 0.19.0 | `nightly-2026-07-21` | Existing pin; image `asterinas/osdk-dev:0.19.0-20261012`. Never bump one without the other. |
| relibc 0.2.5 | `nightly-2026-05-24` | Different nightly from the kernel; only matters if relibc is re-scored. |
| goblin 0.10.7 | `redox-rt` uses 0.10 | Same minor line. |
| musl 1.2.6 | os-test pinned commit | Re-run the full allowlist after any musl bump; the denominator and outcomes move. |
| BusyBox 1.38.0 | musl 1.2.6+ | Standard Alpine-style pairing. |

## Gaps and items to verify in the first plans

## Sources

- Repo: `docs/architecture/option-b-microkernel.md`, `docs/architecture/research/rust-kernels-and-adherence.md`, `docs/architecture/research/microkernel-posix.md`, `docs/architecture/b2-spike-report.md`, `kernel/Cargo.toml`, `kernel/rust-toolchain.toml`, `kernel/OSDK.toml`, `kernel/Dockerfile` (HIGH, local).
- musl front page and release history, https://musl.libc.org/ and https://musl.libc.org/releases.html (1.2.6 latest; three CVEs open) (HIGH).
- musl source via cgit, `src/internal/syscall.h` (no `SYSCALL_NO_INLINE`), `arch/x86_64/syscall_arch.h`, six x86_64 asm files, https://git.musl-libc.org/cgit/musl/ (HIGH).
- BusyBox, https://busybox.net/ (1.38.0 dated 13 May 2026) (HIGH).
- os-test README and `misc/` scripts, branch commit list, https://gitlab.com/sortix/os-test (HIGH for pin and usage; MEDIUM for JSON details, not read).
- relibc `Cargo.toml`, `rust-toolchain.toml`, `README.md`, `src/platform/pal/mod.rs`, `redox-rt/Cargo.toml`, https://gitlab.redox-os.org/redox-os/relibc (HIGH).
- crates.io API for goblin, object, xmas-elf, elf, talc, linked_list_allocator, zerocopy, postcard, heapless, rustix, syscalls, sc, spin, hashbrown, queried 2026-10-23 (HIGH).
- Local `rustc +nightly -vV` and `--print target-spec-json --target x86_64-unknown-none` (HIGH).
- OPTS and libc-test versions and licences are from `rust-kernels-and-adherence.md` (MEDIUM; not re-fetched).
- The `research-plan` seam and cache store were not used in this run (context budget); no digests were cached.

<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->

## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->

## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->

## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->

## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-fast` for a trivial task inline, with no subagents and no PLAN.md
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->

## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
