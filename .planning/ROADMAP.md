# Roadmap: agentic-os-kernel

## Overview

Milestone B3 (POSIX core) turns the validated B2 kernel into a system that runs unmodified POSIX
software while an agent stays a kernel object. Roughly half the work is kernel: the ring-3
syscall ABI and IPC v2 (Phase 1), then process objects, teardown and timers (Phase 2). The POSIX
servers follow in dependency order: vfs with per-domain namespaces built in from the first
version (Phase 3), exec and procmgr (Phase 4), signals, sessions and tty (Phase 5). Phase 6 ports
musl, runs BusyBox `ash`, wires the os-test gate and publishes timings against Linux.

**Structure.** 73 requirements in 6 phases (config: coarse). The research proposed 8.
- Research phases 1 and 2 merge into Phase 1. The syscall ABI phase alone holds 3 requirements
  and IPC v2 cannot start without it. The FPU/SSE stop/go gate is its first criterion, so the
  merge costs no decision point.
- Research phases 7 and 8 merge into Phase 6. This is the judgement call. os-test first runs
  when a musl `hello` runs, and BusyBox `ash` is a CI prerequisite for the suites, so the gate
  starts in the libc phase's first plans anyway. Cost: Phase 6 is the largest (16). If planning
  finds it too big, split after SHELL-04 (libc + shell | conformance, timings, audit).
- The other five research boundaries stay: each is a hard dependency or a distinct risk profile.

**Mapping rule.** A requirement maps to the phase that builds its primary component. Legs that
depend on a later phase are separate requirements with their own criteria there (for example,
VFS-06 pipes live in Phase 3; the `SIGPIPE` leg is SIG-03 in Phase 5 and the writer-death leg is
PROC-07 in Phase 4).

**Test-binary rule.** There is no C libc until Phase 6. Phases 1-5 are verified with Rust
ring-3 test binaries. Where a requirement names an os-test suite (`process`, `signal`, `pty`,
`io`), the earlier phase verifies a Rust port of the cases and Phase 6 records the real C suite.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Userland, syscall ABI and IPC v2** - Rust ring-3 toolchain, FPU gate, fuzz-safe syscalls, call/reply IPC with stamped identity
- [ ] **Phase 2: Process objects, teardown and timers** - Space and Thread capabilities, saved-context signals, leak-free exit, idle task, timers
- [ ] **Phase 3: Bootstrap, server runtime and vfs** - Root-authority bootstrap, `server-rt`, vfs with per-domain namespaces, OFDs, pipes
- [ ] **Phase 4: exec and procmgr** - Static exec, fork/spawn/wait, process state machine, credential inheritance, libc ADR
- [ ] **Phase 5: Signals, sessions and tty** - Signal delivery and EINTR contract, fault-to-signal, job control, ttyd, pty, locks, poll
- [ ] **Phase 6: libc, BusyBox, conformance and measurement** - musl port, `ash`, os-test CI gate, Linux-relative timings, agentic audit

## Phase Details

### Phase 1: Userland, syscall ABI and IPC v2

**Goal:** As a server author, I want a Rust ring-3 toolchain, a fuzz-safe syscall ABI and call/reply IPC, so that servers can run and the IPC cost is known.
**Mode:** mvp
**Depends on:** Nothing (first phase; builds on the validated B2 kernel)
**Requirements:** KERN-01, SAFE-01, KERN-02, KERN-03, KERN-05, KERN-06, KERN-12, IPC-01, IPC-02, IPC-03, IPC-04, IPC-05, IPC-06, TEST-06
**Success Criteria** (what must be TRUE):
  1. A Rust `no_std` ring-3 test crate, built for `x86_64-aok-user.json` inside the Docker image and loaded from the flat bootstrap image, replaces the hand-assembled B2 test programs. The B2 ktests and the boot-time budget check stay green, and `cargo clippy` and `cargo fmt --check` pass in CI.
  2. A ring-3 program keeps an XMM value across a yield. Order of attempts: a safe OSTD route, then a contained `unsafe` block recorded in `docs/architecture/unsafe-ledger.md` (SAFE-01). Only if both fail, or the fix needs an OSTD patch, the milestone stops and the ADR 0002 Redox re-score is triggered. The route taken is recorded.
  3. A fuzz-style ktest feeds malformed syscall numbers, pointers and lengths and gets errno returns, never a kernel panic.
  4. A server parks a request and replies later. Interrupting a blocked `call` sends Cancel and the caller stays blocked until the reply, so an interrupted `read` loses no data. A fail-point ktest forces the wake-versus-interrupt interleaving through the shared `block_until`, and a 1000x loop passes.
  5. A server tells two forked siblings apart by the kernel-stamped `tid` and `tag`. A multi-page cross-space copy matches byte for byte, including the first write into a CoW page after clone. A capability sent in a message cannot exceed its attenuated rights. More than 64 capabilities install and derive. The null-IPC round trip is recorded with N, median and p99.

**Research:** research-phase AND spike. Read the OSTD 0.19.0 source (docs.rs failed for 0.19.0): user-mode FPU/SSE enablement and per-thread save (highest-risk unknown; a musl binary cannot run without it), OSDK initramfs key and `boot_info()` contents, serial RX, custom target flags, LLVM 21 in the image. Spike: frame-level cross-space copy and CoW-break order (inferred, never run), the cancel-then-wait protocol (no tested precedent here), whether `VmSpace::activate` keeps TLB entries.
**Decisions due:** `libaok` sits on `posix-client`; Linux syscall numbers stop at `libaok` and server protocols are POSIX-operation granular. Per-thread `tid`/`tag` is the process identity, badged endpoints only for server-to-server delegation. Cancel semantics replace the older "interrupt aborts blocked IPC" wording; abort-at-once stays for a blocked `recv`. One custom user target JSON for all user code.
**Plans:** 1/7 plans executed

Plans:
**Wave 1**
- [x] 01-01-PLAN.md — Tracer: Rust ring-3 image through the real syscall entry, then ring-3 call/reply to a kernel server (W1)

**Wave 2** *(blocked on Wave 1 completion)*
- [ ] 01-02-PLAN.md — Unsafe ledger + checker, scripts/check.sh and CI, FPU/SSE spike with route recorded (W2)

**Wave 3** *(blocked on Wave 2 completion)*
- [ ] 01-03-PLAN.md — Typed syscall decode, copy_to_user/copy_from_user with CoW break, syscall fuzz ktest (W3)
- [ ] 01-04-PLAN.md — Growable CSpace past 64 slots, bounded by MAX_SLOTS (W3, parallel with 01-03)

**Wave 4** *(blocked on Wave 3 completion)*
- [ ] 01-05-PLAN.md — block_until with fail-points, reply handles and reply_recv, tid/tag stamping, bulk copy (W4)

**Wave 5** *(blocked on Wave 4 completion)*
- [ ] 01-06-PLAN.md — Cancel-then-wait and capability transfer with attenuation (W5)

**Wave 6** *(blocked on Wave 5 completion)*
- [ ] 01-07-PLAN.md — Port B2 byte programs to Rust bins, null-IPC measurement, full gate green (W6)

### Phase 2: Process objects, teardown and timers

**Goal:** As the procmgr author, I want Space and Thread capabilities, full process teardown and timers, so that processes can be created, signalled and destroyed without leaks.
**Mode:** mvp
**Depends on:** Phase 1
**Requirements:** KERN-04, KERN-07, KERN-08, KERN-09, KERN-10, KERN-11, AGNT-02, AGNT-05
**Success Criteria** (what must be TRUE):
  1. A holder of a Space capability creates a thread in another space with `thread_create(space, ctx, tag)` and interrupts it. A non-holder is refused.
  2. A fault or thread exit in one of two forked siblings reaches the process endpoint naming the faulting `tid` and `tag`. A signal redirect saves the full context on a kernel-held stack, and `sigreturn` restores it: all general registers and FPU state survive, and the kernel writes nothing to the user stack.
  3. A 10,000x fork+exit loop leaves page and capability counts flat, and a dead process's futex waiters are woken.
  4. With every agent blocked the CPU halts instead of spinning. Timed futex waits and `recv` timeouts expire within a stated tolerance, and a timer delivers a message to an endpoint.
  5. A ring-3 `mmap` past the agent's memory budget returns `ENOMEM`. The server CPU-exemption decision is recorded with a per-client request ledger and a documented limit, and the core-value wording in PROJECT.md does not claim kernel-enforced CPU limits cover server-side work.

**Research:** spike required. `thread_create` from a syscall onto an OSTD `Task`, the scheduler protocol (reverse-engineered from source), idle task versus `pick_next`. Add scheduler-protocol ktests and pin OSTD and the toolchain, since each OSTD upgrade is a deliberate task.
**Decisions due:** Saved-context stack holds FPU state (revisit only if OSTD cannot). Self-memory calls (`mmap`, `brk`, `mprotect`) are cap-less syscalls on the caller's own space, not routed through procmgr. Scope the kernel calls LIBC-04 needs (set-FS-base, thread create for musl start-up, exit-and-unmap) here so Phase 6 does not reopen the kernel; they are verified under LIBC-04.
**Plans:** TBD

### Phase 3: Bootstrap, server runtime and vfs

**Goal:** As an agent developer, I want bootstrap, a server runtime and a vfs with per-domain namespaces, so that a name outside my grant resolves to ENOENT.
**Mode:** mvp
**Depends on:** Phase 2
**Requirements:** SRV-01, SRV-02, SRV-03, SRV-04, VFS-01, VFS-02, VFS-03, VFS-04, VFS-05, VFS-06, TEST-07, AGNT-03
**Success Criteria** (what must be TRUE):
  1. Bootstrap is the only holder of root authority. It loads vfs from the flat image through the shared `elf` crate, hands out capabilities to the servers that exist so far and a Rust test client (procmgr and exec join in Phase 4), then drops its own. The initramfs reaches vfs at boot.
  2. A Rust test client exercises ramfs, tmpfs and devfs (`/dev/null`, `/dev/zero`, `/dev/tty`, `/dev/console`): the `*at` family, all 22 `open` flag combinations, `O_CLOEXEC`, `fcntl` basics, `stat`, `readdir`, `mkdir`, `unlink`, `rename`, `link`, `symlink`, `readlink`, `truncate`.
  3. Domain A cannot name domain B's files; a name outside the grant gives `ENOENT`. A read-only grant fails with the errno recorded in the vfs ADR. Mode bits are checked against the uid/gid the test client pushes. Offset sharing holds across a simulated `fork`, `dup` and `dup2` through shared open file descriptions.
  4. A pipe reader blocks until data arrives, `O_NONBLOCK` returns `EAGAIN`, a write to a closed read end returns `EPIPE`, and EOF follows the last writer's close, all by deferred reply without stalling vfs. Pipe latency and throughput are recorded when pipes land.
  5. Path-resolution unit tests run on the host without QEMU. Every server audit record carries the kernel-stamped credential, and a client-supplied one is ignored.

**Research:** standard patterns (MINIX VFS, QNX, Redox; the namespace design is in the repo). Uses the OSTD initramfs and serial findings from Phase 1; fallback is `include_bytes!` of a cpio.
**Decisions due:** Read-only grant errno (`EROFS`, the POSIX errno, versus `EACCES` in option-b section 7) and cross-domain `kill` errno (`ESRCH` versus `EPERM`, used in Phase 5). Needs Corey's sign-off and one ADR amending option-b.
**Plans:** TBD

### Phase 4: exec and procmgr

**Goal:** As a POSIX program author, I want fork, exec, posix_spawn and wait, so that process trees run with exactly the authority they started with.
**Mode:** mvp
**Depends on:** Phase 2, Phase 3
**Requirements:** PROC-01, PROC-02, PROC-03, PROC-04, PROC-05, PROC-06, PROC-07, EXEC-01, EXEC-02, EXEC-03, LIBC-01, TEST-01, PERF-02
**Success Criteria** (what must be TRUE):
  1. A Rust ring-3 client runs `fork`, `exec` of a static ELF read through a vfs file capability, `_exit`, and `wait`/`waitpid(WNOHANG)`. `posix_spawn` and fork+exec produce the same child state.
  2. Failures leave no partial state. `fork` under an exhausted budget returns `EAGAIN` or `ENOMEM` and leaks nothing. A failed `exec` leaves the old image running. A malformed or dynamic ELF fails with the right errno. A signal arriving mid-swap is delivered to exactly one image.
  3. The process state machine (zombie, limbo, reparent-to-init, pgrp, session) is a pure library with a host model-based test. Rust ports of the os-test `process` cases for `setpgid`, `setsid`, `getpgid` and `getsid` pass, against an ownership table (procmgr owns pgrp, session and the ctty link; ttyd owns the tty-to-session label) written before the code.
  4. A pipe reader sees EOF when its writer is killed, through kernel-to-procmgr-to-vfs fan-out. Credentials inherit across `fork`, `exec` and `posix_spawn`, and an adversarial exec of a file in a wider namespace gains no capability.
  5. The host harness runs Rust test binaries in QEMU over serial with framed records, a watchdog and a distinct hang outcome. The "layer is wrong" threshold and the predicted fork+exec sum are published from the IPC and `cow_clone` timings before the first fork+exec run. The libc ADR is recorded from the bounded spike (`hello`, `fork`, `read`).

**Research:** standard patterns (MINIX PM, Redox procmgr). The libc spike is a bounded task with go/no-go criteria (fd-table placement, `clone` mapping, `pthread_join` on futex), not a research phase. Watch pitfalls 7, 8, 9, 11, 13, 14.
**Decisions due:** libc ADR at phase end (musl is the default; the spike confirms). Exec stays kill-and-create-thread with no in-place swap primitive; the failed-exec and mid-swap-signal tests are written against it and re-open the design only if they expose a race. `SPAWN` is part of the procmgr protocol; benchmark both fork+exec and spawn.
**Plans:** TBD

### Phase 5: Signals, sessions and tty

**Goal:** As a shell author, I want signals, process groups, a controlling terminal and pty pairs, so that job control behaves as POSIX specifies.
**Mode:** mvp
**Depends on:** Phase 4
**Requirements:** SIG-01, SIG-02, SIG-03, SIG-04, SIG-05, TTY-01, TTY-02, TTY-03, VFS-07, VFS-08
**Success Criteria** (what must be TRUE):
  1. `kill`, `sigaction`, `sigprocmask`, `sigsuspend`, `sigpending`, `sigaltstack`, `raise` and `alarm` work over the kernel interrupt and redirect from Rust test binaries. A handler runs once and returns through `sigreturn`, and `kill(self)` works.
  2. A CPU exception becomes `SIGSEGV`, `SIGFPE`, `SIGILL` or `SIGBUS` for the faulting thread only. `SIGCHLD` arrives on child exit or stop, `SIGPIPE` on a write to a closed pipe, `SIGALRM` on the timer, and `SIGXCPU` on CPU-budget exhaustion.
  3. Restartable and non-restartable calls behave per POSIX when interrupted (`EINTR` versus `SA_RESTART`) for every blocking server call, including `wait`, pipe reads, `ppoll` and blocking lock waits. OFD locks and advisory record locks work and are released on process death.
  4. A cross-domain `kill` returns the errno recorded in the amending ADR and reveals no existence information.
  5. On the serial console, ttyd provides canonical and raw modes, echo and `ISIG`. Ctrl-C reaches only the foreground group, and background reads and writes raise `SIGTTIN` and `SIGTTOU`. A pty master and slave pass data. `ppoll`/`poll` report readiness for fds, pipes and ttys. Rust ports of the os-test `pty` cases pass from the first tty build.

**Research:** deeper research. Multithread signal routing, `SA_RESTART` interplay with Cancel, async-signal-safety of the entry stub, ctty and job-control ownership. Needs the serial RX findings from Phase 1. Watch pitfalls 1, 3, 7, 8, 22.
**Decisions due:** The EINTR/`SA_RESTART` contract is defined once, before any server implements it. Cross-domain `kill` errno per the Phase 3 sign-off.
**Plans:** TBD

### Phase 6: libc, BusyBox, conformance and measurement

**Goal:** As the researcher, I want BusyBox on musl, gated os-test results and Linux-relative timings, so that B3 reports measured adherence and the cost of the layer.
**Mode:** mvp
**Depends on:** Phase 4, Phase 5
**Requirements:** LIBC-02, LIBC-03, LIBC-04, LIBC-05, SHELL-01, SHELL-02, SHELL-03, SHELL-04, TEST-02, TEST-03, TEST-04, TEST-05, PERF-01, PERF-03, AGNT-01, AGNT-04
**Success Criteria** (what must be TRUE):
  1. A C `hello` built from the musl patch series on the pinned post-CVE commit runs on the system through `libaok`. `pthread_create`, mutexes and condition variables work via futex, and the `atfork` hooks and the `posix_spawn` mapping are verified against the pin.
  2. BusyBox 1.38.0 builds static with `ash` from a committed `.config` and explicit applet list. `ash -c 'ls | grep x > /tmp/y'` runs, then scripts with pipelines, redirection, command substitution and background jobs. Interactive `ash` on the serial console has working `^C`, `^Z`, `fg` and `bg`. BusyBox `testsuite/runtest` runs with a triaged expected-fail list.
  3. os-test at the pinned commit runs in CI with per-suite allowlists that only grow, gated on every outcome including hang and neutral. Kernel-heavy suites (`process`, `signal`, `pty`, `io`) are reported separately from libc-bound ones (`include`, `namespace`, `limits`, `basic`). A musl-on-Linux control row runs the same suites. The Open POSIX Test Suite and musl libc-test run as secondary suites with allowlists.
  4. Fork+exec, `posix_spawn`, signal round trip, pipe throughput and latency, and null `read`/`write` are measured against a Linux guest in the same QEMU/KVM configuration. The write-up in `docs/architecture/` records N, median, p99, commit, QEMU flags and host CPU, with the Phase 4 prediction beside the measurement.
  5. A process without an agent credential scores identically to an agent with a full grant on the same suites. A real BusyBox tool run under a narrowed namespace fails with `ENOENT` outside the grant and works inside it.

**Research:** research-phase for the musl patch (asm syscall entry and `__set_thread_area` against the chosen pin; `clone`, `CLONE_CHILD_CLEARTID` and `set_tid_address` mapping; OPTS build on musl headers). The harness and allowlist mechanism are specified in STACK; read the os-test JSON schema in-phase. KVM on the CI host is assumed, not checked.
**Decisions due:** Verify whether musl's `posix_spawn` uses `clone(CLONE_VM|CLONE_VFORK)` in the pin and map it to the procmgr `SPAWN`. No os-test target tier is set here (B5 owns it); the allowlist baseline is whatever the first full run produces.
**Plans:** TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Userland, syscall ABI and IPC v2 | 1/7 | In Progress | - |
| 2. Process objects, teardown and timers | 0/0 | Not started | - |
| 3. Bootstrap, server runtime and vfs | 0/0 | Not started | - |
| 4. exec and procmgr | 0/0 | Not started | - |
| 5. Signals, sessions and tty | 0/0 | Not started | - |
| 6. libc, BusyBox, conformance and measurement | 0/0 | Not started | - |
