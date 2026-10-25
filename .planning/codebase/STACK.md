# Technology Stack (brief)

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
