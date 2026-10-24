# Requirements: agentic-os-kernel (B3, POSIX core)

**Defined:** 2026-10-24
**Core Value:** The protection boundary knows what an agent is: authority is exactly the set of capability handles an agent holds, and the kernel stamps its identity on every message, while unmodified POSIX programs still run.

Scope: milestone B3 only. B4 and B5 are in v2. Sources: `.planning/PROJECT.md`, `.planning/research/SUMMARY.md`
(and the four files it summarizes), `docs/architecture/scoping.md`, `option-b-microkernel.md`,
`b2-spike-report.md`.

Every requirement is testable by a ktest, a host unit test, a QEMU integration run, or a
recorded measurement. "Test" below names what would show it worked.

## v1 Requirements

### Kernel enablers (KERN)

- [ ] **KERN-01**: User-mode FPU/SSE state is enabled under OSTD 0.19.0 and saved and restored per thread across context switches and signal delivery. If OSTD cannot do this without patches or `unsafe`, B3 stops and the Redox-fork re-score in ADR 0002 is triggered. Test: a ring-3 program keeps an XMM value across a yield and a signal redirect.
- [ ] **KERN-02**: A ring-3 syscall ABI exists (`syscall.rs`) with a typed decode layer. Test: a fuzz-style ktest feeds malformed numbers, pointers and lengths and gets errno returns, never a kernel panic.
- [ ] **KERN-03**: User code (servers, tests, bootstrap) builds as Rust `no_std` for a custom `x86_64-aok-user.json` target inside the Docker image and loads from a flat bootstrap image. Test: a Rust ring-3 test crate replaces the hand-assembled B2 test programs.
- [ ] **KERN-04**: `Object::Space` and `Object::Thread` capability types exist, with `thread_create(space, ctx, tag)` gated by the Space capability. Test: a holder can create and signal a thread in another space; a non-holder cannot.
- [ ] **KERN-05**: CSpace grows past the 64-slot B2 limit. Test: install and derive more than 64 capabilities.
- [ ] **KERN-06**: Every kernel write into user memory goes through one `copy_to_user` that breaks CoW and retries. Test: the first `read` into a buffer after `fork` succeeds (written before procmgr exists).
- [ ] **KERN-07**: An idle task halts the CPU when nothing is runnable. Test: the scheduler runs with all agents blocked and the CPU halts instead of spinning.
- [ ] **KERN-08**: Destroying a process returns its carved budgets and capabilities to the parent (`charge_pages` gets an inverse) and wakes its futex waiters. Test: a 10,000x fork+exit loop leaves page and capability counts flat.
- [ ] **KERN-09**: `futex` wait and IPC `recv` accept timeouts, and a timer can deliver a message to an endpoint. Test: timed waits expire within a stated tolerance.
- [ ] **KERN-10**: Thread-exit and exception notifications go to a process endpoint and name the faulting `tid` and `tag`. Test: procmgr can tell which of two forked siblings faulted.
- [ ] **KERN-11**: Signal redirect saves the full user context on a kernel-held stack and `sigreturn` restores it, with no kernel write to the user stack. Test: all general registers and FPU state survive a redirect and return.
- [ ] **KERN-12**: One shared `block_until(condition, deadline)` helper implements every blocking wait. Hand-rolled wait loops are absent. Test: a fail-point ktest forces the wake-versus-interrupt interleaving, plus a 1000x loop in CI.

### IPC (IPC)

- [ ] **IPC-01**: Synchronous `call` with a kernel-held reply handle, deferred reply, and `reply_recv`. Test: a server parks a request and replies later from another event.
- [ ] **IPC-02**: Interrupting a blocked `call` sends a Cancel to the server and the caller stays blocked until the server replies, with commit-or-not-started semantics. Replies to dead waiters are dropped by the kernel. Abort-at-once EINTR stays only for a blocked `recv`. Test: an interrupted `read` loses no data.
- [ ] **IPC-03**: The kernel stamps every message with the sender's `tid` and `tag` (the tag is set at `thread_create`). Test: two forked siblings are distinguishable by a server.
- [ ] **IPC-04**: Bulk data moves by kernel copy between address spaces with CoW break on the destination. Test: a multi-page `read` and `write` across spaces matches byte for byte.
- [ ] **IPC-05**: Capabilities transfer in messages with rights attenuation. Test: a file capability passes from vfs through exec and the receiver cannot exceed its rights.
- [ ] **IPC-06**: The null-IPC round trip is measured the day IPC v2 lands and recorded with N, median and p99.

### Server runtime and bootstrap (SRV)

- [ ] **SRV-01**: A shared `elf` crate parses static ELF for bootstrap and exec (goblin `elf64`, `endian_fd`).
- [ ] **SRV-02**: An in-repo `server-rt` framework provides the receive loop, request parking and the acyclic call-graph rule. Servers are `no_std` + `alloc`.
- [ ] **SRV-03**: Bootstrap is the only holder of root authority, wires procmgr, vfs and exec, distributes capabilities, then drops its own.
- [ ] **SRV-04**: An initramfs (cpio newc) is delivered to vfs at boot, via OSTD boot modules or an `include_bytes!` fallback.

### Process management (PROC)

- [ ] **PROC-01**: procmgr owns pid, ppid, pgrp, session and uid/gid, and owns every user Thread and Space capability. Server tables are keyed on `(agent_id, tag)`.
- [ ] **PROC-02**: `fork` is reserve-then-commit. A failed `fork` (budget, pids) leaves no partial child. Test: `fork` under an exhausted budget returns `EAGAIN` or `ENOMEM` and leaks nothing.
- [ ] **PROC-03**: `_exit`, `wait`/`waitpid` with `WNOHANG`, zombie and limbo as explicit states, and reparent-to-init. The process state machine is a pure host-testable library with a model-based test.
- [ ] **PROC-04**: `posix_spawn` is a procmgr operation. Test: spawn and fork+exec produce the same child state.
- [ ] **PROC-05**: A failed `exec` leaves the old image running, and a signal arriving mid-swap is delivered to exactly one image. Test: both written against the kill-and-create-thread design.
- [ ] **PROC-06**: `setpgid`, `setsid`, `getpgid`, `getsid` follow POSIX, with the ownership table (procmgr owns pgrp, session, ctty link) written before code. Test: os-test `process` suite.
- [ ] **PROC-07**: Process death fans out: the kernel notifies procmgr, which notifies vfs and ttyd. Test: a pipe reader sees EOF when the writer is SIGKILLed.

### Exec (EXEC)

- [ ] **EXEC-01**: The exec server builds a complete fresh Space (segments, stack, argv, envp, auxv) from a static ELF read through a vfs file capability, and returns it to procmgr.
- [ ] **EXEC-02**: Malformed or unsupported ELF (dynamic, wrong machine, overlapping segments) fails with the right errno and no state change.
- [ ] **EXEC-03**: Credentials inherit across `fork`, `exec` and `posix_spawn`. An agent cannot gain capabilities through exec. Test: an adversarial test attempts escalation via exec of a file in a wider namespace.

### Virtual filesystem (VFS)

- [ ] **VFS-01**: vfs serves ramfs from the initramfs plus tmpfs, pipefs and devfs (`/dev/null`, `/dev/zero`, `/dev/tty`, `/dev/console`).
- [ ] **VFS-02**: Per-domain namespaces: a name outside the caller's grant resolves to `ENOENT`. Built into the first vfs, not retrofitted. Test: domain A cannot name domain B's files.
- [ ] **VFS-03**: Open file descriptions are shared by `dup`, `dup2` and `fork`. Fd tables are held in vfs, keyed on `(agent_id, tag)`. A rights field is reserved in the OFD handle. Test: offset sharing across `fork`.
- [ ] **VFS-04**: The `*at` family, `openat` flag combinations, `O_CLOEXEC`, `fcntl` basics, `stat`, `readdir`, `mkdir`, `unlink`, `rename`, `link`, `symlink`, `readlink`, `truncate`. Test: os-test `io` suite, 22 open-flag cases.
- [ ] **VFS-05**: Mode-bit permission checks against the caller's uid/gid pushed by procmgr. uid/gid is a server attribute, never kernel authority. Read-only grants fail with the errno chosen in the open decision below.
- [ ] **VFS-06**: Pipes and FIFOs with blocking and `O_NONBLOCK` semantics via deferred reply, `SIGPIPE`/`EPIPE`, and EOF on writer death.
- [ ] **VFS-07**: `fcntl` OFD locks (os-test `io`: 32 cases) and advisory record locks.
- [ ] **VFS-08**: `ppoll`/`poll` over fds, pipes and ttys.

### Signals, sessions and tty (SIG)

- [ ] **SIG-01**: `kill`, `sigaction`, `sigprocmask`, `sigsuspend`, `sigpending`, `sigaltstack`, `raise`, `alarm`, with delivery over the kernel interrupt and redirect. Test: os-test `signal` suite.
- [ ] **SIG-02**: Fault-to-signal: CPU exceptions become `SIGSEGV`, `SIGFPE`, `SIGILL`, `SIGBUS` for the faulting thread.
- [ ] **SIG-03**: `SIGCHLD` on child exit or stop, `SIGPIPE`, `SIGALRM`, `SIGXCPU` on CPU-budget exhaustion.
- [ ] **SIG-04**: The EINTR/`SA_RESTART` contract is defined once and holds for every blocking server call. Test: restartable and non-restartable calls behave per POSIX on interruption.
- [ ] **SIG-05**: A cross-domain `kill` returns the errno chosen in the open decision below and leaks no existence information.
- [ ] **TTY-01**: ttyd provides a line discipline and `termios` (canonical and raw modes, echo, `ISIG`) on the serial console.
- [ ] **TTY-02**: Controlling terminal and foreground process group follow POSIX; background reads and writes raise `SIGTTIN`/`SIGTTOU`. Test: os-test `pty` suite, run from the first tty build.
- [ ] **TTY-03**: pty master and slave pairs.

### Libc (LIBC)

- [ ] **LIBC-01**: A libc decision ADR is recorded at the end of the exec/procmgr phase, from a bounded spike (`hello`, `fork`, `read`) with go/no-go criteria: fd-table placement, `clone` mapping, `pthread_join` on futex. musl is the default.
- [ ] **LIBC-02**: `libaok` (Rust `no_std` staticlib) maps Linux syscall numbers to kernel syscalls and `posix-client` calls. Linux syscall numbers stop at `libaok`; server protocols are POSIX-operation granular.
- [ ] **LIBC-03**: musl builds statically as a patch series on a pinned post-CVE-fix commit, with the asm syscall entry and `__set_thread_area` patched. Test: a C `hello` runs on the system.
- [ ] **LIBC-04**: Thread creation, set-FS-base and exit-and-unmap kernel calls exist so musl start-up works. `pthread_create`, mutexes and condition variables work via futex.
- [ ] **LIBC-05**: `atfork` hooks and the `posix_spawn` mapping are verified against the pinned musl.

### Shell (SHELL)

- [ ] **SHELL-01**: BusyBox 1.38.0 builds static with `ash`, `CONFIG_NOMMU=n`, a committed `.config` and an explicit applet list.
- [ ] **SHELL-02**: `ash -c 'ls | grep x > /tmp/y'` runs, then scripts with pipelines, redirection, command substitution and background jobs.
- [ ] **SHELL-03**: Interactive `ash` on the serial console with job control (`^C`, `^Z`, `fg`, `bg`).
- [ ] **SHELL-04**: BusyBox `testsuite/runtest` runs with a triaged expected-fail list.

### Conformance and CI (TEST)

- [ ] **TEST-01**: A host-side harness runs test binaries in QEMU over the serial console with framed records, a watchdog, and a distinct "hang" outcome. The skeleton lands with Rust test binaries before C suites exist.
- [ ] **TEST-02**: Sortix os-test (pin `8e790a45188ac67b550ba62cdde4103b253f5ce8`) runs in CI with per-suite allowlists that only grow, gated on every outcome including hang and neutral.
- [ ] **TEST-03**: Kernel-heavy suites (`process`, `signal`, `pty`, `io`) are reported separately from libc-bound ones (`include`, `namespace`, `limits`, `basic`).
- [ ] **TEST-04**: A musl-on-Linux control row runs the same suites so libc-attributable failures are visible.
- [ ] **TEST-05**: Open POSIX Test Suite (LTP `20260930`, fetched at CI time) and musl libc-test run as secondary suites with allowlists.
- [ ] **TEST-06**: The existing ktests and the boot-time CPU-budget check stay green on every commit through B3, and `cargo clippy` and `cargo fmt --check` pass in CI.
- [ ] **TEST-07**: Host-side unit tests cover path resolution and the procmgr state machine without QEMU.

### Performance (PERF)

- [ ] **PERF-01**: fork+exec, `posix_spawn`, signal round trip, pipe throughput and latency, and null `read`/`write` are measured against a Linux guest in the same QEMU/KVM configuration, with rdtsc timing, N, median and p99.
- [ ] **PERF-02**: Before the first fork+exec run, a "layer is wrong" threshold is set from the IPC round-trip (Phase 1) and `cow_clone` (Phase 4) primitive timings, together with the predicted fork+exec sum. The prediction and the measurement are both published.
- [ ] **PERF-03**: Results and methodology are written up in `docs/architecture/` with the commit, QEMU flags and host CPU recorded.

### Agentic properties (AGNT)

- [ ] **AGNT-01**: A process without an agent credential sees an unmodified POSIX system. Dual-mode run: the suite score for non-agent and for agent-with-full-grant is identical.
- [ ] **AGNT-02**: Agent budget exhaustion maps to standard errno (`ENOMEM`, `EAGAIN`) and `SIGXCPU`, never a new interface. Test: a memory-budget-exhausted `mmap` returns `ENOMEM`.
- [ ] **AGNT-03**: Server audit records carry the kernel-stamped credential, not a client-supplied one.
- [ ] **AGNT-04**: A narrow-grant demo: a real BusyBox tool run under a reduced namespace fails with `ENOENT` outside the grant and works inside it.
- [ ] **AGNT-05**: The server CPU-exemption decision is recorded with a request ledger and a documented limit. The core-value statement does not claim kernel-enforced CPU limits cover server-side work.

## v2 Requirements

### B4: agents running POSIX tools under a grant

- **AGT-01**: Agents execute POSIX tools under attenuated capability grants end to end
- **AGT-02**: Inference server and inference device as a kernel-visible resource
- **AGT-03**: Context store
- **AGT-04**: Trusted-path escalation for grants
- **AGT-05**: Sockets, to support the above

### B5: adherence tier

- **ADH-01**: Choose an os-test target tier (baselines: Minix 73%, Redox 84%, Sortix 85%, FreeBSD 90%, Linux 92%, musl 94%) and a date
- **ADH-02**: Close failing kernel-heavy suites to reach the chosen tier

### Deferred from B3

- **VFS2-01**: fd-as-capability rights masks (the OFD rights field is reserved in B3)
- **VFS2-02**: Scoped accounting files; scoped PID namespace
- **VFS2-03**: File-backed `mmap`
- **VFS2-04**: Disk filesystems
- **LIBC2-01**: pthread polish beyond what BusyBox and the suites need
- **KERN2-01**: SMP, after `cow_clone` TLB shootdown and the interrupt race are fixed
- **KERN2-02**: Dynamic linking (`ld.so`)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Formal POSIX certification | Dropped 2026-10-23; adherence measured with free suites |
| setuid/setgid, uid-0 bypass | Ambient authority; contradicts "authority is exactly the capability set" |
| `/proc`, `/sys` | Ambient introspection of other domains; Linux-specific |
| Linux ABI emulation | The libc shim is the compatibility layer; emulation drags in the Linux surface |
| Ambient `/` visible to every process | Namespace-as-authority; every name must be granted |
| Dynamic linking in B3 | Static ELF only; `ld.so` is a large untested surface (v2) |
| Sockets | B4 |
| SysV IPC, AIO, mqueue | Not needed by BusyBox or the target suites |
| SMP | `-smp 1`; revisit only if B3 forces it |
| A broker or an LLM in any syscall path | The LLM is never in the trusted computing base |
| GUI | Not a goal |
| Frontier model training or hosting | Out of research scope |
| B1 Linux broker prototype | B2 built grant, attenuate and revoke in-kernel |
| Side channels between agents sharing an inference device | Threat model v2 |

## Open decisions (need sign-off; requirements above assume the research recommendation)

- **Cross-domain `kill` errno and read-only grant errno.** `option-b-microkernel.md` says `EPERM` and `EACCES`. Research recommends `ESRCH` (leaks nothing) and `EROFS` (the POSIX errno). Changing it needs an ADR amending option-b. Affects SIG-05 and VFS-05, decided by the vfs phase.
- **musl as the default libc.** Research evidence is from musl source and published scores only; nothing was built. LIBC-01 keeps it a spike with go/no-go criteria rather than a commitment.
- **Flat bootstrap image and ELF parsing in userland** are researcher recommendations (SRV-01, SRV-03, KERN-03), not decisions from the scoping documents.

## Traceability

Each v1 requirement maps to exactly one phase. Rule: the phase that builds its primary component.

| Requirement | Phase | Status |
|-------------|-------|--------|
| KERN-01 | Phase 1 | Pending |
| KERN-02 | Phase 1 | Pending |
| KERN-03 | Phase 1 | Pending |
| KERN-04 | Phase 2 | Pending |
| KERN-05 | Phase 1 | Pending |
| KERN-06 | Phase 1 | Pending |
| KERN-07 | Phase 2 | Pending |
| KERN-08 | Phase 2 | Pending |
| KERN-09 | Phase 2 | Pending |
| KERN-10 | Phase 2 | Pending |
| KERN-11 | Phase 2 | Pending |
| KERN-12 | Phase 1 | Pending |
| IPC-01 | Phase 1 | Pending |
| IPC-02 | Phase 1 | Pending |
| IPC-03 | Phase 1 | Pending |
| IPC-04 | Phase 1 | Pending |
| IPC-05 | Phase 1 | Pending |
| IPC-06 | Phase 1 | Pending |
| SRV-01 | Phase 3 | Pending |
| SRV-02 | Phase 3 | Pending |
| SRV-03 | Phase 3 | Pending |
| SRV-04 | Phase 3 | Pending |
| PROC-01 | Phase 4 | Pending |
| PROC-02 | Phase 4 | Pending |
| PROC-03 | Phase 4 | Pending |
| PROC-04 | Phase 4 | Pending |
| PROC-05 | Phase 4 | Pending |
| PROC-06 | Phase 4 | Pending |
| PROC-07 | Phase 4 | Pending |
| EXEC-01 | Phase 4 | Pending |
| EXEC-02 | Phase 4 | Pending |
| EXEC-03 | Phase 4 | Pending |
| VFS-01 | Phase 3 | Pending |
| VFS-02 | Phase 3 | Pending |
| VFS-03 | Phase 3 | Pending |
| VFS-04 | Phase 3 | Pending |
| VFS-05 | Phase 3 | Pending |
| VFS-06 | Phase 3 | Pending |
| VFS-07 | Phase 5 | Pending |
| VFS-08 | Phase 5 | Pending |
| SIG-01 | Phase 5 | Pending |
| SIG-02 | Phase 5 | Pending |
| SIG-03 | Phase 5 | Pending |
| SIG-04 | Phase 5 | Pending |
| SIG-05 | Phase 5 | Pending |
| TTY-01 | Phase 5 | Pending |
| TTY-02 | Phase 5 | Pending |
| TTY-03 | Phase 5 | Pending |
| LIBC-01 | Phase 4 | Pending |
| LIBC-02 | Phase 6 | Pending |
| LIBC-03 | Phase 6 | Pending |
| LIBC-04 | Phase 6 | Pending |
| LIBC-05 | Phase 6 | Pending |
| SHELL-01 | Phase 6 | Pending |
| SHELL-02 | Phase 6 | Pending |
| SHELL-03 | Phase 6 | Pending |
| SHELL-04 | Phase 6 | Pending |
| TEST-01 | Phase 4 | Pending |
| TEST-02 | Phase 6 | Pending |
| TEST-03 | Phase 6 | Pending |
| TEST-04 | Phase 6 | Pending |
| TEST-05 | Phase 6 | Pending |
| TEST-06 | Phase 1 | Pending |
| TEST-07 | Phase 3 | Pending |
| PERF-01 | Phase 6 | Pending |
| PERF-02 | Phase 4 | Pending |
| PERF-03 | Phase 6 | Pending |
| AGNT-01 | Phase 6 | Pending |
| AGNT-02 | Phase 2 | Pending |
| AGNT-03 | Phase 3 | Pending |
| AGNT-04 | Phase 6 | Pending |
| AGNT-05 | Phase 2 | Pending |

**Coverage:**
- v1 requirements: 72 total
- Mapped to phases: 72
- Unmapped: 0

---
*Requirements defined: 2026-10-24*
*Last updated: 2026-10-24 after roadmap creation*
