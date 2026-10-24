# Project Research Summary

**Project:** agentic-os-kernel, milestone B3 (POSIX core: procmgr, vfs, exec, libc, BusyBox shell, os-test CI)
**Domain:** POSIX personality servers on a Rust capability microkernel (Asterinas OSTD 0.19.0) where agents are kernel objects
**Researched:** 2026-10-23
**Confidence:** MEDIUM

## Executive Summary

B3 is a MINIX-3-shaped POSIX personality on top of the validated B2 kernel: single-threaded ring-3 servers (procmgr, vfs, exec, later ttyd) that own all POSIX state, a thin client layer that turns libc calls into synchronous call/deferred-reply IPC, and a kernel that knows only threads, address spaces, endpoints and capabilities. Pids, fds, paths, signal numbers and ELF stay out of the kernel. The B2 kernel is not ready to host this. It has one user syscall set (`exit`, `yield`, `futex`), no reply path, no bulk copy, no Thread or Space capability, no teardown that returns budgets, no timeouts, no idle task, and it cannot identify two processes of the same agent. Roughly half of B3 is kernel work, and that work has to land before any server is worth writing.

The recommended approach: kernel enablers first (syscall ABI, IPC v2 with a process tag, Space and Thread objects, death notification, uncharge, timers, idle task), then vfs with per-domain namespaces built in from day one, then exec and procmgr, then signals/sessions/tty, then the libc port and BusyBox, then the CI gate. Libc is musl behind a Linux-shaped shim (STACK), with every server protocol kept at POSIX-operation granularity so the libc decision stays reversible (ARCHITECTURE, PITFALLS). The agentic differentiators (namespace-as-authority, credential inheritance across fork/exec, budget-to-errno mapping, dual-mode conformance) are cheap only if the namespace and identity design is right in the first vfs build. They cost a rewrite if retrofitted.

The dominant risks are all in the first three phases and are invisible to the existing 27 ktests: a blocking-call cancel protocol that loses data on EINTR, wake/interrupt races, kernel writes into CoW pages (EFAULT on the first `read` after `fork`), per-process leaks that sink the os-test run, and servers that never learn a client died (pipelines hang). The mitigation is the same each time: one shared primitive (`block_until`, `copy_to_user`, `destroy process`) plus a ktest that fails when it is removed. A second class of risk is attribution: the os-test headline is libc-dominated, so report kernel-heavy suites (`process`, `signal`, `pty`, `io`) separately from the start and run a musl-on-Linux control.

## Key Findings

### Recommended Stack

Full detail in `.planning/research/STACK.md`. musl is chosen because it holds the highest published os-test score (94%) and its only OS dependency is the raw `syscall` instruction; the patch surface is `arch/x86_64/syscall_arch.h` plus six asm files. A small in-repo `libaok` staticlib maps Linux syscall numbers to kernel syscalls and server IPC. Servers are Rust `no_std` + `alloc` with an in-repo `aok-rt` receive loop; no existing server framework fits.

**Core technologies:**
- musl (pinned post-CVE-fix commit, not the 1.2.6 tarball): C library for BusyBox and all test suites — highest os-test score, small syscall layer, three open CVEs in 1.2.6 including `regcomp`
- `libaok` (new, Rust `no_std` staticlib): Linux-syscall-to-kernel/IPC shim linked into every client — keeps relibc's Linux backend and Rust-musl programs possible later
- Rust `nightly-2026-07-21` (existing kernel pin): servers, `libaok`, exec — one toolchain for both sides; custom target JSON for user code
- goblin 0.10.7 (`elf64`, `endian_fd`): ELF parsing in the exec server — same crate/features as `redox-rt`; static ELF only, no `ld.so`
- BusyBox 1.38.0: static, `allnoconfig` plus explicit applet list, `ash`, `CONFIG_NOMMU=n`
- os-test (pin `8e790a45188ac67b550ba62cdde4103b253f5ce8`), Open POSIX Test Suite (LTP `20260930`, fetch at CI time, GPL-2.0), musl libc-test: conformance, with per-suite allowlists that only grow
- clang/lld LLVM 21+ in the Docker image, GNU make, `cpio` newc, Python 3 host harness

### Expected Features

Full detail in `.planning/research/FEATURES.md`. Key evidence from reading os-test: `process` is 24 tests, all pgrp/session/zombie/limbo edge cases (a procmgr state-machine exercise); `io` is 55 tests of which 32 are OFD locks and 22 are `open()` flag combinations (cheap if vfs owns OFD objects); `pty` is 29 tests of ctty/foreground-group state; `basic` is libc breadth and must not drive kernel priorities.

**Must have (table stakes):**
- Tier 0 kernel enablers: ring-3 syscall ABI, Space and Thread caps, per-process IPC identity, CoW-safe kernel-to-user writes, idle task, uncharge on teardown, futex/recv timeouts
- Static exec, procmgr (pid, wait, zombie and limbo as explicit states, reparent to init), `posix_spawn`, fork with fd-table copy
- vfs: ramfs from initramfs, per-domain namespace, OFDs, server-held fd tables, pipes, devfs, `*at` family, mode-bit checks
- Signals (`sigaction`, masks, `kill`, `SIGCHLD`, fault-to-signal, EINTR/restart contract), pgrp/session, ctty/termios, then pty
- BusyBox `ash` running scripts and pipelines; `testsuite/runtest` with a triaged expected-fail list; os-test CI; benchmarks (fork+exec, spawn, signal, pipe, null read/write) against Linux in the same QEMU/KVM configuration

**Should have (differentiators):**
- Capability-namespace-aware vfs (names outside the grant are `ENOENT`)
- Credential inheritance across fork/exec/spawn, backed by an adversarial test
- Budget-to-errno mapping (`ENOMEM`, `EAGAIN`, `SIGXCPU`)
- Dual-mode conformance (non-agent vs agent-with-full-grant must score the same)
- Kernel-stamped credential in server audit records; scoped PID namespace; narrow-grant demos of real tools

**Defer (v2+ or later milestones):**
- setuid/setgid, uid-0 bypass, `/proc` and `/sys`, Linux ABI emulation, dynamic linking, disk filesystems, sockets (B4), SysV IPC/AIO/mqueue, SMP, broker or LLM in any syscall path
- fd-as-capability rights masks (leave a rights field in the OFD handle), scoped accounting files, file-backed `mmap`, pthread polish

### Architecture Approach

Full detail in `.planning/research/ARCHITECTURE.md`. The kernel gains a native syscall dispatch (`syscall.rs`), growable CSpace, IPC v2 (synchronous `call`, kernel-held reply handles, deferred reply, cancel, bulk copy with CoW break, capability transfer), `Object::Space` and `Object::Thread`, a per-thread `tid`/`tag` stamped on every message, timers, and a thread-exit notification to a process endpoint. Servers follow an acyclic call graph (client to procmgr to exec to vfs; vfs and exec reach procmgr only by async `send`), and procmgr pushes uid/gid/ctty changes to vfs before replying. Bootstrap is a flat user binary and the only holder of root authority; it wires the servers and drops its caps. A `posix-client` crate is the single adapter between servers and any libc.

**Major components:**
1. Kernel (OSTD 0.19.0, `deny(unsafe_code)`) — threads, spaces, endpoints, caps, budgets, timers; knows no POSIX identity
2. procmgr — pid, ppid, pgrp, session, uid/gid, zombies, wait, fork/exec glue, kill routing, fault-to-signal; owns every user Thread and Space cap
3. vfs (+tmpfs, pipefs, devfs) — per-domain namespaces, per-process fd tables keyed on `(agent_id, tag)`, shared OFDs, mode checks, deferred-reply backends
4. exec — builds a complete fresh Space (segments, stack, argv/envp/auxv) and returns it; procmgr does the swap
5. `libaok` / `posix-client` + musl — the only place POSIX signatures meet Rust types; server protocols stay POSIX-operation granular

### Critical Pitfalls

Full list (28) in `.planning/research/PITFALLS.md`. The five that gate the roadmap:

1. **Cancel without a server-side protocol** — define commit-or-not-started semantics once; interrupt of a blocked `call` is a Cancel to the server, not an abort; replies to dead waiters are dropped by the kernel; servers park requests, never threads.
2. **Hidden wake/interrupt races** — one shared `block_until(condition, deadline)` helper, forbid hand-rolled wait loops, add a fail-point ktest and a 1000x loop in CI.
3. **Kernel writes into CoW pages** — one `copy_to_user` that breaks CoW and retries; write the failing post-clone IPC ktest first, before procmgr exists.
4. **Teardown leaks and no death notification** — "exit releases everything" is a Phase 3 exit criterion with a 10,000x fork+exit leak test; the kernel notifies procmgr, which fans out to vfs and ttyd; test pipe EOF on SIGKILL of the writer.
5. **Session/ctty/pty state split across servers** — write the ownership table (procmgr owns pgrp, session, ctty link; ttyd owns tty-to-session label) before any code; run os-test `pty` from the first pty build (Redox 6%, MINIX `process` 16% are this failure).

Also on the critical path: signal redirect must preserve full user state (including FPU), and user-mode FPU/SSE enablement under OSTD is the highest-risk unknown in the stack (no musl binary runs without it).

## Implications for Roadmap

Eight phases, following ARCHITECTURE's A-I decomposition (the only file with a dependency-derived order), adjusted for PITFALLS and FEATURES. Phases 1-3 are risky kernel work; 4-6 are server work with known designs. The roadmapper may coarsen to 6 by merging 1+2 and 7+8.

### Phase 1: Userland foundation and syscall ABI
**Rationale:** First Rust ring-3 binary de-risks the Docker image, target spec, flat-image mapping, and the ABI before any design depends on them; retires hand-assembled test programs.
**Delivers:** `userland/` workspace, custom user target, `syscall.rs` dispatch with a typed decode layer and a fuzz-style ktest, flat bootstrap image, Rust ring-3 test crate, Dockerfile additions. **Settles the FPU/SSE question** (enable and save per-thread state, or stop).
**Addresses:** Tier 0 syscall ABI.
**Avoids:** Pitfalls 23, 24, 26; the FPU unknown (STACK gap 1, Pitfall 3).

### Phase 2: IPC v2
**Rationale:** call/reply is the mechanism every server uses and its cost decides whether the layered design is viable; measure the round trip the day it exists.
**Delivers:** tid/tag stamped messages, `call`/`recv`/`reply`/`reply_recv`/cancel, bulk copy with CoW break (`copy_to_user`), capability transfer, growable CSpace, the shared `block_until(deadline)` helper, a bulk-capable message format, null-IPC round-trip timing.
**Addresses:** per-process identity, kernel-write-CoW fix, futex/recv timeouts (K2, K3, K10).
**Avoids:** Pitfalls 1, 2, 4, 15, 18 (format).

### Phase 3: Space and Thread objects, teardown, timers
**Rationale:** idle task and uncharge are correctness conditions for long-running servers, not polish; thread/space caps are what procmgr is made of.
**Delivers:** `Object::Space`, `Object::Thread`, `thread_create(space, ctx, tag)`, saved-context redirect plus `sigreturn`, thread-exit/exception notification naming tid and tag, destroy-process with uncharge and futex-waiter wakeup, idle task that halts, timer-to-message, server CPU-exemption decision plus request ledger, scheduler-protocol ktests for OSTD assumptions, OSTD pinned.
**Addresses:** Tier 0 remainder (K4-K9).
**Avoids:** Pitfalls 3 (contract), 5, 6 (primitive), 10, 14 (primitive), 16, 20, 21.

### Phase 4: Bootstrap, server runtime, and vfs
**Rationale:** vfs has no server dependencies, is testable alone with a Rust client, is where the namespace-as-authority design must be right from the first build, and exec needs it to read files.
**Delivers:** `elf` crate, `server-rt`, bootstrap wiring and cap distribution, initramfs/cpio source, vfs with tmpfs/pipefs/devfs, per-domain namespaces, OFDs, server-held fd tables, `*at` calls, mode checks, deferred-reply pipes, rights field reserved in the OFD handle, host-side unit tests for path resolution.
**Uses:** goblin, zerocopy, hashbrown, talc.
**Addresses:** Tier 2 vfs, namespace differentiator, OFD locks groundwork.
**Avoids:** Pitfalls 1 (pipes), 6 (EOF on writer death acceptance), 11 (badge scheme), 18 (measure pipes here).

### Phase 5: exec and procmgr core
**Rationale:** needs Phases 3 and 4. Build the process state machine as a pure host-testable library with a model-based test.
**Delivers:** exec server, procmgr (pid table, `posix_spawn`, fork as reserve-then-commit, exec with failure rollback, exit, wait, reparent to init, zombie and limbo states, `setpgid`/`setsid` model), `posix-client` crate, the os-test host harness skeleton (serial runner, watchdog, hang outcome), primitive timings (IPC, `cow_clone`) and the fork+exec predicted sum, domain A/B isolation test, **the libc decision spike and ADR at the end of this phase**.
**Addresses:** Tier 1, credential inheritance, early benchmarks.
**Avoids:** Pitfalls 7 (ownership table written here), 8, 9, 11, 12 (skeleton), 13, 14, 19.

### Phase 6: Signals, sessions, and tty
**Rationale:** needs procmgr. The EINTR/restart contract is retrofit-expensive, and ctty/pty is the lowest-scoring area of every comparison OS.
**Delivers:** `kill`, `sigaction`/masks, libc trampoline entry, fault-to-signal, `SIGCHLD`/`SIGPIPE`/`SIGALRM`, `sigaltstack`, `ppoll`, pgrp/session/ctty, ttyd line discipline and termios, then pty. os-test `process`, `signal`, `pty` wired into CI the day they first run.
**Avoids:** Pitfalls 1, 3, 7, 8, 22.

### Phase 7: libc port and BusyBox
**Rationale:** nothing in Phases 1-6 depends on the libc choice; this phase needs only the ADR from Phase 5.
**Delivers:** musl patch series (carried on a pinned post-CVE commit), `libaok`, thread-creation/FS-base/exit-and-unmap kernel calls if pthreads are in scope, atfork hooks, static BusyBox `ash` with committed `.config`, `ash -c 'ls | grep x > /tmp/y'`, then interactive job control, BusyBox `testsuite/runtest` with triaged expected-fail list.
**Avoids:** Pitfalls 9 (atfork, `posix_spawn` mapping), 13, 27.

### Phase 8: Conformance gate, benchmarks, authority audit
**Delivers:** per-suite allowlists gated on all outcomes (including hang and neutral), musl-on-Linux control row, kernel-heavy vs libc-bound reporting, OFD locks and remaining `io` score, pthreads, libc breadth, benchmarks versus a Linux guest in the same QEMU/KVM configuration (rdtsc, median, p99, N, pre-declared "layer is wrong" threshold), dual-mode run, adversarial credential-inheritance test, optional `-smp 2` probe.
**Avoids:** Pitfalls 12, 17, 19, 25.

### Phase Ordering Rationale

- Dependencies force 1 to 2 to 3 to 4 to 5 to 6 to 7. After Phase 3, vfs and the `elf`/exec parsing work are independent and can run in parallel.
- Namespaces and identity are built into Phase 4 and the Phase 2 tag, because the differentiators and Pitfall 11 become rewrites if retrofitted.
- Process-state logic (wait, pgrp, session) is a pure library with host tests, which sidesteps the Docker/QEMU loop and addresses the Redox `waitpid` deadlock class.
- The os-test harness skeleton precedes the 4,500 tests (Phase 5), and `pty` tests run from the first pty build.
- Measurement is staged: IPC round trip (Phase 2), pipe (Phase 4), fork+exec predicted vs measured (Phase 5), publication (Phase 8).

### Research Flags

Needs `/gsd-plan-phase --research-phase <N>` during planning:
- **Phase 1:** FPU/SSE user state under OSTD 0.19.0; OSDK initramfs key and `boot_info()` contents (read the 0.19.0 source; docs.rs build failed for 0.19.0); custom target flags; LLVM 21 availability in the image.
- **Phase 2:** spike required. Frame-level cross-space copy and CoW-break order were inferred, never run; cancel-then-wait protocol has no tested precedent here.
- **Phase 3:** spike required. `thread_create` from a syscall onto an OSTD `Task`, scheduler protocol reverse-engineered from source, idle task vs `pick_next`.
- **Phase 6:** deeper research. Multithread signal routing, `SA_RESTART` interplay with Cancel, async-signal-safety of the entry stub, ctty/job-control ownership.
- **Phase 7:** musl asm patch details against the chosen pin; `clone`/`CLONE_CHILD_CLEARTID`/`set_tid_address` mapping; OPTS build on musl headers.

Standard patterns (skip research-phase):
- **Phase 4:** MINIX VFS, QNX, Redox cover it; the namespace design is documented in the repo.
- **Phase 5:** MINIX PM, Redox procmgr patterns; the libc spike is a bounded task, not a research phase.
- **Phase 8:** harness and allowlist mechanism are specified in STACK; os-test JSON schema to be read in-phase.

## Contradictions Between Research Files

These were not silently resolved. The recommendation column is this synthesis's opinion; each needs a decision before or at the named phase.

| # | Topic | What each file says | Recommendation | Decide by |
|---|-------|---------------------|----------------|-----------|
| 1 | **Libc timing** | STACK: musl + `libaok`, decided. FEATURES and PITFALLS: open, no evidence, spike both (FEATURES: 2-day `hello`+`fork`+`read` on each). ARCHITECTURE: decide at start of Phase H. | Treat musl as the default and the spike as a confirmation, not a bakeoff. STACK is the only file with source-level evidence (patch surface, scores). Record as an ADR at the end of Phase 5; keep the spike's go/no-go criteria from PITFALLS 13 (fd-table placement, `clone` mapping, `pthread_join` on futex). | Phase 5 end |
| 2 | **Shim vs protocol granularity** | STACK: libc boundary is Linux-syscall-shaped (`__aok_syscall`). PITFALLS 13: a server request named after a Linux syscall is a warning sign. ARCHITECTURE: `proto` is ours, `posix-client` is the single adapter. | These reconcile only if `libaok` sits on top of `posix-client` and servers speak POSIX-operation protocols. Make that explicit: Linux syscall numbers stop at `libaok`. | Phase 2 (proto design) |
| 3 | **Process identity on the wire** | ARCHITECTURE: kernel-stamped `tid` + `tag` per thread, set at `thread_create` by the Space-cap holder; vfs keys on `(agent_id, tag)`. FEATURES: badge on derive or per-process session endpoint, open. PITFALLS 11: per-client badged session endpoints (seL4 style). | Adopt ARCHITECTURE's tag; it is the only concrete design and keeps fork inside the agent cspace. Use badged endpoints only for server-to-server delegation (vfs to ttyd) where a tag does not carry over. Record both uses so they are not read as competing. | Phase 2 |
| 4 | **Signal frame ownership** | ARCHITECTURE (Pattern 5) and STACK (patch 4): kernel keeps a saved-context stack, `sigreturn` pops it; no kernel write to user memory. PITFALLS 3: libc trampoline pushes the full context incl. xsave; kernel only jumps. | Take ARCHITECTURE's kernel-held saved-context stack (avoids user-stack writes and CoW), with FPU state saved in that kernel context. PITFALLS' register-survival ktest applies unchanged. Revisit if FPU save in the kernel context is not feasible in OSTD. | Phase 3 |
| 5 | **Cancellation of blocked calls** | ARCHITECTURE Pattern 4 and PITFALLS 1: interrupt of a blocked `call` is a Cancel; caller stays blocked until the server replies. STACK patch 3: rely on "the B2 interrupt-aborts-blocked-IPC path (EINTR)" for cancellation points. | STACK's wording is stale against the other two. Use Cancel semantics; the abort-at-once behaviour stays only for a blocked `recv`. STACK patch 3 should be read as "pthread cancellation points get EINTR from the server's reply". | Phase 2 |
| 6 | **Per-process memory authority** | ARCHITECTURE: self-memory ops (`mmap`, `brk`, `mprotect`) are cap-less syscalls on the caller's own space; Anti-Pattern 1 forbids per-process Space caps in the shared cspace. PITFALLS 21: give each process a self address-space cap with restricted rights. | Take ARCHITECTURE. A self-cap in the shared agent cspace is usable by siblings. Pitfall 21's real concern (do not route memory calls through procmgr) is met by the cap-less syscalls. | Phase 3 |
| 7 | **User-side target** | ARCHITECTURE: `userland/` built for stock `x86_64-unknown-none`. STACK (verified locally): that target is soft-float, no SSE, kernel code model, red zone off, and cannot share an ABI with musl C. | Use STACK's custom `x86_64-aok-user.json` for all user code from Phase 1, including servers (servers calling `libaok`/shared structs need one ABI). Update ARCHITECTURE's Dockerfile note accordingly. | Phase 1 |
| 8 | **Phase order, vfs vs procmgr/exec** | FEATURES MVP: exec + procmgr + spawn + minimal libc first, vfs third. ARCHITECTURE: vfs (E) before exec/procmgr (F); exec reads ELF through a vfs file cap. PITFALLS: P1 kernel, P2 procmgr+exec+signals, P3 vfs+pty. | ARCHITECTURE's order, because exec has a hard dependency on vfs there. FEATURES' "spawn early" intent is kept by landing `posix_spawn` before fork inside Phase 5. | Roadmap |
| 9 | **Phase placement of signals** | PITFALLS puts signals with procmgr (P2); ARCHITECTURE and FEATURES put them after (G / Tier 3). | Separate Phase 6, but the redirect/sigreturn kernel primitive and the Cancel protocol land in Phases 2-3 so Phase 5 never blocks on them. | Roadmap |
| 10 | **Libc in the roadmap** | FEATURES Tier 1 lists a minimal libc in "boot a static binary"; ARCHITECTURE puts libc in H (after servers, tested with Rust clients); PITFALLS puts it in P4. | Servers and early tests use `posix-client` and Rust ring-3 binaries; no C libc until Phase 7. FEATURES' "minimal libc" is a Rust test runtime, not musl. | Roadmap |
| 11 | **`posix_spawn` and `vfork`** | FEATURES: `posix_spawn` first-class procmgr op (cheap path), BusyBox NOMMU uses vfork-style spawn. STACK: `vfork` aliased to fork semantics; `CONFIG_NOMMU=n`. ARCHITECTURE: no `posix_spawn` at all. PITFALLS 9: musl's `posix_spawn` uses `clone(CLONE_VM\|CLONE_VFORK)` internally, unverified. | Add `SPAWN` to procmgr's protocol in Phase 5; keep NOMMU off; have `libaok` map musl's `posix_spawn` to it. Verify the `clone` use in the chosen musl pin. Benchmark both fork+exec and spawn and report both. | Phase 5 / 7 |
| 12 | **Cross-domain `kill` errno, read-only mount errno** | option-b text (quoted in FEATURES, ARCHITECTURE): `EPERM` for cross-domain kill, `EACCES` for read-only writes. FEATURES and PITFALLS 11: `ESRCH` leaks nothing; `EROFS` is POSIX. | `ESRCH` and `EROFS`. The option-b text should be amended via ADR. Needs Corey's sign-off since it changes a documented decision. | Phase 4 (vfs), Phase 5 (kill) |
| 13 | **Exec swap primitive** | ARCHITECTURE: exec builds a new Space, procmgr kills old threads and creates a new thread with the same tag; no in-place swap. PITFALLS 14: a kernel `swap_into(thread)` primitive in P1. | ARCHITECTURE's kill-and-create is simpler and needs no new kernel verb, but the "failed exec leaves the old image running" and "signal arrives mid-swap" tests from PITFALLS 14 must be written against it. Re-open only if those tests expose a race. | Phase 5 |
| 14 | **Where the os-test harness starts** | PITFALLS: skeleton in P2 (before many tests). STACK: ash must run first, a hard CI prerequisite. FEATURES and ARCHITECTURE: CI at the end. | Skeleton (framed serial records, host watchdog, hang outcome) in Phase 5 with Rust test binaries; real suites need ash from Phase 7. Both are right for different parts. | Phase 5 / 7 |

Consistent across all four files (no action): vfs-held fd tables for B3; acyclic server call graph with procmgr pushing credentials to vfs; static linking only; no setuid, no `/proc`, no SMP, no broker in any syscall path; servers CPU-exempt with a request ledger and a documented limit; pin OSTD 0.19.0 and the toolchain for all of B3.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM-HIGH | Versions read live today (crates.io, musl, busybox.net, GitLab). musl patch surface grepped from cgit. Design choices (`libaok`, `aok-rt`) are interpretation; nothing was built. OSDK initramfs key, os-test JSON schema, OPTS build, and LLVM 21 availability unverified. |
| Features | MEDIUM | os-test test content read from a clone (HIGH); B2 gaps read from code (HIGH); BusyBox/ash needs from prior knowledge, not re-verified (MEDIUM); all complexity estimates are judgement (LOW). |
| Architecture | MEDIUM | Gap list against `kernel/src` is HIGH. Server design follows MINIX/QNX/Redox and is interpretation for this kernel. OSTD console and boot-module APIs are LOW (docs.rs failed for 0.19.0). CoW-break-on-copy and cancel protocol never exercised. |
| Pitfalls | MEDIUM | Repo-sourced pitfalls are HIGH; cross-system patterns are MEDIUM (a few web checks); musl `clone` details, `VmSpace::activate` TLB behaviour, OSTD FPU enablement, runner KVM availability are LOW. |

**Overall confidence:** MEDIUM

### Gaps to Address

- **User-mode FPU/SSE under OSTD 0.19.0:** highest-risk unknown; every musl binary and every signal frame depends on it. Resolve in Phase 1 before anything else; stop if it fails.
- **Thread creation, set-FS-base, exit-and-unmap syscalls:** do not exist; `libaok` and musl startup (`__set_thread_area`) need them even for single-threaded programs. Scope them in Phase 3.
- **Cap transfer in IPC** (exec's file cap): if it slips, the fallback is a deputy-shaped `OPEN_FOR` accepted only from exec's credential; label it `shortcut:` and keep out of the final design.
- **OSTD APIs for initramfs and serial RX** (`boot_info()`, `arch::serial`, interrupt-driven RX): read 0.19.0 source in Phase 1; fallback is `include_bytes!` of a cpio.
- **Whether `VmSpace::activate` preserves TLB entries:** affects IPC and pipe cost; measure in Phase 2.
- **Server CPU accounting:** B3 treats servers as budget-exempt with a ledger; the core-value statement must not claim kernel-enforced CPU limits cover server-side work. Needs wording in PROJECT.md.
- **Latency pass/fail threshold:** none set. Per the project's rigor rule, pick it when Phase 2 and 5 primitive timings land, before the first fork+exec run.
- **KVM on the CI host:** assumed, not checked.
- **Quantitative targets:** no os-test target tier is set for B3 (B5 owns it); B3 only measures. Allowlist baseline is whatever the first full run produces.

## Sources

### Primary (HIGH confidence)
- Repo: `kernel/src/*`, `kernel/Cargo.toml`, `rust-toolchain.toml`, `OSDK.toml`, `Dockerfile`, `docs/architecture/b2-spike-report.md`, `option-b-microkernel.md`, `research/microkernel-posix.md`, `research/rust-kernels-and-adherence.md`, `.planning/PROJECT.md`
- musl.libc.org front page, releases, and cgit source (syscall surface, three open CVEs)
- busybox.net (1.38.0, 13 May 2026)
- Sortix os-test source and README (gitlab.com/sortix/os-test), test names and counts per suite
- crates.io API (goblin, zerocopy, hashbrown, talc, etc.), local `rustc +nightly` target-spec output

### Secondary (MEDIUM confidence)
- sortix.org/os-test results page (per-suite scores; comparison tool, not certification)
- Redox POSIX conformance write-up; relibc commits ("Fix waitpid deadlock", allocator locking PR 986)
- Asterinas `RELEASES.md` and release dates via `gh api`
- relibc `Cargo.toml`, `Pal` trait, `redox-rt`
- Hurd performance page (fork cost warning, no numbers)

### Tertiary (LOW confidence, needs validation)
- OSTD 0.18.1 docs for `boot_info` and `arch::serial` (0.19.0 docs.rs build failed)
- OPTS and libc-test versions and licences (carried from repo research, not re-fetched)
- BusyBox/ash runtime needs (prior knowledge, not re-verified against source)
- musl `clone`/`CLONE_CHILD_CLEARTID`/`set_tid_address` use; user-mode FPU enablement; runner KVM availability

---
*Research completed: 2026-10-23*
*Ready for roadmap: yes*
