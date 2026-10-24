# Pitfalls Research

**Domain:** POSIX personality (procmgr, vfs, exec, libc, BusyBox, os-test CI) on a Rust capability microkernel built on Asterinas OSTD 0.19.0
**Researched:** 2026-10-23
**Confidence:** MEDIUM. Repo facts (B2 gaps, baselines) are HIGH. Cross-system failure patterns come from the repo's own survey plus a few web checks (MEDIUM). Several pitfalls are inferred from B2's code shape and marked *(interpretation)*; none of those have been exercised.

Phase labels are placeholders until the roadmap exists:

| Label | Scope |
|-------|-------|
| **P1** | Ring-3 syscall surface and kernel gaps: syscalls, `Object::Thread`, `Object::AddressSpace`, timers, death notification, idle task, teardown, user-copy CoW fix |
| **P2** | procmgr + exec + fork/wait/kill/signals, driven by a hand-written test binary with no libc |
| **P3** | vfs + fd tables + pipes + pty/sessions/ctty |
| **P4** | libc port + BusyBox |
| **P5** | os-test CI, allowlists, latency measurement. The harness skeleton starts in P2; P5 is where the numbers get published. |
| **X** | Cross-cutting (SMP trigger, OSTD upgrades, authority audit) |

Repo doc references: `b2` = `docs/architecture/b2-spike-report.md`, `optB` = `docs/architecture/option-b-microkernel.md`, `mkp` = `docs/architecture/research/microkernel-posix.md`, `rka` = `docs/architecture/research/rust-kernels-and-adherence.md`.

## Critical Pitfalls

### Pitfall 1: Blocking IPC cancel without a server-side cancel protocol (EINTR double-effect)

**What goes wrong:**
B2 can abort a *client* blocked in IPC with `Interrupted` (`b2` row "Thread interrupt and redirect"). That says nothing about the server. A signal lands while a client waits for `read`, `wait4` or `write`. The client returns EINTR, but the server has already consumed pipe bytes, reaped the zombie, or written half the buffer. The data or the child's exit status is gone. The opposite failure also happens: the server replies to a client that no longer waits, and the reply either blocks the server (bounded queue, `b2` "IPC endpoints") or lands in the next call's reply slot.

**Why it happens:**
The kernel half of signals is done, so it looks finished. Hurd needed a whole `intr-msg` layer for exactly this (`mkp` section 6, item 3, and section 8.2 item 4). B2's cancel is "designed in" at the kernel level but the *protocol* above it does not exist yet.

**How to avoid:**
- Define the cancel contract once in P1 and write it into every server protocol: a request is either committed (reply carries the result, including partial counts) or not started. Servers never leave a half-done non-idempotent op.
- Replies go to a one-shot reply capability. A reply to a dead or cancelled waiter is dropped by the kernel, never queued and never blocking the server.
- For `wait4` and pipe `read`, the server must park the *request*, not hold a thread. A cancel message (or reply-cap close) removes it from the park list.
- SA_RESTART lives in libc: retry only if the server reported "not started".

**Warning signs:**
- A server thread blocked forever on a reply send.
- A shell that loses a line of pipe output after Ctrl-C.
- `wait4` returning ECHILD after an EINTR'd `wait4` (zombie reaped, status lost).

**Phase to address:** P1 (reply-cap semantics, drop-on-dead), P2 (wait/kill), P3 (pipes).

---

### Pitfall 2: Interrupt-vs-block and wake races that the test suite cannot see

**What goes wrong:**
The "a wake that raced the interrupt wins" re-check in `futex::wait` can be deleted and 27/27 ktests still pass (`b2` "Verified", "Not caught"). Every B3 blocking call (wait4, read on pty, poll, sigsuspend, pause) repeats the pattern. Each new instance can hide the same lost-wakeup or lost-signal bug. On `-smp 1` with tick preemption, the race window is a few instructions wide. It will show up as one os-test hang per few hundred runs, and then it will be blamed on QEMU.

**Why it happens:**
The B2 tests are deterministic and single-CPU. The report says outright the window "is reasoned about, not tested." B3 multiplies blocking sites by about ten.

**How to avoid:**
- Build one shared "block until condition or interrupted" helper in P1 and forbid hand-rolled wait loops elsewhere. The re-check lives in one place.
- Add a test-only fail-point at the race window (yield or tick-injection between the condition check and the sleep). Make a ktest that fails when the re-check is removed. B2 already used mutation checks ("can they fail?"); repeat that for the helper.
- Run signal and wait tests in a loop (N=1000) in CI. A flaky pass is a failure.

**Warning signs:**
- Intermittent hangs that disappear under `-d` tracing.
- A test that passes alone and hangs in the full suite.
- A second hand-written wait loop appearing in a PR.

**Phase to address:** P1 (helper plus fail-point tests), re-verified in P2.

---

### Pitfall 3: Signal redirect that corrupts user state (registers, FPU, rdi) or breaks `kill(self)`

**What goes wrong:**
B2 redirect "rewrites rip/rdi" (`b2` table row 5). Two failures follow:
1. The handler trampoline clobbers `rdi` and the SSE/x87 state of the interrupted code. musl's `printf("%f")`, `memcpy` and `qsort` use xmm registers. A signal in the middle of float code returns wrong values only under load. B2 states FPU/SIMD user state is untested and that OSTD may not enable FPU state for user mode.
2. `kill(getpid(), sig)` must deliver before `kill` returns (POSIX, unblocked, calling thread). The path is `kill -> IPC to procmgr -> kernel redirect -> reply`. If the redirect is applied lazily, the "up to one timer tick" latency (`b2` Friction 3) breaks the ordering. os-test signal tests check ordering, not latency.

**Why it happens:**
The redirect was built to prove the kernel can do it. A POSIX signal frame needs the full user context saved and restored by `sigreturn`.

**How to avoid:**
- Redirect only jumps to a libc trampoline. The trampoline (not the kernel) pushes the complete saved context, including xsave area, flags and rdi, and `sigreturn` restores it. Redox does this with a control block shared between kernel and libc (`mkp` section 1, item 3, `INHIBIT_DELIVERY`). Copy the idea.
- Add a ktest: spin loop that holds known values in rdi, xmm0 and flags; redirect it; return; assert nothing changed.
- For self-targeted `kill`, apply the redirect on the return path of the thread that is already in the kernel. Do not rely on the timer poll.
- Decide in P1 whether user-mode FPU is on at all. If OSTD does not save it per task, find out now, not when `printf("%f")` breaks.

**Warning signs:**
- Floating-point test failures that depend on timing.
- `signal/` os-test cases failing only for the "raise" or "kill self" variants.
- A trampoline in the kernel crate.

**Phase to address:** P1 (FPU decision, redirect contract), P2 (frame and sigreturn).

---

### Pitfall 4: Kernel/server writes into CoW pages fail right after the first `fork`

**What goes wrong:**
`VmWriter` user copies are fallible and skip the CoW path (`b2` Friction 1, "inferred from the API and not exercised"). After `fork`, every writable page in parent and child is read-only CoW. The first `read(fd, buf, n)` into a stack or heap buffer asks the kernel to copy into such a page. The result is a spurious EFAULT in the *first command the shell runs*. It looks like a libc bug.

**Why it happens:**
All B2 CoW tests write from ring 3, which takes the fault path. Kernel-originated writes (IPC payload delivery, `read` replies, exec argv setup, signal frames) are a different path.

**How to avoid:**
- In P1, build one `copy_to_user` that on a write failure breaks CoW for that range, then retries. Route every kernel-to-user write through it.
- Add a test now, before procmgr: clone, then deliver an IPC message into a CoW buffer in the child. This is the regression test that fails today.
- Server-to-client bulk data should go through a shared frame or the same helper, not ad hoc.

**Warning signs:**
- EFAULT from the first read after fork.
- `copy_to_user` implemented twice.

**Phase to address:** P1.

---

### Pitfall 5: Teardown gap becomes a per-fork leak that kills the os-test run

**What goes wrong:**
Budget carved for a child is never returned and caps held by a dead agent are never cleaned (`PROJECT.md` Context, `b2` "Teardown"). In B3 that applies to *processes*: cspace slots, endpoints, forwarder chains, reply caps, futex waiters, thread objects, the address space. os-test and BusyBox scripts fork thousands of short-lived processes. The system hits ENOMEM or cspace exhaustion partway through the suite. The failure is blamed on a test, and the allowlist logic then records it as a regression.

Futex has its own variant: a page unmapped between key lookup and table lock "leaves a waiter on a dead frame" (`b2` "Futex"). With process exit as the normal case, this stops being rare.

**Why it happens:**
Teardown was the right thing to skip for a spike. fork/exit/wait turns it into the main loop.

**How to avoid:**
- Make "exit releases everything" a P1 exit criterion, with a leak test: fork+exit N=10,000 times and assert the budget, cspace occupancy and frame count return to baseline.
- Process exit is one kernel operation ("destroy this process's objects") plus a death notification (Pitfall 6). Do not distribute the cleanup across the servers' goodwill.
- Futex waiters on an exiting space are woken with an error in the same operation.

**Warning signs:**
- Free-frame count drifts across a loop of `/bin/true`.
- Late-suite os-test failures that pass in isolation.
- ENOMEM on the Nth fork where N is stable.

**Phase to address:** P1 (kernel destroy plus the leak test), P2 (exit/wait wiring).

---

### Pitfall 6: Servers cannot learn that a client died

**What goes wrong:**
vfs holds an open-file description and fd table per client. pty holds session state. procmgr holds the process record. When a client dies (exit, kill -9, exception with a kill reply), nothing tells the servers. Open-file descriptions never close, so a pipe write end stays open and the reader never sees EOF. A shell pipeline hangs forever. Ctrl-D and SIGHUP never fire.

**Why it happens:**
The B2 kernel has no death notification and no thread or process capability type (`b2` "Exception messages do not name the faulting thread"). Microkernel POSIX stacks need one: MINIX PM notifies VFS explicitly (`mkp` section 5, item 1, `VFS_PM_FORK` and friends), seL4 and Mach have notifications and dead-names.

**How to avoid:**
- Add a kernel "agent/process exit" notification to P1, delivered to a registered endpoint. Order is a design decision to make explicitly: kernel tears down, then notifies procmgr, then procmgr fans out to vfs and pty. The kernel does not reference the servers.
- procmgr is the only party that knows what died. vfs and pty trust procmgr's notification, which is a stamped message from a known credential.
- The pipe-EOF-on-writer-death test belongs in P3's acceptance. Run it with SIGKILL, not only a clean exit.

**Warning signs:**
- `cat | head` hangs.
- Open-file-description counts only ever grow.
- A server polling for client liveness.

**Phase to address:** P1 (notification primitive), P2 (fan-out), P3 (acceptance).

---

### Pitfall 7: Session, controlling terminal and pty split across three servers with no owner

**What goes wrong:**
This is the known failure: Redox pty 6% (2/29), MINIX 34%, process-global state spread across servers (`mkp` section 7 and 8.4). The os-test pty tests exercise exactly the cross-server paths: `tiocsctty` (child `setsid`, open pty, `TIOCSCTTY`), `tcgetsid`, and `tcgetsid-wrong-controlling` (a second opener with `O_NOCTTY` must not get the controlling tty). The same state is touched by:
- implicit ctty acquisition when a session leader opens a tty without `O_NOCTTY` (vfs/pty must tell procmgr);
- `SIGHUP` to the session on leader exit;
- `SIGTTIN`/`SIGTTOU`/`SIGTSTP` for background jobs;
- orphaned process group rules.
If procmgr owns sessions and pty owns terminals with no stated rule for who is authoritative, each server grows a copy and the copies drift. MINIX duplicates process state between PM and VFS (`fp_tty` in vfs vs sessions in PM, `mkp` section 5, item 2).

**Why it happens:**
Sessions look like a procmgr feature and terminals look like a device feature. The coupling only shows up in tests.

**How to avoid:**
- Write the ownership rule before any code: procmgr owns the session-to-ctty link and each process's pgrp. pty owns the tty-to-session-id label. Every access check ("is this the ctty?") is a call to procmgr with the stamped credential, or a cached value invalidated by a procmgr push.
- Put the controlling-terminal protocol (acquire, release, hangup, tcgetpgrp/tcsetpgrp, job-control signals) in a design doc for P2, even though pty ships in P3.
- Have the os-test `pty` suite running in CI from the first pty build. `optB` section 6 says this already ("Test them from the first procmgr build, not last"). The mistake is writing pty last because it is boring.
- Report `process`, `signal`, `pty` separately and never fold them into the headline (Pitfall 12).

**Warning signs:**
- A tty or session field in two servers' structs.
- pty tests at 0% for weeks because "pty isn't built yet".
- A `setsid`/`setpgid` stub returning success.

**Phase to address:** P2 (ownership rule, pgrp/session), P3 (pty, ctty, job control).

---

### Pitfall 8: `wait`/SIGCHLD/job-control state machine with stale queue entries

**What goes wrong:**
Redox's relibc hit a `waitpid` deadlock: a SIGCONT sent to a new child left a stale `(pid, 65536)` entry in the parent's waitpid queue, and when the child changed pgid (as the Ion shell does) the exit's pgid message was dropped. Fix: drain the stale entry with `WUNTRACED|WCONTINUED` in the fork path. Source: relibc commit "Fix waitpid deadlock" (https://gitlab.redox-os.org/plimkilde/relibc/-/commit/24ecd6ee16660651881a7ff218dc5293b95d7a26). That is a symptom of modelling wait state as a queue of events instead of derived state. The same family covers reparenting to init, `SA_NOCLDWAIT`, `SIGCHLD` ignored vs default, `waitpid(-pgid)`, and zombies whose parent dies.

**Why it happens:**
Wait looks like "return the exit status". The POSIX rules over groups, stopped/continued reports and reparenting make it a state machine with many transitions.

**How to avoid:**
- Keep procmgr's process table as a pure Rust library (no kernel types) with a model-based test: random sequences of fork, exit, stop, continue, setpgid, setsid, kill, wait; assert invariants (no zombie without a waiter or reparent; every `wait4` eventually returns when a child exists). It runs under host `cargo test` in seconds and avoids the Docker/QEMU loop.
- Store state, not events: child status is a field; `wait` scans.

**Warning signs:**
- `wait` implemented with a channel or queue.
- Shell hangs after a job-control sequence (`sleep 1 &`, `fg`, Ctrl-Z).

**Phase to address:** P2.

---

### Pitfall 9: Fork is a multi-server transaction with no rollback, and the cost is never measured

**What goes wrong:**
fork needs: allocate pid (procmgr), clone the address space (kernel), duplicate the fd table and bump open-file-description counts (vfs), copy signal dispositions and masks, create the child thread, install its cspace. A failure in the middle leaves a half-built process. MINIX's PM comment says it must not fail a fork after `vm_fork` (`mkp` section 5, item 1). Multi-threaded parents add a second problem: the child has one thread, but locks held by other threads in libc state (malloc, stdio) stay locked. Redox needed allocator locking fixed around fork (relibc PR 986, https://code.redox-os.org/redox-os/relibc/pulls/986); QNX documents `forksafe_mutex` and atfork (`mkp` section 4, item 1).

Cost is the second half. No surveyed system publishes POSIX-layer cost (`mkp` section 9 of the matrix and section 8.4; Hurd warns without numbers). fork here is at least procmgr + vfs + kernel round trips, plus a page-table switch per hop. Linux numbers on the same hardware are a project requirement, and the measurement has not been designed.

**Why it happens:**
It is natural to write fork happy-path first and to measure last.

**How to avoid:**
- Order the operations so that failure is cheap to undo: reserve everything that can fail (pid, budget, cspace slots) first; do the irreversible kernel clone last; make vfs's "clone table" a prepared-then-commit pair.
- Measure the primitive early. As soon as two processes exist (P2), time a null IPC round trip and a bare `cow_clone`. Then `fork+exec` is a predicted sum before it is a measurement; a gap between the sum and the measurement points at the hidden cost.
- Give the libc fork path an atfork hook for allocator locks from day one. musl takes its malloc locks around fork; the shim must not bypass that.
- Also provide `posix_spawn` as a direct procmgr request (new AS + exec in one call, no CoW clone). Most BusyBox and shell exec paths can use it. Note that musl's `posix_spawn` uses a `clone(CLONE_VM|CLONE_VFORK)` internally (*interpretation*, check against the chosen musl version), so the shim must map that or replace it.

**Warning signs:**
- A fork failure path that is a `todo!()`.
- Benchmarks planned "after BusyBox works".
- A fork microbenchmark that includes `exec`, so it cannot be decomposed.

**Phase to address:** P2 (transaction ordering, early primitive timings), P4 (atfork, posix_spawn mapping), P5 (published numbers).

---

### Pitfall 10: Server work charged to the wrong CPU budget (stall or ambient CPU)

**What goes wrong:**
The B2 scheduler charges the task running when the tick lands (`b2` "CPU budget precision"). procmgr and vfs are tasks belonging to some agent. Two failures:
1. The servers' agent has a finite quota carved from root. When it is exhausted for the period, every client's fork/open/read stalls until the next period. The whole system appears to hang for a fraction of 10 ticks at a time.
2. To fix (1), servers are made budget-free. Now any agent can burn unlimited CPU by calling procmgr or vfs in a loop, and the "CPU budget per agent" claim is false for anything that touches a server.

The reverse: a budgeted thread with nothing else eligible keeps the CPU, and a real idle task is a "B3 item" (`b2` bullet 3). Servers blocked on IPC plus all clients out of budget leaves the pick-next function with nothing legal to return.

**Why it happens:**
B2 had two spinning agents and no servers. Budget donation across IPC is the known hard part of seL4 MCS-style designs, and it is not in the B2 design.

**How to avoid:**
- Decide in P1: servers are budget-exempt TCB agents in B3, and the per-request accounting goes to a ledger by stamped credential (CPU time or request count). State plainly in the docs that kernel-enforced CPU limits do not cover server-side work in B3. Do not claim otherwise in the core-value statement.
- Build the idle task in P1 (not a never-budgeted busy task; a task that halts). Add the ktest `b2` says is missing for the integrated scheduler, or at least extend the boot check to include a server and two clients.
- A test: one agent with quota 1/10 hammers procmgr; assert other agents' fork latency stays bounded.

**Warning signs:**
- Periodic latency spikes of exactly one budget period.
- A boot check that passes with no servers present.
- `pick_next` returning `None` in a panic message.

**Phase to address:** P1 (decision, idle task), P2 (first server exercises it).

---

### Pitfall 11: Authority leaks through the servers (confused deputy and ambient namespaces)

**What goes wrong:**
The kernel stamps the *sender's* credential. When a client calls vfs and vfs calls the pty server, pty sees vfs's credential, not the client's. A server that acts for a client with its own broad authority is the classic confused deputy. Specific leak points in this design:
- `kill(pid)`: a global pid space is a global namespace. A cross-domain `kill` returning EPERM confirms the pid exists. `optB` section 3.1 says EPERM; for processes outside the caller's domain, ESRCH reveals nothing and is allowed by POSIX *(interpretation)*.
- exec server holds read access to every ELF, and procmgr holds thread and address-space capabilities for every process. They are the TCB; their request handlers must authorize by the stamped credential and the target's domain, never by a pid in the message.
- `/tmp`, `/dev/ptmx`, `/dev/null`, `/proc`-like files: shared names between domains allow squatting and covert channels.
- "The shell needs `/` to work, so grant it" is the creep `optB` section 6 names. In practice it arrives as an exec search path, `getcwd`, or `cd ..` above the grant root.
- uid/gid: vfs must learn the caller's uid. If vfs asks procmgr per open, every open costs a round trip; if vfs caches, `setuid` makes the cache stale. MINIX holds this split in PM and VFS with explicit sync messages (`mkp` section 5, items 2 and 6).

**Why it happens:**
POSIX semantics assume a global kernel with ambient identity. Each server is written to "just work", and the first fix for a permission failure is to widen a grant.

**How to avoid:**
- Pick the delegation pattern in P1/P3 and use it everywhere: per-client badged session endpoints (seL4 style). vfs mints a badged endpoint per opener; downstream servers read the badge, which vfs cannot forge into another client's. Never carry "on behalf of" as an unauthenticated message field.
- Pid tables are per domain. A pid not in the caller's table is ESRCH.
- procmgr pushes credential changes to vfs (versioned record), instead of vfs pulling per call.
- Standing acceptance test in every phase from P2 onward: a process in domain A cannot name, signal, wait on, or open anything in domain B, and a plain (non-agent) process sees an unmodified system. If the fix to a failing POSIX test is a wider grant, the test result is "bug in the namespace model" and is logged as such (project rule from `optB` section 6).

**Warning signs:**
- A grant list that grows to include `/`.
- A server handler taking a `pid` or `uid` argument and using it for authorization.
- `ENOENT` and `EACCES` used interchangeably in a way that leaks existence.

**Phase to address:** X (policy, audited at each phase boundary), P2 (procmgr authorization), P3 (vfs badge scheme).

---

### Pitfall 12: The os-test headline measures the libc, and the allowlist rots

**What goes wrong:**
- `include`, `namespace` and `limits` mostly test headers (confirmed from the os-test page: header declarations, namespace pollution, `limits.h` values). A libc swap moves the headline without any kernel change. Reported ranges: include 77-97%, namespace 0-98%, limits 89-100% across systems. musl on Linux scores 94% overall (`rka` Part 3), which is the ceiling.
- Failure attribution is harder with a userspace-heavy libc. Redox's own write-up says a conformance failure "could originate in libc, the kernel, or the Process Manager" (https://www.redox-os.org/news/posix-conformance-testing/).
- Denominators move between systems (4380 to 4582) because neutral outcomes are dropped. A pass-rate can rise because tests moved to neutral. Compare percentages, never counts, and key allowlists on *test identity plus expected outcome*.
- os-test is a moving target (POSIX.1-2024, active development; Redox's published results are per dated build). "Only grows" conflicts with bumping the suite.
- os-test's runner expects ssh to a target (os-test page). There is no ssh here, tests hang rather than fail, and QEMU timing makes borderline tests flaky.

**Why it happens:**
One number is attractive, and CI wiring is left until the system is "ready".

**How to avoid:**
- Report a table per suite from the first run: overall; kernel-heavy (`process`, `signal`, `pty`, `io`, plus `basic`); libc-bound (`include`, `namespace`, `limits`). Keep the kernel-heavy composite as the number that gates milestones.
- Run the same pinned os-test commit against musl on Linux (the 94% row, reproduced locally) as the control. The delta between control and B3, per suite, is the kernel's share.
- Pin the os-test commit. A bump is a separate change that re-baselines the allowlist with a recorded diff.
- Per-test timeout with the host watchdog around QEMU; a timeout is a distinct outcome ("hang"), never silently a pass or an unlisted fail. Hangs count against the allowlist.
- Quarantine flaky tests explicitly (listed, expiring) rather than loosening the gate.
- Stand up the harness in P2 with a trivial test set so the serial-console runner, result collection and timeout logic exist before the 4,500 tests do. Ask whether CI has `/dev/kvm` before assuming (`PROJECT.md`: KVM host required).

**Warning signs:**
- A single percentage in the README.
- Headline jumps with no kernel commit.
- "Allowlist only grows" being violated by a bump with no diff.
- Test results that differ between two runs of the same build.

**Phase to address:** P2 (harness skeleton), P5 (reporting and gating). Decision on libc (Pitfall 13) must land before P5 is meaningful.

---

### Pitfall 13: Libc choice made after the protocols are frozen (fd table placement, Linux-shaped syscalls)

**What goes wrong:**
Open question B3 (relibc vs musl+shim) is still open. The server protocols drift toward whichever libc is in the developer's head. Specific traps:
- **musl+shim:** musl's syscall interface is Linux-shaped and assumes a kernel-resident fd table and per-process state. Where the table lives is the decision. If the table is in libc memory (Fuchsia fdio, Hurd, Genode style, `mkp` section 8.1), `exec` destroys it, so exec must marshal the table (and CLOEXEC flags) to the new image. If it lives in vfs (MINIX style), every `dup2`, `fcntl(F_SETFD)`, and `close` is a round trip, but exec and fork are simple. Neither is wrong; deciding by accident is.
- musl's `pthread_create` and `pthread_join` use `clone` flags, `set_tid_address`, and a kernel-side clear-and-wake of the tid word at thread exit (`CLONE_CHILD_CLEARTID`) *(interpretation: verify against the musl version)*. B2's futex has no exit-time clear-and-wake, and no thread capability type (`b2`).
- **relibc:** `redox-rt` does fork, exec and signals against Redox's kernel and scheme paths. Its os-test ceiling is "unknown apart from the kernel" (`optB` section 3.2). Porting it means rewriting `redox-rt`'s platform layer, not configuring it.
- Dynamic linking pulls in file-backed `mmap` and a pager. Static BusyBox avoids that for B3.

**Why it happens:**
The servers are built first because the kernel work is fresh, and the libc is "just the client".

**How to avoid:**
- Run a time-boxed libc decision spike at the end of P2 (hand-written test binaries in P2 already need a tiny runtime). Criteria: where does the fd table live, what does `clone` map to, can `pthread_join` work with B2's futex plus a thread-exit clear-and-wake.
- Define server protocols at POSIX-operation granularity (`open`, `read`, `kill`, `wait4`, `spawn`), not Linux-syscall granularity (`clone` flags). Then swapping libc changes only the shim.
- Record the decision as an ADR with the evidence, because it sets the os-test ceiling.
- Prefer static linking and no file-backed mmap for B3.

**Warning signs:**
- A server request named after a Linux syscall number.
- An exec implementation that does not mention the fd table.
- `pthread_join` hanging on exit.

**Phase to address:** end of P2 (decision), P4 (port).

---

### Pitfall 14: exec as "load an ELF" when it is "replace the image of a live process"

**What goes wrong:**
`exec` keeps pid, ppid, pgrp, session, fds without CLOEXEC, signal mask and pending signals; resets caught handlers to default; kills sibling threads; ignores setuid under `nosuid`. It must happen atomically with respect to signals and to other threads. The calling thread is blocked in IPC to the exec server while its own address space is being replaced: who unmaps its stack, and what happens if a signal redirect arrives mid-swap? B2's `VmSpace` is attached per task, and there is no capability-gated "replace this thread's address space" operation (`b2`: `cow_clone` takes a `&VmSpace`, no address-space object type). The exec server also must map into a fresh space it does not own, so it holds a powerful map capability to it.

**Why it happens:**
ELF loading is the visible work and is easy to demo (spawn a new program). The replace-in-place semantics only show up with a shell running `exec`, or with a failing exec that must return ENOEXEC with the old image intact.

**How to avoid:**
- Split exec into: exec server builds a complete new space (ELF segments, stack with argv/envp/auxv) and returns an address-space capability; procmgr (or a kernel op) does the atomic swap and resets register state; failure before the swap leaves the old image untouched.
- The swap is a P1 kernel primitive (`Object::AddressSpace` with rights, and `swap_into(thread)`), alongside the thread capability.
- Test list for P2: exec failure returns to old image; fd inheritance and CLOEXEC; signal disposition reset; exec from a multithreaded process; exec while a signal is pending.

**Warning signs:**
- exec implemented as spawn+exit.
- A failed exec killing the process.
- An exec server holding a long-lived capability to every process's address space.

**Phase to address:** P1 (swap primitive, address-space capability), P2 (exec server).

---

### Pitfall 15: Timeouts and timers are a hidden prerequisite everywhere

**What goes wrong:**
B2 has no timeouts: futex `WAIT` with a timeout returns `ENOSYS` (`b2` "Futex"), and IPC receive has no timeout. B3 needs timers for `sleep`, `nanosleep`, `alarm`, `poll`/`select` with timeouts, `pthread_cond_timedwait`, `waitpid` with `WNOHANG` polling loops in BusyBox applets, `SIGALRM`, and the host-side hang watchdog's inner timeouts. Because B2's CPU quota is also tick-based, a tick-granularity timer is easy and the finer one is not, and the measurement requirement (latency vs Linux) cannot be met with 10 ms ticks.

**Why it happens:**
Timeouts are plumbing, so they get deferred until a BusyBox applet needs one. By then four blocking paths exist and each needs retrofitting.

**How to avoid:**
- Put a timer-wheel (or deadline-sorted list) and a `block_until(condition, deadline)` in the P1 shared wait helper (Pitfall 2). Every blocking call has the deadline parameter from the start.
- Use TSC reads from ring 3 for latency measurement; do not measure with tick counts.

**Warning signs:**
- A `sleep` implemented as a busy loop with `sched_yield`.
- A new `ENOSYS` for a timeout flag.

**Phase to address:** P1.

---

### Pitfall 16: OSTD upgrade breakage in the code that copies OSTD's undocumented scheduling protocol

**What goes wrong:**
The custom scheduler repeats OSTD's handshake on `schedule_info().cpu`, and relies on "pick_next must never return the running task or `None` after `update_current` asked for a switch" read from source (`b2` Friction 4). The signal redirect rests on `UserModeHooks::has_kernel_event`, and the OSTD 0.19.0 release notes list "Introduce `UserModeHooks`" (PR 3325), so that API is one release old and has no track record. Release cadence is roughly every 2-3 months (0.17.0 2025-12-17, 0.17.1 2026-03-12, 0.17.2 2026-05-09, 0.18.0 2026-06-03, 0.18.1 2026-08-18, 0.19.0 2026-10-12; `gh api repos/asterinas/asterinas/releases`). Earlier notes show OSTD-level refactors that would touch this project: floating-point context handling in context switching and signal handling, `iret` vs `sysret` selection, a race between enabling IRQs and halting the CPU (0.17-era), `Pod` rewritten on zerocopy and DMA API refactor (0.18.0). `RELEASES.md` does not mark which changes are breaking, and there is no API stability statement (`rka` Part 1, `b2` Friction 5).

**Why it happens:**
It is tempting to follow the release train for bug fixes, particularly the SMP-related ones.

**How to avoid:**
- Freeze at 0.19.0 for the whole of B3. Upgrade only at a phase boundary, as its own task, if a needed fix is only available upstream.
- Treat the B2 ktests and boot check as the upgrade contract. Add one test for each undocumented scheduler-protocol assumption (task woken before it finished switching out; `pick_next` never returning the running task), so an upgrade fails loudly instead of silently misbehaving. The `b2` Friction 4 list is already the test list.
- Keep the Docker image and Rust toolchain pinned together (`kernel/Dockerfile`). Record the toolchain, not just the crate version, in each phase's verification.
- If a needed change requires `unsafe` in the kernel crate or an OSTD patch, that is the recorded signal to re-score the Redox fork (`PROJECT.md` Constraints). Check before each phase whether any planned work trips it.

**Warning signs:**
- A `cargo update` in a feature branch.
- A scheduler that "mostly works" after a bump.
- An ostd API call that was added in the last release.

**Phase to address:** X (policy now), re-checked at each phase boundary.

---

### Pitfall 17: SMP retrofit when `-smp 1` assumptions are spread through new servers

**What goes wrong:**
SMP is out of scope, but three B2 shortcuts are SMP-shaped, and B3 builds on top of them: one run queue pinned to the boot CPU; one global futex table lock; `cow_clone` TLB flush dispatched "but never waits for remote CPUs" (`b2`). Code written under `-smp 1` bakes in assumptions: lock ordering between procmgr data and kernel state is never exercised, "I hold the CPU so nobody interleaves" reasoning appears in comments, and the TLB-flush hole is a security bug (a stale writable mapping after fork), not a perf bug. The B2 report names SMP as the place OSTD "could force a patch."

**Why it happens:**
Single CPU makes everything easy to pass. The decision to stay `-smp 1` is sound for a solo research project, but it needs an explicit trigger.

**How to avoid:**
- Write the trigger down: SMP is revisited only if B3 forces it. The candidates that would force it are (a) a pthread test (os-test TPS/TSH, the BusyBox testsuite) that needs true parallelism, (b) a measured server bottleneck. Everything else stays `-smp 1`.
- Rule for new kernel code: locks follow a documented order; no code relies on "single CPU" for correctness without a `// single-cpu:` marker that grep can list, same as the existing `shortcut:` comments.
- Do not extend the cow_clone path without fixing the remote TLB wait first if SMP is ever on.
- One cheap probe, if the budget allows near the end of B3: boot the existing tests with `-smp 2` with everything pinned to the boot CPU, to see whether OSTD even brings up the second CPU cleanly with the custom scheduler. This costs a day and turns a guess into data.

**Warning signs:**
- Comments saying "safe because one CPU".
- Server code that holds a lock across IPC.

**Phase to address:** X (marker convention now, probe late in P5).

---

### Pitfall 18: Pipe and read/write throughput limited by two copies and a page-table switch per hop

**What goes wrong:**
Every `read`/`write` becomes client -> kernel -> server and back. The B2 scheduler activates the incoming task's `VmSpace` after every context switch (`b2` row 6). Whether that includes address-space tagging (PCID) is not known; without it every IPC hop flushes the TLB *(interpretation, check OSTD's `VmSpace::activate`)*. A shell pipeline (`cat | grep | wc`) is therefore many round trips per 4 KiB. The bounded message queue (`b2` "IPC endpoints") limits message size, and large `read` calls have no bulk path.

**Why it happens:**
The IPC was designed for control messages. Bulk data was never a B2 requirement.

**How to avoid:**
- Design the IPC message format in P1 with a bulk path in mind: a message can carry a frame capability or a mapping grant, with the copy path as the fallback. It does not need to be built in B3, but the format must not preclude it.
- The pipe implementation is a P3 decision: server-resident buffer first; measure; shared-ring optimization only if the measured number demands it.
- Measure pipe latency and throughput as soon as pipes exist (P3), not in P5.

**Warning signs:**
- A hard-coded small message size in the wire format.
- Pipe throughput under 1% of Linux's in the first measurement (target is not set; set it as the baseline run, not a guess).

**Phase to address:** P1 (format), P3 (measure), P5 (publish).

---

### Pitfall 19: Latency comparison vs Linux that is not comparing the same thing

**What goes wrong:**
The requirement is "fork+exec, signal round-trip and pipe latency measured against Linux on the same hardware" (`PROJECT.md`). Linux native and this kernel in QEMU/KVM are different. KVM exits (timer, APIC) distort short intervals, 10-tick budget periods are not a fair tick-rate, and a loop measured in ms cannot resolve a µs operation.

**How to avoid:**
- Run Linux as a guest in the same QEMU/KVM configuration (same host, same `-cpu`, same memory) as the baseline. Report both. Native-Linux numbers only as an extra row.
- Time with `rdtsc` in the process; report median, p99 and N; discard warm-up.
- Fix the benchmark source (same C program, same libc where possible; musl on Linux is the natural control) before the first run.
- State before running what number would mean the layer design is wrong (the project's rigor rule): pick the threshold when the primitive timings (Pitfall 9) come in.

**Warning signs:**
- A single run, no variance.
- Different libcs on the two sides.

**Phase to address:** P2 (harness and first timings), P5 (publication).

## Moderate Pitfalls

### Pitfall 20: Exception-message thread identity and process-wide fault routing
**What goes wrong:** Exception messages do not name the faulting thread (`b2`), and only vectors 0-21 and 28-30 are encoded. SIGSEGV for a multi-threaded process routes to the wrong thread, and SIGFPE/SIGILL/SIGBUS are untranslated.
**Prevention:** `Object::Thread` in P1; a vector-to-signal table in procmgr with tests per signal; unknown vectors are fatal with a log line, not a hang.
**Phase:** P1, P2.

### Pitfall 21: Per-process memory syscalls routed through procmgr
**What goes wrong:** `mmap`/`munmap`/`mprotect`/`brk` as IPC to a server makes `malloc` slow, and fault-time behavior (CoW charging the faulting agent) must still hold.
**Prevention:** Give each process a self address-space capability with a restricted rights mask and make map/unmap/protect direct kernel calls on it, charged to the budget. procmgr only handles creation and cloning. Only 4 KiB pages are supported (`vm.rs` asserts it), so check that BusyBox and musl do not need huge-page splits or `MAP_FIXED` overlaps.
**Phase:** P1.

### Pitfall 22: Process-global state in two places drifting (credentials, cwd, umask, pgrp)
**What goes wrong:** vfs needs uid/gid/umask/cwd; procmgr needs pgrp/session; both change via syscalls. Two copies diverge (`mkp` section 8.4).
**Prevention:** One owner per field, written in a table in the P2 design doc, and all others hold versioned copies pushed by the owner. Never "pull on demand" in the hot path.
**Phase:** P2, P3.

### Pitfall 23: Hand-assembled test binaries stop scaling and tests lie about ring 3
**What goes wrong:** B2 tests use a 50-byte hand-assembled program. P2 needs real multi-process scenarios (fork, wait, signals, exec). Hand-assembled programs are a bottleneck, and tests that run in-kernel (calling kernel functions directly, as B2 does for cap and IPC) skip the syscall boundary where validation lives.
**Prevention:** Build a tiny `no_std` Rust test-binary crate (raw syscall stubs) in P1 so P2 tests are written in Rust, built in the Docker image, and loaded by the kernel test harness. Every new kernel operation gets one test through the real syscall entry.
**Phase:** P1.

### Pitfall 24: Syscall boundary validation inherited from kernel-internal tests
**What goes wrong:** The cap and IPC operations are currently kernel functions with typed arguments. Once reached from ring 3 they take raw integers: invalid cap slots, out-of-range rights, user pointers that point into kernel space. The B2 futex path already hit this (`futex_on_a_kernel_address_is_efault_not_a_panic`).
**Prevention:** One decode layer that turns raw registers into typed arguments, with a fuzz-style ktest feeding random register values and asserting no panic. A panic from user input is a kernel bug.
**Phase:** P1.

### Pitfall 25: Allowlist gate only on the pass side
**What goes wrong:** CI fails on regressions but nobody notices tests that moved from "fail" to "hang" or from "pass" to "neutral".
**Prevention:** Gate on all outcomes per test, including timeouts and neutral changes; diff the full result map against the committed allowlist, not just the count.
**Phase:** P5.

## Minor Pitfalls

### Pitfall 26: `exit` is the only user-mode syscall, so scaffolding code lives in tests
**What goes wrong:** helper code written for ktests ends up as the de facto API.
**Prevention:** Delete the test-only paths when the syscall layer lands; keep one entry point.
**Phase:** P1.

### Pitfall 27: BusyBox applets pulling in unexpected kernel features
**What goes wrong:** `ash` wants job control, `/proc`, `ioctl(TIOCGWINSZ)`, `getdents`, `uname`, `utime`, `ls` wants `lstat`/`stat` fields. A defconfig BusyBox is much bigger than a shell demo.
**Prevention:** Start from a minimal `.config` (ash, cat, ls, echo, sh builtins, then grow); gate the list in the BusyBox testsuite expected-fail file (`rka` Part 3), triaged once.
**Phase:** P4.

### Pitfall 28: Documentation drift between wiki claims and B3 results
**What goes wrong:** B3 learns something that contradicts a design claim in `optB` and the doc is never updated (for example, EPERM for cross-domain kill).
**Prevention:** Each phase updates the design doc or records an ADR; claims not stated by a source stay marked *(interpretation)* per repo convention.
**Phase:** every phase, at transition.

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Servers budget-exempt, no CPU donation | procmgr/vfs never stall | CPU limit does not cover server work; DoS via a server loop | Acceptable in B3 if documented and ledgered; revisit in B4 |
| Pipe/read/write by copy through the kernel | Simple IPC | Throughput trap (Pitfall 18) | Acceptable until measured; keep format open to frame grants |
| `-smp 1` with a `// single-cpu:` marker | No SMP work | Retrofit pain; TLB-flush hole stays | Acceptable for B3 with the marker and trigger written down |
| Hand-rolled wait loops in new blocking calls | Quick to write | Lost-wakeup races (Pitfall 2) | Never |
| fd table in libc memory without exec marshalling | Fast `dup`/`close` | Lost fds across exec | Never without the marshalling design |
| uid check by pulling from procmgr per call | Always fresh | Round trip per open | Only until Pitfall 22's push model lands |
| Static-only BusyBox, no file-backed mmap, no pager | Skips dynamic loader | No glibc/dynamic binaries | Acceptable for B3 |
| Carved budgets never returned | Existing behavior | Leak per process (Pitfall 5) | Never past P2 |
| Skipping tests for `pty` until last | Defers the hardest suite | Design lock-in, 6% repeat (Pitfall 7) | Never |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| os-test runner | Expecting ssh to the target; no per-test timeout | Serial-console runner under a host watchdog; per-test timeout recorded as "hang" |
| os-test results | Using counts across OSes | Compare percentages; key allowlists on test + outcome; pin the commit |
| BusyBox/bash testsuites | Counting raw pass/fail | Triaged expected-fail file; failures from GNU extensions and missing `/proc` are not POSIX failures (`rka` Part 3) |
| Docker + QEMU/KVM | CI runners without `/dev/kvm`; running `osdk.sh` outside the image | Verify KVM on the CI host before designing the gate; keep every command through `kernel/osdk.sh` |
| OSTD | Reading scheduling rules from source once | Convert each assumption into a ktest; pin version and toolchain |
| musl on Linux (control) | Skipping the control run | Run the pinned os-test against musl on Linux to calibrate libc-bound suites |
| relibc `redox-rt` | Assuming a config switch | Budget a platform-layer rewrite |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| fork = N server round trips + address-space switches | `fork+exec` far above predicted sum | Time primitives first; `posix_spawn` direct path | First shell script with a loop |
| Page-table switch per IPC hop without tagging | Pipe throughput collapses | Check whether `VmSpace::activate` is TLB-preserving; measure | First `cat | wc` |
| Global futex lock | Contention | Hashed buckets (only matters for SMP) | SMP only |
| CoW over-copy because frame refcount lags an RCU grace period (`b2` Friction 2) | Extra page copies after fork+exec | Accept; do not use copies-per-fork as a gate | Fork+exec benchmarks, small constant |
| Tick-granularity everything | Latency numbers quantized to ticks | `rdtsc` measurement; finer timer for deadlines | First latency run |
| Per-open credential round trip | `open` slow | Push model (Pitfall 22) | Any file-heavy script |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Servers authorize by a pid/uid in the message | Confused deputy; domain escape | Authorize by stamped credential and badge only |
| Server calls downstream with its own credential | Authority amplification through vfs/pty | Badged per-client endpoints |
| `cow_clone` TLB flush not waiting for remote CPUs | Stale writable mapping after fork (if SMP is ever on) | Fix before enabling SMP; marker until then |
| Global names (`/tmp`, `/dev/ptmx`, pid space) shared across domains | Squatting, existence leaks, covert channel | Per-domain namespaces; ESRCH/ENOENT for invisible things |
| Signal frame written to user stack by kernel without validation | Kernel write to attacker-chosen address | Libc trampoline plus bounded, validated write path (Pitfall 4's helper) |
| Granting `/` to make a POSIX program work | Ambient authority returns | Log as a namespace bug; fix the mount layout |
| exec server holds map rights to all new spaces forever | Standing powerful capability | Capability per exec, dropped after handing over |
| Exception handler reply accepted from a stale/revoked endpoint | Fault resolved by an unauthorized party | Already tested in B2 (`revoked_or_missing_handler_is_fatal`); keep when threads become capabilities |

## UX Pitfalls

Not a UI product. The user-facing surface is the shell and error behavior.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Denied operations return unusual errno | Programs misbehave in ways that look like bugs | Map denials to standard errno values (`PROJECT.md` Constraints); document the mapping once |
| Shell with no job control or Ctrl-C | Looks broken, hides kernel bugs behind hangs | Ctrl-C/Ctrl-Z in the P3 acceptance criteria |
| Hangs with no output | Developer cannot tell which server is stuck | Server liveness dump on a console key; serial log of IPC state on watchdog timeout |

## "Looks Done But Isn't" Checklist

- [ ] **fork:** Often missing the failure path and the multi-thread lock state. Verify: fork under ENOMEM; fork with a second thread holding the malloc lock.
- [ ] **fork:** Often tested with a write from the child's own code, never a kernel write. Verify: `read` into a CoW buffer in the child (Pitfall 4).
- [ ] **signals:** Often missing full register/FPU restore. Verify: a spin loop with known register values survives a handler (Pitfall 3).
- [ ] **signals:** Often only delivered to running threads. Verify: signal a thread blocked in `read` on a pipe and in `wait4`; check no data is lost (Pitfall 1).
- [ ] **wait:** Often missing reparenting and stopped/continued. Verify: model-based test passes; orphan reparents to init.
- [ ] **exit:** Often does not free anything. Verify: 10,000 fork+exit loop returns frames, budget, cspace slots to baseline (Pitfall 5).
- [ ] **pipes:** Often missing EOF on writer death. Verify: `kill -9` the writer; reader sees EOF (Pitfall 6).
- [ ] **exec:** Often missing failure rollback and CLOEXEC. Verify: exec of a non-ELF returns ENOEXEC with the old image running; CLOEXEC fds closed, others kept.
- [ ] **pty:** Often missing implicit ctty acquisition and SIGHUP. Verify: os-test `tiocsctty`, `tcgetsid`, `tcgetsid-wrong-controlling` pass.
- [ ] **os-test CI:** Often has no hang handling. Verify: a deliberately hanging test is recorded as "hang" and fails the gate.
- [ ] **os-test report:** Often a single number. Verify: per-suite table and the musl-on-Linux control row present.
- [ ] **Authority:** Often silently widened. Verify: a domain-A process cannot name any domain-B pid, path, or pty; a non-agent process sees a normal system.
- [ ] **OSTD protocol assumptions:** Often only in comments. Verify: each Friction-4 assumption has a test that fails if broken.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Cancel protocol missing (1) | HIGH | Change every server protocol to commit-or-not; redo the tests |
| Wait-loop races (2) | MEDIUM | Replace the hand-rolled loops by the shared helper; add the fail-point |
| Signal frame corruption (3) | MEDIUM | Move saving into the libc trampoline; add the register-survival test |
| CoW write path (4) | LOW | One `copy_to_user` helper; route all writers |
| Leaks (5) | MEDIUM-HIGH | Add the destroy op, then audit every object type created in B3 |
| No death notification (6) | HIGH if servers already hold state | Add notification; retrofit server cleanup; rebuild state from procmgr |
| ctty ownership ambiguity (7) | HIGH | Pick the owner, delete the copies, redo the pty suite |
| Fork half-built (9) | MEDIUM | Reorder into reserve-then-commit |
| CPU-charging stalls (10) | MEDIUM | Make servers exempt; add a ledger |
| Authority creep (11) | HIGH | Re-derive the namespace layout; audit every grant |
| Headline inflated (12) | LOW | Re-publish with the per-suite table; re-baseline |
| Libc chosen late (13) | HIGH | Shim re-write if protocols were POSIX-granular; protocol re-write if Linux-granular |
| Late exec design (14) | MEDIUM-HIGH | Add swap primitive; rewrite exec server |
| OSTD bump broke the scheduler (16) | MEDIUM | Revert the bump; fix against the protocol tests |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1. Cancel without server protocol | P1, P2, P3 | Interrupted `read`/`wait4` loses no data or status; server never blocks on a dead reply |
| 2. Hidden wake/interrupt races | P1 | Mutation of the shared helper fails a ktest; 1000x loop green |
| 3. Redirect corrupts state / kill(self) | P1, P2 | Register/FPU survival test; os-test signal raise/kill-self cases |
| 4. Kernel write into CoW | P1 | Post-clone IPC delivery ktest |
| 5. Teardown leaks | P1, P2 | 10,000 fork+exit returns to baseline |
| 6. No death notification | P1, P2, P3 | SIGKILL writer gives EOF to reader |
| 7. Session/ctty/pty ownership | P2, P3 | os-test `pty` suite running from first pty build; ownership table in design doc |
| 8. wait state machine | P2 | Model-based host test; Ctrl-Z / fg sequence works |
| 9. Fork transaction and cost | P2, P4, P5 | ENOMEM-during-fork test; primitive timings predict fork+exec |
| 10. Server CPU charging | P1, P2 | Throttled agent hammering procmgr does not stall others; idle task halts |
| 11. Authority leaks via servers | X, P2, P3 | Domain A/B isolation test in every phase |
| 12. os-test headline and allowlist | P2 (skeleton), P5 | Per-suite table; control row; hang outcome fails gate |
| 13. Libc decision late | end of P2 (spike/ADR), P4 | ADR with fd-table placement and clone mapping; pthread_join works |
| 14. exec replace-in-place | P1, P2 | Failed exec keeps old image; CLOEXEC, handler reset tests |
| 15. Timers | P1 | `block_until` with deadline used by every blocking call |
| 16. OSTD upgrade | X | Scheduler-protocol ktests exist; version and toolchain pinned per phase |
| 17. SMP retrofit | X, late P5 | `// single-cpu:` markers listed; `-smp 2` probe result recorded |
| 18. Pipe throughput | P1 (format), P3 | Pipe latency and throughput measured when pipes land |
| 19. Latency baseline fairness | P2, P5 | Linux guest in the same QEMU/KVM config; rdtsc medians and p99 |

## Sources

Repo documents (HIGH for repo facts):
- `/home/rakdos/repos/agentic-os-kernel/.planning/PROJECT.md` (Active requirements, Context, Constraints)
- `/home/rakdos/repos/agentic-os-kernel/docs/architecture/b2-spike-report.md` ("Not verified", "Friction to carry into B3", "Verified: Not caught")
- `/home/rakdos/repos/agentic-os-kernel/docs/architecture/option-b-microkernel.md` (sections 3.1, 3.2, 5, 6)
- `/home/rakdos/repos/agentic-os-kernel/docs/architecture/research/microkernel-posix.md` (sections 5-8: MINIX PM/VFS split, Hurd intr-msg, os-test table, risks)
- `/home/rakdos/repos/agentic-os-kernel/docs/architecture/research/rust-kernels-and-adherence.md` (Part 3: os-test, libc-test, application suites)
- `/home/rakdos/repos/agentic-os-kernel/kernel/src/` (`shortcut:` markers in `vm.rs`, `agent.rs`, `futex.rs`, `sched.rs`, `user.rs`)

Web, checked 2026-10-23 (MEDIUM):
- Sortix os-test results page, suite descriptions and scoring rules: https://sortix.org/os-test/
- os-test `pty` test sources as hosted by Redox (tiocsctty, tcgetsid, tcgetsid-wrong-controlling): https://doc.redox-os.org/test-results/os-test/os-test-20260828/html/pty/tiocsctty.c , https://doc.redox-os.org/test-results/os-test/os-test-20260821/html/pty/tcgetsid-wrong-controlling.c
- Redox on POSIX conformance testing, libc/kernel/procmgr attribution and test-harness problems: https://www.redox-os.org/news/posix-conformance-testing/
- relibc "Fix waitpid deadlock": https://gitlab.redox-os.org/plimkilde/relibc/-/commit/24ecd6ee16660651881a7ff218dc5293b95d7a26
- relibc "Ensure consistent allocator locking after fork" (PR 986): https://code.redox-os.org/redox-os/relibc/pulls/986
- Asterinas `RELEASES.md` (0.17-0.19 OSTD notes, `UserModeHooks` in 0.19.0) via `gh api repos/asterinas/asterinas/contents/RELEASES.md`, and release dates via `gh api repos/asterinas/asterinas/releases`
- Hurd performance page on fork cost warning (no numbers): https://www.gnu.org/s/hurd/open_issues/performance.html

Not verified (LOW; marked *(interpretation)* inline): musl's use of `clone`/`CLONE_CHILD_CLEARTID` and `set_tid_address`; whether `VmSpace::activate` preserves TLB entries; user-mode FPU enablement in OSTD 0.19.0; GitHub-hosted runner KVM availability.

---
*Pitfalls research for: POSIX personality on a Rust capability microkernel (OSTD 0.19.0)*
*Researched: 2026-10-23*
