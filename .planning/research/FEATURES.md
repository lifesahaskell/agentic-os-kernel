# Feature Research

**Domain:** POSIX personality (procmgr, vfs, exec, libc, BusyBox shell) on a Rust capability microkernel where agents are kernel objects
**Milestone:** B3 (POSIX core), subsequent to the validated B2 kernel spike
**Researched:** 2026-10-23
**Confidence:** MEDIUM overall. HIGH for what os-test tests (read from a clone of the suite), HIGH for what B2 already provides (read from `kernel/src` and the spike report), MEDIUM for BusyBox/ash needs (from prior knowledge of the code, not re-verified this pass), LOW for any layer-cost estimate (no system publishes one; see `docs/architecture/research/microkernel-posix.md` section 8.4).

Repo docs relied on: `docs/architecture/option-b-microkernel.md` (sections 2, 3, 5), `docs/architecture/scoping.md` (sections 5, 7), `docs/architecture/research/microkernel-posix.md`, `docs/architecture/research/rust-kernels-and-adherence.md`, `docs/architecture/b2-spike-report.md`. They are not repeated here; this file fills gaps and orders the work.

## Findings that change the plan

These came from reading os-test source (shallow clone of https://gitlab.com/sortix/os-test, 2026-10-23) and the B2 code. Each one moves a feature up or down.

1. **os-test `process` is 24 tests and all of them are pgrp/session edge cases.** Names: `fork-setpgid-*`, `fork-setsid-setpgid-*`, `zombie-setpgid*`, `limbo-setpgid`, `waitpid-pgid-*`, `fork-exec-setpgid-*`. There is no plain fork/exec/wait test in it. Score there is a procmgr state-machine exercise (zombies, "limbo" children, group membership after leader exit), not a kernel exercise. MINIX 16% vs Hurd/Redox 83% (research doc section 7) is a procmgr-correctness gap. Cheap to win if procmgr models pgrp/session/zombie transitions from the first build.
2. **os-test `io` is 55 scored tests, 32 of which are OFD locks (`ofd-getlk/setlk-*`, `F_OFD_*`) and 22 are `open()` flag combinations** (`open-tmpdir-{rdonly,wronly,rdwr}-{creat,trunc,append,directory}`, `open-mkstemp-*`), plus `open-clofork-fork` and `dup3-clofork-fork`. Redox scores 94%, Sortix 41%. OFD locks hang off the open-file description, so vfs gets them nearly free if it owns OFD objects. This is the largest cheap score block outside libc.
3. **os-test `pty` is 29 tests, nearly all controlling-terminal and foreground-group state machines** (`tcgetpgrp-*`, `tcsetpgrp-wrong-*`, `tiocsctty-*`, `tiocnotty-*`, `pty-hup-*`). Redox 6% vs Sortix 96%. It needs pgrp/session state in procmgr and the tty server to agree. It is not about line editing.
4. **os-test `signal` is 31 tests about mask/ignore/handler/raise ordering and exec inheritance** (`block-raise-unblock`, `ignore-block-raise-unignore-unblock`, `*-exec`, `sigaltstack-*`, `ppoll-block-*`). The `ppoll-*` tests need `ppoll` with an atomic mask swap. Signal state must survive `exec` in the POSIX-defined way (handlers reset, ignores and mask kept).
5. **os-test `basic` is one invocation test per function, 979 non-neutral tests (Redox 76%, Sortix 70%)**, across 74 header directories including `pthread`, `aio`, `mqueue`, `ndbm`, `glob`, `iconv`. This is mostly libc breadth. Linking a function that returns `ENOSYS` still scores some of it. Do not let it set kernel priorities.
6. **Gap in B2: servers cannot tell two processes of the same agent apart.** `ipc::Message.sender` is the agent `Credential` only (`kernel/src/ipc.rs`, `agent.rs`); there is no per-endpoint badge. procmgr and vfs must identify the calling pid, cwd, fd table and uid. Without a per-process identity on the wire, every fork child in one agent looks identical. Needed before vfs can exist: either a badge/word minted per capability derive (seL4 style) or a per-process session endpoint minted by procmgr. This is a prerequisite, not a feature a user sees.
7. **Gap in B2: kernel writes into user memory skip CoW** (b2-spike-report "Friction" item 1). After `fork`, the first `read(fd, buf)` into a CoW page lands in the receiver buffer through IPC. That path errors instead of faulting. Fork plus read is the BusyBox hot path, so this is a B3 blocker, not a polish item.
8. **POSIX.1-2024 made threads mandatory base.** The kernel futex exists, so pthreads are libc work. BusyBox does not need them; the os-test `basic` and `TPS`/`TSH` rows do. Schedule them after the shell boots.

## Feature Landscape

Complexity: LOW is under a few days of agent-assisted work, MEDIUM is a phase-sized item, HIGH is multi-phase or carries design risk. Estimates are judgement, not measurement.

### Table Stakes (BusyBox boots, os-test scores, nothing explodes)

Ordered by dependency. Group letter is the build tier (see Dependencies).

#### Tier 0: kernel-side enablers (no POSIX visible)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Ring-3 syscall ABI for cap, IPC, thread, address-space ops | User mode only has `exit` today; nothing else can be built | MEDIUM | Keep the ABI capability-shaped (handle + op), not Linux-numbered. The libc shim maps to it. |
| `Object::AddressSpace` and `Object::Thread` capability types; cap-gated `cow_clone` | procmgr must fork and signal on another process's behalf (B2 "Not verified") | MEDIUM | Rights on AS: clone, map, destroy. Rights on Thread: interrupt, redirect, read-regs. Exceptions must name the faulting thread (B2 gap). |
| Per-process identity on IPC (badge or per-process session endpoint) | Finding 6 | MEDIUM | Decide in ARCHITECTURE. Server-side pid table keyed on the badge. |
| Kernel write into CoW user page: break CoW first or fault-and-retry | Finding 7 | MEDIUM | Inferred from the OSTD API, never exercised. Write the failing ktest first. |
| Real idle task; return of budgets/caps on agent death | B2 gaps; procmgr reaping needs teardown that actually frees | LOW-MEDIUM | `exit`/`wait` on a leaky teardown corrupts the memory-budget story. |
| Futex timeouts | `pthread_cond_timedwait`, `sem_timedwait`, `sleep` built on futex | LOW | B2 futex has none. Also needs a timer source (nanosleep/clock_gettime). |

#### Tier 1: boot a static binary

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| exec server: static ELF (ET_EXEC and static-PIE) into a fresh address space, argv/envp/auxv on the stack | Nothing else runs without it | MEDIUM | No dynamic linker. Static musl or relibc BusyBox only. Shebang (`#!`) handling belongs in exec server; BusyBox ash scripts need it. |
| init and initramfs-style boot image served by vfs | Something must be pid 1 and own orphans | LOW | Pid 1 reaps reparented children (needed by `wait` semantics). Image is a ramfs populated at boot. |
| procmgr: pid allocation, ppid, process table, `_exit`, `wait`/`waitpid` (WNOHANG, status macros), zombie reaping, orphan reparent to init | Core of "process" | MEDIUM | Model zombie and "limbo" (post-fork, pre-exec) as explicit states from the start (finding 1). |
| `fork` via procmgr: clone AS, copy fd table (bump OFD refcounts), copy signal dispositions, single-threaded child | The headline operation | MEDIUM-HIGH | Lazy CoW and charge-the-faulter are done in B2. Cost is procmgr round trips; measure. Child stays in the parent's agent object. |
| `execve` / `execvp` / `posix_spawn` | Shell's only way to launch a command | MEDIUM | `posix_spawn` should be a first-class procmgr operation, not libc `fork`+`exec`: it is the cheap path and avoids a whole AS clone. Use for the latency benchmark too. |
| `getpid`, `getppid`, `getuid`/`getgid`/`geteuid`/`getegid`, `umask` | Every program calls them | LOW | uid/gid is a procmgr attribute (Key Decision). Real/effective/saved all stored; `setuid` only for uid-0 domain, see Anti-Features. |
| libc: stdio, string, malloc, errno, `environ`, `atexit`, startup (`_start`, `__libc_start_main` equivalent) | Cannot link BusyBox otherwise | MEDIUM-HIGH | Choice is open (relibc vs musl+shim, option-b section 7 B3). This research has no new evidence to decide it. Decide by spiking `hello` + `fork` + `read` on both before committing. |
| `brk`/`sbrk`/`mmap(MAP_ANONYMOUS|MAP_PRIVATE)`/`munmap`/`mprotect` | malloc and ELF loading | MEDIUM | `MAP_SHARED` file-backed writable (MINIX refuses it with ENXIO) is not needed for BusyBox. |
| Clock: `clock_gettime`, `nanosleep`/`sleep`, `time` | `sleep` applet, make, timeouts | LOW-MEDIUM | Needs a timekeeping server or direct kernel clock object. |

#### Tier 2: files, fds, pipes, console

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| vfs: ramfs/tmpfs, path resolution inside the caller's namespace (`.`, `..`, absolute, relative, trailing slash rules), `ENOENT` outside | Everything is a file | MEDIUM-HIGH | Lookup root is the domain root, never a global one. `..` at the namespace root stays at the root. |
| Per-process cwd and root; `chdir`, `getcwd`, `fchdir`; `openat` family and `*at` calls | Shell `cd`, `pwd`; `*at` needed by modern coreutils and by namespace-safe code | MEDIUM | Make `openat` the real primitive and `open` a wrapper (Redox NLnet direction, research doc Part 1). |
| Open-file descriptions in vfs; fd table; `open`/`close`/`read`/`write`/`lseek`/`dup`/`dup2`/`dup3`/`fcntl(F_DUPFD, F_GETFD/SETFD, F_GETFL/SETFL)`; `O_CLOEXEC`; `O_APPEND`, `O_TRUNC`, `O_CREAT`, `O_EXCL`, `O_DIRECTORY`, `O_NOCTTY`, `O_NONBLOCK` | POSIX fd semantics, shared offset across `dup` and `fork` | MEDIUM | Finding 2: the `open-*` and `dup3-clofork-fork` os-test files are exactly this. fd table lives server-side (MINIX-style, keyed on pid) so it survives `exec` and `CLOEXEC` is applied there. A client-side table loses fds across exec unless re-passed. Recommend vfs-held for B3; revisit if the round trip dominates the latency numbers. |
| `stat`/`fstat`/`lstat`/`fstatat`, `mkdir`, `rmdir`, `unlink`, `rename` (atomic), `link`, `symlink`/`readlink`, `opendir`/`readdir`/`getdents`, `truncate`, `chmod`/`fchmod`, `chown`, `access`, `utimensat` | Every coreutils and BusyBox file applet | MEDIUM | Symlink resolution must be done by vfs against the caller's namespace root, never the host's, so a symlink cannot escape the grant. |
| Permission bits enforced in vfs (owner/group/other, `rwx`, sticky, directory search) using the uid/gid from procmgr | "Mode bits answer may you" (option-b section 3.1) | MEDIUM | uid 0 override ("root bypass") needs a decision, see Anti-Features. |
| `pipe`/`pipe2`, `mkfifo` (named pipes), blocking, `PIPE_BUF` atomicity, EOF on last-writer close, `SIGPIPE`/`EPIPE` | Shell pipelines | MEDIUM | Pipe is its own OFD kind in vfs (or a tiny pipe server). Pipe latency is one of the three benchmark numbers. |
| `/dev/null`, `/dev/zero`, `/dev/full`, `/dev/urandom` (or `getrandom`), `/dev/tty` | Shell redirections, BusyBox | LOW | Static device nodes in the ramfs, served by a small devfs server. |
| Console tty: write path, canonical-mode line read, echo, `^C`/`^Z`/`^D`, `termios` (`tcgetattr`/`tcsetattr`, `ICANON`, `ECHO`, `ISIG`, `OPOST`, `ICRNL`), `isatty` | ash line editing and interactive use; `ISIG` ties tty to signals | MEDIUM-HIGH | Needed for an interactive shell. A non-interactive `ash -c` script boots without it, so ordering below puts a dumb serial console first. |
| `poll`/`ppoll`/`select`/`pselect` | ash `read`, `sleep`-like waits, `ppoll` in os-test `signal` | MEDIUM | Needs vfs and every server to export a readiness interface (Redox emulates over epoll; Hurd uses `io_select` RPC). Pick one protocol early, because servers written before it exists need rework. |
| `getrlimit`/`setrlimit` (stubs returning sane values), `uname`, `sysconf`/`pathconf`/`confstr` | configure scripts, BusyBox `ulimit`, os-test `limits` (Redox 94%, Sortix 97% - this is libc) | LOW | Mostly libc constants. |

#### Tier 3: signals and job control

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `sigaction`, `sigprocmask`/`pthread_sigmask`, `kill`, `raise`, `sigsuspend`, `sigpending`, `sigwait`/`sigtimedwait`, default actions (terminate, ignore, stop, continue, core), `SA_RESTART`, `SA_SIGINFO`, `SA_NODEFER`, `SA_RESETHAND` | os-test `signal`, every shell | HIGH | Kernel half exists (interrupt aborts blocked IPC with EINTR; redirect rewrites registers). Libc half: trampoline, `sigreturn`, signal frame, restart logic. Known cost: redirect of a running user thread waits up to one timer tick (B2 Friction 3). Document in latency numbers. |
| Signal state across `fork` and `exec` per POSIX | os-test `*-exec` tests | MEDIUM | Handlers reset on exec, ignores and mask preserved, pending cleared on fork child. |
| `SIGCHLD` generation and `SA_NOCLDWAIT`/`SA_NOCLDSTOP`; `SIGPIPE`, `SIGALRM` (`alarm`, `setitimer`), `SIGSEGV`/`SIGFPE`/`SIGILL`/`SIGBUS` from the B2 exception message | Shell job tracking; fault-to-signal | MEDIUM | Fault path: handler endpoint receives the exception message, procmgr synthesizes the signal. |
| `sigaltstack` | os-test `signal` (4 tests) | LOW-MEDIUM | Needs kernel/libc cooperation on the frame location. |
| `EINTR` plumbing in every blocking server call; restart only under `SA_RESTART` | The Hurd lesson: retrofitting this is a rewrite | MEDIUM | Define the protocol once: server operations that block must be cancellable by endpoint cancel and idempotent or restartable. Write it down before vfs and pipe servers are built. |
| `setpgid`, `getpgid`, `getpgrp`, `setsid`, `getsid`, `waitpid(-pgid)`, `kill(-pgid)`, orphaned process groups | os-test `process` (24 tests), shell job control | MEDIUM | Finding 1. Test from the first procmgr build (option-b section 6 risk). |
| Controlling terminal: ctty per session, `tcgetpgrp`/`tcsetpgrp`, `tcgetsid`, `TIOCSCTTY`, `TIOCNOTTY`, `SIGHUP` on hangup, `SIGTTIN`/`SIGTTOU` | Finding 3; ash job control | MEDIUM-HIGH | procmgr and tty server must share session/pgrp truth. Single source of truth in procmgr; tty server queries it. |

#### Tier 4: finish the score and the app gate

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `posix_openpt`/`grantpt`/`unlockpt`/`ptsname`, pty master/slave pair, `pty-hup-*` | os-test `pty`; `script`, `expect`, `ssh`-like programs | MEDIUM | On top of the console tty code; do not write a second line discipline. |
| `fcntl` advisory locks (`F_SETLK`, `F_GETLK`) and open-file-description locks (`F_OFD_*`) | 32 of 55 os-test `io` tests | MEDIUM | Lock table keyed on OFD (OFD) or (pid, inode) (classic). Classic locks drop on `exec`-less close of any fd for the file: a famous POSIX wart. Implement OFD first. |
| pthreads on futex: create/join/detach, mutex, condvar, rwlock, barrier, once, TLS, `pthread_atfork`, per-thread signal masks, `pthread_kill`, cancellation | POSIX.1-2024 base; os-test `basic` and TPS/TSH rows | HIGH | Kernel futex exists. `fork` in a multithreaded process must rebuild libc state in the child (QNX `forksafe_mutex`, Hurd `_Fork` hooks; research doc 8.4). Needs futex timeouts (Tier 0). |
| `mmap` of files (`MAP_PRIVATE` read), `msync` stub | Larger coreutils, `ld`-less loaders | MEDIUM-HIGH | Needs the pager hook (research doc 8.2 item 5) or a copy-in fallback. Fallback is fine for B3. |
| `getenv`/`setenv`, locale stubs (`C`/`POSIX` only), `iconv` stubs, `glob`, `fnmatch`, `regcomp`, `wordexp` | os-test `basic`, `fnmatch`, `regex`; shell globbing | MEDIUM | libc work. Take musl's if musl is chosen; relibc lags (Redox regex 61%). |
| BusyBox shell runs: ash, `echo`, `cat`, `ls`, `cp`, `mv`, `rm`, `mkdir`, `grep`, `sed`, `sh -c`, pipelines, redirection, `$(...)`, `&&`, `||`, `wait`, `trap`, `read` | The B3 exit criterion | n/a (integration) | Config note: build ash with job control optional first; enable once Tier 3 `ctty` passes. |
| BusyBox `testsuite/runtest` with a triaged expected-fail list | Milestone gate (option-b section 5) | MEDIUM | Needs the applets that the tests invoke, plus `diff`, `cmp`, `sort`. Many failures will be GNU-extension or `/proc` related; triage against the same BusyBox build on Linux, not an absolute count. |
| os-test wired into CI with per-suite allowlists that only grow; kernel-heavy suites (`process`, `signal`, `pty`, `io`) reported separately | Active requirement | MEDIUM | `make json`; allowlist per suite; list excludes `udp` (no net in B3). Run each suite twice, non-agent and agent-with-full-grant, and diff (scoping section 5 rule 1). |
| Benchmarks: fork+exec, `posix_spawn`, signal round trip, pipe latency, `read`/`write` null device, vs Linux on the same hardware | Active requirement; no published numbers anywhere | MEDIUM | Harness first, numbers second. Include a number for IPC round trip alone so layer cost can be split into kernel IPC vs server logic. |

### Differentiators (the agentic part of B3)

These are the reasons not to just fork Redox. Each must be additive and invisible to a process with no agent credential (scoping section 5 rules 1-2).

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Capability-namespace-aware vfs: each domain gets a mount table built from granted directory capabilities; names outside return `ENOENT`; read-only grants return `EROFS` for a read-only mount and `EACCES` for a mode-bit denial | The thing a Linux container cannot do natively: authority is the set of caps held. Prevents "needs `/`" | HIGH | Decision to make: option-b section 3.1 says granted read-only paths return `EACCES` on write. POSIX defines `EROFS` for writes on a read-only file system. Recommend `EROFS` for read-only mounts, `EACCES` for modes, so unmodified programs (`install`, `cp`) report the right message. Mapped denial codes are an existing constraint. |
| Credential inheritance across `fork`/`exec`/`posix_spawn`: child runs in the same agent object; exec cannot change agent, cannot add caps | Shelling out cannot shed or gain authority; the central claim of scoping 3.1 | MEDIUM | Mostly falls out of "procmgr clones the thread into the parent's agent". The work is the test suite: an adversarial test that every path that creates a process (fork, spawn, exec of a setuid file, ptrace-like attaches, `clone`-like) stays inside the agent. |
| Budget-to-errno mapping that stays POSIX: memory carve exhaustion -> `ENOMEM`; process-count/CPU carve exhaustion on `fork` -> `EAGAIN`; CPU budget exhausted -> `SIGXCPU` | Unmodified programs handle budget limits the way they already handle `RLIMIT_*` | LOW-MEDIUM | B2 already charges the faulting agent for CoW pages. The mapping is the new part. `EAGAIN` for `fork` is POSIX-defined. |
| Kernel-stamped credential visible to servers and propagated in audit records: vfs and procmgr log (agent_id, on_behalf_of, pid, path/op, result) | Attribution without trusting claimed identity (option-b section 2.1) | MEDIUM | An audit ring in the servers is enough for B3. Broker integration is B4. |
| Scoped PID namespace per domain: `kill` and `waitpid` only see processes of the same domain or caps held; cross-domain `kill` returns `ESRCH` (or `EPERM` if the target is known to exist via a held cap) | Closes a side channel, mirrors Capsicum (denies global PID namespace) | MEDIUM | Option-b section 3.1 says cross-domain `kill` returns `EPERM`. `ESRCH` is the POSIX answer for "no such process" and leaks nothing; `EPERM` confirms the pid exists. Flag as a decision; do not decide here. |
| Dual-mode conformance: same suite, non-agent vs agent-with-full-grant, scores must match | Proves the coexistence rules instead of asserting them (scoping section 5 rule 1) | LOW (harness) | The grant-mapping work has to be real: full grant means a domain whose mount table is the standard POSIX tree. |
| Process-visible "narrow grant" runs of real tools: `ls`, `grep`, `make` inside a one-directory namespace | Demonstrates rule 4 ("agent-visible files are ordinary files") on the B3 stack without B4 | LOW | Cheap and convincing demo for the roadmap; also the first test that ambient authority did not creep back. |
| `nosuid` namespaces by default; setuid bits ignored (accepted as `ST_NOSUID`) | POSIX-permitted, removes the confused-deputy escalation path | LOW | See Anti-Features: build none of setuid, not just disable it for agents. |
| Per-domain resource accounting readable through ordinary files in the namespace (`/agent/self/budget` style) instead of `/proc` | Agents and humans can inspect without a new syscall | MEDIUM | Defer unless B4 needs it. Listed so `/proc` is not built to get it. |
| fd-as-capability: an fd carries a rights mask (read/write/seek/lock/dup) enforced by the server; `dup` and fd passing can attenuate (Capsicum model) | Natural fit for capability kernel; enables "pass a read-only fd to a subagent" | HIGH | Not required for B3. The Tier 2 OFD design should leave room (rights field in the OFD handle) so it is not a rewrite. |

### Anti-Features (do not build in B3)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| `setuid`/`setgid` executables (exec honoring `S_ISUID`/`S_ISGID`), at least for agents; recommend for everyone in B3 | `su`, `sudo`, `passwd`, `ping` expect it; POSIX defines the bits | It is the confused-deputy mechanism (OSTEP security chapters; scoping section 6). No B3 deliverable needs it. Dropping it for non-agents saves exec-server and procmgr complexity | Mount `nosuid` everywhere. Return the real file mode, ignore the bits on exec. Record as a known non-adherence if an os-test test cares (none seen in the four kernel suites read). |
| Ambient root filesystem / global `/` for agents; a "just mount `/` read-only" shortcut | Real programs open `/etc/passwd`, `/usr/lib`, `/tmp` and fail otherwise | Option-b section 6: a grant of `/` is a bug in the namespace model, not a config change. It silently converts the capability system into a Unix | Give each domain a synthesized root with only what the tool needs (static binary, `/tmp` scratch, `/dev/null`). Fix the program or the grant list, never widen to `/`. |
| Superuser (uid 0) bypass of namespace and mode checks inside vfs | Every Unix admin tool assumes root | uid is a server-side attribute, never authority (Key Decision). A root override in vfs makes uid authority again | uid 0 may bypass mode bits only inside its own namespace, never extend the namespace. For agents, no uid 0 at all in B3. |
| `/proc` and `/sys` | BusyBox `ps`, `top`, `free`, `lsof`; shell `$$`-style scripts read them | Enumerates every process and fd in the system: an ambient observation channel. Not POSIX | `getpid`-style syscalls plus the scoped files in the Differentiators row. Accept that `ps` is limited. |
| Kernel-resident fd table or kernel POSIX objects (Redox style) | Fewer IPC round trips on `read`/`write` | Pulls POSIX identity into the kernel; breaks "kernel has only the section 2 primitives" | Server-held tables; optimize with an in-libc fd cache only after the latency numbers say so. |
| Linux ABI emulation (Linux syscall numbers, `clone`, `epoll`, `inotify`, `/proc/self/*`) | Run unmodified glibc binaries | Becomes a second project (Starnix); moves the score target from POSIX to Linux compat | Static musl/relibc programs built for this OS. Revisit only after B5. |
| Dynamic linking (`ld.so`, `dlopen`) | Run existing shared-library binaries | Needs mmap-of-files with pager hook, relocation, TLS; large; BusyBox does not need it | Static binaries. `dlfcn` os-test tests remain failing and are listed as such. |
| Disk filesystems, block drivers, `fsync` durability, journaling | A "real" OS persists files | Not needed to prove the personality; large driver and consistency scope. `fsync-and-durability` work is scoping 3.6 and belongs when agent state needs it | ramfs/tmpfs from a boot image. `fsync` returns success on ramfs. |
| Sockets, `udp`, `getaddrinfo`, net server | os-test has a 109-test `udp` suite; tools like `wget` | Net server and destination policy are agent features (B4). Excluded from B3 numbers | Return `ENOSYS` / `EAFNOSUPPORT`; report the `udp` suite as not attempted, not failed. Keep the os-test headline denominator honest (it varies 4380-4582 by OS). |
| SysV IPC, POSIX message queues, AIO, POSIX timers, `<spawn>` extras, `wordexp` | Option groups in POSIX | Low payoff for the shell milestone and for the kernel-heavy suites. XSI is 52% on Redox and the overall is still 84% | Stub with correct `ENOSYS`. `posix_spawn` basics only. Pick up per os-test `basic` breadth after Tier 4. |
| SMP | Real hardware | Not in B3 (PROJECT.md out of scope). TLB flush on `cow_clone` doesn't wait for remote CPUs; one run queue | `-smp 1`. If a feature needs it, record as a B3 finding and stop. |
| Process-global shared mutable state in the client (Fuchsia "POSIX Lite" in reverse) | Faster fork by keeping state in libc | Pgrp/session/ctty in distributed places is where conformance leaked on Redox (pty 6%, namespace 21%) and MINIX (process 16%) | One authoritative owner per piece of global state: procmgr for pid/pgrp/session/ctty/uid; vfs for fd, OFD, cwd, locks. |
| Making the broker or LLM part of any syscall path in B3 | "Agents native" | B4 scope; adds latency to every number measured here and breaks the unmodified-POSIX claim | Keep B3 servers deterministic and agent-agnostic apart from the credential read. |

## Feature Dependencies

```
Ring-3 syscall ABI
    └──requires──> (nothing in B3; first item)

Object::AddressSpace + Object::Thread caps ──requires──> Ring-3 syscall ABI
Per-process badge on IPC                    ──requires──> Ring-3 syscall ABI
Kernel write breaks CoW (fault-and-retry)   ──requires──> (kernel only; before any fork+read)
Exceptions name faulting thread             ──requires──> Object::Thread

exec server (static ELF)  ──requires──> Object::AddressSpace, Ring-3 syscalls
procmgr (pid, wait, zombie) ──requires──> Object::Thread, per-process badge, exec server
fork                      ──requires──> procmgr, cap-gated cow_clone, kernel-write-CoW fix
posix_spawn               ──requires──> procmgr, exec server    (cheaper than fork; do first)

vfs (ramfs, namespace, OFD, fd table) ──requires──> per-process badge, procmgr pid table
fork's fd-table copy                  ──requires──> vfs OFD refcounts
pipe, devfs, console I/O              ──requires──> vfs
poll/ppoll                            ──requires──> vfs + every server exporting readiness
BusyBox ash (non-interactive)         ──requires──> exec, fork or posix_spawn, wait, vfs, pipe, dup2, libc

signals (kill, sigaction, mask)       ──requires──> procmgr, thread caps, libc trampoline, EINTR protocol
EINTR/restart protocol                ──requires──> IPC cancel (done in B2); precedes vfs blocking ops
SIGCHLD, SIGPIPE, fault signals       ──requires──> signals, B2 exception messages
pgrp/session (setpgid, setsid)        ──requires──> procmgr (state machine incl. zombie/limbo)
ctty, tcsetpgrp, ISIG, job control    ──requires──> pgrp/session, console tty, signals
pty                                   ──requires──> console tty line discipline, ctty
BusyBox ash (interactive, job control)──requires──> ctty, signals, termios

OFD locks                             ──requires──> vfs OFD objects
pthreads                              ──requires──> futex timeouts, per-thread masks, fork-in-MT rules
file mmap                             ──requires──> pager hook or copy-in fallback

os-test CI + allowlists               ──requires──> libc on target, ash/sh to run the harness, cross-compile toolchain
Dual-mode (agent vs non-agent) runs   ──requires──> namespace-aware vfs, credential inheritance
Benchmarks                            ──requires──> fork+exec, posix_spawn, signals, pipes working

namespace-aware vfs  ──enhances──> everything in vfs (path resolution is the enforcement point)
credential inheritance ──enhances──> fork/exec/spawn
Audit log            ──enhances──> vfs, procmgr (reads the kernel-stamped credential)

setuid          ──conflicts──> credential inheritance, ambient-authority rule
root bypass     ──conflicts──> namespace-as-authority
/proc           ──conflicts──> scoped PID namespace
```

### Dependency Notes

- **fork requires the kernel-write-CoW fix:** the child's first `read()` into inherited memory goes through an IPC copy into a CoW page. B2's `VmWriter` errors there. Fixing it later invalidates every vfs read path.
- **vfs requires per-process identity:** the kernel only stamps agent identity. Two forked children in one agent are indistinguishable until a badge or per-process endpoint exists. This bites at the first fork test, not the last.
- **posix_spawn before fork:** spawn needs no AS clone and no fd-table duplication race. ash works on it (BusyBox can be built for NOMMU, which uses vfork-style spawning; confidence MEDIUM, not re-verified). Landing spawn first gets a shell earlier and gives the benchmark a floor.
- **EINTR protocol before servers block:** write the cancel/restart contract once; vfs read on pipes, tty reads, `wait` and `poll` all block.
- **Console tty before real pty, ctty before job control:** one line discipline implementation; pty is a second backend for it.
- **Dual-mode conformance conflicts with ambient authority:** if the full grant ends up being "mount the host tree", the scores will match trivially. The test is only meaningful if the full-grant domain is built from capabilities.
- **setuid conflicts with credential inheritance:** an exec that changes uid is a place for authority to change; with no setuid there is nothing to audit.

## MVP Recommendation

Build order for B3, as phases the roadmap can adopt. Each ends in something runnable.

1. **Kernel enablers (Tier 0):** syscall ABI, AS and Thread caps, cap-gated clone, per-process IPC identity, kernel-write CoW fix, idle task, futex timeouts, teardown that returns budgets. Done when a ktest forks via capability and reads into a CoW buffer through IPC.
2. **Static exec + procmgr + posix_spawn + minimal libc (Tier 1):** pid, wait, zombies, init. Done when a static `hello` and a spawn+wait loop run from a boot image.
3. **vfs + fds + pipes + devfs + serial console (Tier 2, no line editing):** open/read/write/dup/pipe/stat/readdir/chdir. Done when `ash -c 'ls | grep x > /tmp/y'` runs. This is the first BusyBox milestone. Add fork here, with the fd-table copy.
4. **Signals + pgrp/session (Tier 3a):** build `EINTR` contract, `sigaction`/`kill`/masks, `SIGCHLD`, fault signals, `setpgid`/`setsid`/`waitpid(-pgid)`. Wire os-test `process` and `signal` into CI the day they first run.
5. **tty/ctty/termios + interactive ash with job control (Tier 3b), then pty (Tier 4):** wire `pty`.
6. **Score and gates (Tier 4):** OFD locks (`io`), pthreads, libc breadth, os-test allowlists, BusyBox testsuite, benchmark numbers.
7. **Agentic differentiators, threaded through but gated at the end:** namespace-aware vfs is built in phase 3 (not retrofitted). The adversarial credential-inheritance test and the dual-mode suite run land here.

Prioritize: Tier 0, the Tier 2 vfs with namespaces built in from the start, procmgr state machine for pgrp/session/zombie/limbo, and the EINTR contract. Those are the four places retrofitting costs a rewrite.

Defer: pthreads polish, file `mmap`, pty (until console tty works), fd-as-capability rights, scoped accounting files, `/proc`-like introspection.

Risks to the order: if layer cost (fork+exec, signal, pipe) comes out far above Linux after phase 3, the server split is the wrong cut and the fd-table placement is the first thing to revisit; this is the B3 "wrong path" signal in scoping section 7.

## Open decisions flagged for ARCHITECTURE and Corey

1. **Per-process identity on the wire:** badge on endpoint derive, or per-process session endpoint from procmgr? Needs a kernel change either way.
2. **fd table location:** vfs-held (recommended for B3), client-held, or fd=capability. Affects `exec` and the rights-mask differentiator.
3. **libc:** relibc or musl+shim. No evidence in this pass moves the decision. Suggest a two-day spike of `hello`+`fork`+`read` on each.
4. **Errno for read-only grants:** `EROFS` (POSIX) vs `EACCES` (option-b section 3.1 text).
5. **Cross-domain `kill`:** `EPERM` (option-b text) vs `ESRCH` (POSIX; leaks nothing).
6. **os-test target tier (B5):** untouched here; B3 only measures.
7. **Is `posix_spawn` allowed as the primary launch path** in the benchmark, given fork+exec is what the roadmap asks to measure? Measure both and report both.

## Sources

- B2 code and report, this repo: `kernel/src/ipc.rs`, `kernel/src/agent.rs`, `docs/architecture/b2-spike-report.md` (HIGH).
- Repo research docs listed at the top (cite their upstream URLs there; HIGH for repo contents, as secondary as their sources).
- Sortix os-test source, cloned 2026-10-23 from https://gitlab.com/sortix/os-test: test file names under `process/`, `signal/`, `pty/`, `io/` (HIGH for what is tested; test counts are file counts, scored counts per https://sortix.org/os-test/ differ slightly by OS, HIGH).
- Per-suite scores for Sortix and Redox: https://sortix.org/os-test/ (read 2026-10-23, HIGH as published figures, MEDIUM as a basis for targets because os-test is a comparison tool, not a certification suite).
- BusyBox/ash runtime needs (fork or vfork, `waitpid`, `pipe`, `dup2`, `sigaction`, `setpgid`, `tcsetpgrp`, termios): prior knowledge of BusyBox, not re-verified against source in this pass (MEDIUM).
- No research-plan/provider seam was run; the question was answerable from repo docs plus the os-test clone. No external search results are cited.
