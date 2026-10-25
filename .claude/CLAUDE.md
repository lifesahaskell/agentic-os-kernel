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

<!-- GSD:stack-start source:codebase/STACK.md -->

## Technology Stack

- Brief of `.planning/research/STACK.md`: decisions table and avoid-list only. The generator copies this file into `.claude/CLAUDE.md`, so keep it short.
- Full stack, versions, alternatives and sources: `.planning/research/STACK.md`. Read it before pinning or bumping anything.

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
