# Architecture Research

**Domain:** POSIX personality (procmgr, vfs, exec, libc) on a Rust capability microkernel (Asterinas OSTD 0.19.0) where agents are kernel objects
**Researched:** 2026-10-23
**Confidence:** MEDIUM overall. HIGH for the gap analysis against the B2 code (read directly from `kernel/src/`). MEDIUM for the server design (it follows MINIX 3 PM/VFS, QNX and Redox as summarised in `docs/architecture/research/microkernel-posix.md`, but is *(interpretation)* for this kernel). LOW for OSTD console and boot-module APIs (docs.rs failed to build 0.19.0; the 0.18.1 docs were read instead).

Scope: B3 only. The existing kernel is not re-researched; section 1 lists only what B3 must change in it. Repo docs relied on: `docs/architecture/option-b-microkernel.md` (sections 2, 3), `docs/architecture/b2-spike-report.md` ("Not verified", "Friction"), `docs/architecture/research/microkernel-posix.md` (sections 1, 5, 8).

## Standard Architecture

### System Overview

```
 ring 3, agent "user" (the POSIX domain)         ring 3, server agents (TCB)
┌───────────────────────────────────────┐  ┌──────────────────────────────────────────────┐
│  BusyBox / POSIX programs             │  │  procmgr   vfs (+tmpfs, pipefs, devfs)       │
│  libc (relibc Pal | musl + shim)      │  │  exec      ttyd (later)                      │
│  posix-client: fork/exec/signal/fd    │  │  each: one event loop, deferred replies      │
│  libsys: native syscall wrappers      │  │  libsys + proto + server-rt                  │
└───────────────┬───────────────────────┘  └───────────────┬──────────────────────────────┘
                │ call(ep) [tag=pid stamped]               │ call / send between servers
                │ native syscalls (rax=nr)                 │ + kernel notifications
════════════════╧══════════════════════════════════════════╧════════════════════════════════
 kernel (OSTD 0.19.0, #![deny(unsafe_code)])
 syscall.rs  dispatch (new)     ipc.rs    endpoints, call/recv/reply/cancel, bulk copy, cap transfer
 cap.rs      growable CSpace    thread.rs Thread objects, tag, saved contexts, interrupt/suspend/kill
 agent.rs    credential, budgets, uncharge on teardown
 vm.rs       Space objects: create / clone / map_anon / read / write / destroy
 futex.rs    + timeouts         user.rs   run loop, exit/fault notification to the process endpoint
 sched.rs    + idle task        timer.rs  deadline -> endpoint message (new)
```

Rule that shapes everything: **the kernel knows threads, address spaces, endpoints and capabilities. It does not know pids, fds, paths, signals-by-number or ELF.** A pid is an opaque `tag` the kernel stamps on messages; procmgr owns its meaning.

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| `syscall.rs` (kernel) | Single dispatch for the native ABI; extracted from the `match ctx.rax()` in `user::run` | One `fn dispatch(thread, space, ctx) -> isize`; Linux register convention (rax = number, rdi/rsi/rdx/r10/r8/r9, return in rax), our own numbers except the three B2 ones (yield 24, exit 60, futex 202), which stay so the B2 tests keep passing |
| `Object::Space` (kernel) | An address space as a capability. Owns the `Arc<VmSpace>`, the `owner: Arc<Agent>` that is charged for its pages, and a count of pages charged | Wraps `vm::cow_clone`; ops need rights `CLONE`, `MAP`, `RW`, `THREAD`; dropping the last cap frees the space and uncharges |
| `Object::Thread` (kernel) | A thread as a capability, for procmgr only | Wraps `Arc<Thread>`; rights `INTERRUPT`, `SUSPEND`, `KILL`, `REPLY_EXC` |
| ipc v2 (kernel) | Synchronous `call` with a kernel-held reply handle, deferred reply, cancel, inline plus bulk copy, cap transfer | Extends `ipc.rs`; the endpoint queue holds blocked senders, not copied messages (see Pattern 2) |
| bootstrap (user, flat image) | The only holder of root authority after boot; creates agents, endpoints and servers, hands out attenuated caps, then drops its own | Kernel maps one flat binary at a fixed address; no ELF code in the kernel |
| vfs | Per-domain namespaces, mount table, per-process fd tables, shared open-file descriptions (OFDs), mode-bit checks, cwd and root, pipes and devices as in-process backends | Single-threaded event loop; backends behind a Rust trait: `tmpfs` (populated from the initrd), `pipefs`, `devfs` (`null`, `zero`, `console`) |
| exec | Parse an ELF, build a *fresh* Space with stack, argv, envp, auxv; return `{space cap, entry, rsp}` | `no_std` `elf` crate shared with bootstrap; reads the file through a one-shot file cap from vfs |
| procmgr | pid, ppid, pgrp, session, uid/gid, zombies, `wait`, `fork`, `exec` glue, `kill` routing, signal pending/entry bookkeeping, exit and fault handling, alarm timers | MINIX PM role. Owns every `Object::Thread` and `Object::Space` cap for user processes |
| ttyd / pty (later) | Line discipline, controlling terminal, job-control signals | Separate server; B3 needs it for `sh` job control and the os-test `pty` suite |
| `libsys` (user crate) | Native syscall wrappers, message encode/decode | `no_std`, shared by servers and by posix-client |
| `proto` (user crate) | Wire types for procmgr, vfs, exec: opcodes, structs, errno | Shared by clients and servers so a protocol change is a compile error on both sides |
| `posix-client` (user crate) | `fork`, `execve`, `waitpid`, `kill`, `open`, `read`, `dup2`, `pipe`... as Rust functions returning `Result<_, Errno>` | The seam between servers and any libc. Built and tested before the libc port |
| libc | C ABI over posix-client (relibc: new `Pal` backend; musl: shim that turns `__syscall(nr, ...)` into posix-client calls) | Decision B3 in option-b section 7. Nothing below this row depends on it |

## Recommended Project Structure

```
kernel/src/                  # existing crate "aok"
├── syscall.rs               # NEW: native ABI dispatch (moved out of user.rs)
├── cap.rs                   # CSpace becomes growable; Object::{Space, Thread}
├── ipc.rs                   # call / recv / reply / cancel; bulk copy; cap transfer
├── timer.rs                 # NEW: deadline -> message; futex/recv timeouts
└── (agent, thread, vm, futex, sched, user: extended, not rewritten)
userland/                    # NEW cargo workspace, target x86_64-unknown-none
├── libsys/                  # syscall wrappers, Message, Errno
├── proto/                   # procmgr/vfs/exec wire types
├── server-rt/               # event loop, deferred-reply table, bump heap on map_anon
├── elf/                     # no_std ELF64 parser + segment plan (host-testable)
├── bootstrap/               # flat image; wires everything
├── procmgr/  vfs/  exec/    # servers
├── posix-client/            # POSIX semantics as Rust functions
└── tests/                   # Rust ring-3 test programs (replace hand-assembled hex)
libc/                        # port, added in the libc phase
```

### Structure Rationale

- **`userland/` is a separate workspace from `kernel/`.** The kernel crate is built by `cargo osdk` inside the Docker image; user binaries need a different target. `kernel/Dockerfile` gains a `rustup target add x86_64-unknown-none` and `objcopy`. The kernel embeds the built `bootstrap` binary with `include_bytes!`, so a user-side change forces a kernel rebuild; accept it, it keeps boot a single artifact.
- **`proto` and `posix-client` are separate from the servers** so the libc decision and server work are decoupled, and so host unit tests (no QEMU) cover encode/decode, the `elf` planner, and vfs path resolution logic.
- **Rust ring-3 tests replace the hand-assembled programs.** B2's 50-byte programs (`user.rs` `prog::BYTES`) do not scale to fork, exec and call/reply; each one costs an `objdump` round trip.

## Architectural Patterns

### Pattern 1: Tag-stamped identity (process = tag, not cspace)

**What:** Today a message carries `Credential { agent_id, on_behalf_of, parent }` only (`ipc.rs`), and the `CSpace` belongs to the *agent* (`agent.rs`: `pub cspace`). Every process that forks inside an agent shares that cspace, so servers cannot tell two processes of one agent apart. Fix: each `Thread` gets a kernel `tid` and a `tag: u64` set once, at `thread_create`, by the holder of the Space cap (procmgr). The kernel stamps `{credential, tid, tag}` on every message. procmgr sets `tag = pid`. vfs keys its fd tables on `(agent_id, tag)`.
**When to use:** always; it is the only identity vfs and procmgr trust.
**Trade-offs:** costs two words in `Message` and one field in `Thread`. The alternative (a cspace per process) breaks "fork stays inside the agent" (option-b section 2.1) and makes fork copy caps.
**Corollary (a hard rule):** no per-process authority lives in the agent cspace. If a process held a Space cap to itself in the shared cspace, any sibling process in the agent could write its memory. Self-memory operations (`mmap_anon`, `munmap`, `mprotect`, `brk`) are therefore syscalls on the *calling thread's own space*, taking no cap. Space and Thread caps exist only in procmgr's and exec's cspaces.

```rust
pub struct Message {
    pub sender: Credential,   // agent_id, on_behalf_of, parent (B2)
    pub tid: u64,             // kernel-assigned, unique per thread
    pub tag: u64,             // procmgr-assigned at thread_create; = pid
    pub words: [u64; 6],      // label + 5 payload words, register-passed
}
```

### Pattern 2: Synchronous call, deferred reply (QNX rcvid shape)

**What:** B2 IPC is fire-and-forget `send` into a 16-entry queue plus `recv`; there is no reply, so the exception path replies through a kernel-held `Arc<Thread>` (the `deliver` shortcut in `user.rs`). B3 needs `call(ep, msg, send_buf, reply_buf)`: the caller blocks with its message stored *in the caller's `Thread`*; the endpoint holds a list of blocked callers (so `QUEUE_LEN` and `IpcError::Full` stop being a client-visible failure, and a flood is bounded by the number of threads). `recv` returns the message plus a **reply handle** (kernel table, per server thread); `reply(handle, msg, buf)` wakes the caller. A server may keep the handle and reply later; that is how `wait`, a blocking pipe `read`, and `tcgetattr`-style waits are served without blocking the server. `reply_recv` fuses the two for the server loop.
**When to use:** every client-server interaction. Plain `send` stays, for kernel notifications and server-to-server upcalls that need no answer.
**Trade-offs:** more kernel code in `ipc.rs` than a bare reply endpoint; the alternative (reply endpoints as capabilities, seL4 `Call`) costs a cap slot per call.
**Bulk data:** the 6 words carry opcode, fd, flags, lengths. Paths, argv, `read` and `write` data move by kernel copy: the caller passes `(ptr, len)` buffers, the kernel copies caller-to-server at `recv` time and server-to-caller at `reply` time, in chunks of at most 64 KiB (libc loops). `read(fd, buf, n)` replies straight into the user's `buf`: one copy. Copy at the frame level (`UFrame` reader/writer, as `vm.rs` already does with `new.writer().write(&mut old.reader())`) after breaking CoW with `handle_cow_fault` on the destination, exactly as `futex.rs::lookup` does. This answers B2 friction 1 (kernel writes into user memory skip CoW). It was inferred, not exercised; verify first.

### Pattern 3: Acyclic server call graph

**What:** servers call each other only downward, synchronously:

```
client ──► procmgr ──► exec ──► vfs        (exec reads the ELF through a file cap)
   └─────► vfs  ◄──── procmgr              (procmgr notifies vfs: FORK, EXEC, EXIT, SETID, SETSID)
vfs / exec ─────► procmgr                  only by async `send` (never `call`)
```

vfs never `call`s procmgr. When vfs needs process data (uid/gid, ctty), procmgr has pushed it ahead of time (MINIX VFS_PM_* messages, `microkernel-posix.md` section 5). This removes the PM-calls-VFS-calls-PM deadlock class by construction.
**Trade-offs:** state is duplicated (uid/gid lives in procmgr and a copy in vfs), and procmgr must push every change before replying to the client, or vfs checks against stale credentials.

### Pattern 4: Interrupt is a cancel, not an abort, for `call`

**What:** B2's interrupt aborts a blocked wait immediately (`Thread::block_on` returns `Interrupted`). For a blocked `call` that is unsafe: the server may already have consumed a pipe byte or advanced an offset, then the client sees EINTR and the effect is lost, or the reply arrives late into a recycled buffer. Rule: interrupting a thread blocked in `call` queues a kernel `Cancel(reply handle)` message to the server and the caller **stays blocked until the server replies** (with data, a partial count, or `EINTR`). Servers must answer a cancel promptly, which they can because blocked requests are deferred entries in their own table, not blocked server threads. A blocked `recv` (a server idle) still aborts at once as in B2.
**When to use:** all `call`s; the Hurd `intr-msg` layer exists because Mach lacked this (`option-b-microkernel.md` section 2).
**Trade-offs:** a wedged server wedges its callers' signals. Acceptable in a TCB server set; `SIGKILL` bypasses it (procmgr kills threads via the Thread cap; a killed caller's pending calls are dropped by the kernel).

### Pattern 5: Libc-filtered signals, procmgr/kernel-enforced fatal ones

**What:** procmgr routes `kill` and picks the target thread. Delivery is `thread_interrupt(thread_cap, entry, arg=signo)`. `entry` is the process's `__sig_entry` registered with procmgr at startup. The libc stub owns sigaction, masks and pending bits in user memory, and filters: blocked, ignored or default-ignore signals return immediately through `sigreturn`; a spurious EINTR is hidden by the libc wrapper restarting the call. `SIGKILL` and `SIGSTOP` never go through libc: procmgr uses `thread_kill` and `thread_suspend`.
**Kernel change needed:** B2's redirect (`user.rs`: `ctx.set_rdi(ctx.rip()); ctx.set_rip(handler)`) overwrites `rdi` and saves nothing else. Replace it with a per-thread **saved-context stack kept in the kernel**: redirect pushes the full `UserContext`, sets `rip = entry`, `rdi = arg`, and drops `rsp` below the 128-byte red zone with 16-byte alignment; `sigreturn` pops. This needs no kernel write into user memory (so no CoW problem) and keeps the register rewrite on the thread's own task, between `execute` calls, as the B2 design requires.
**Trade-offs:** libc filtering means a process can ignore a catchable signal it should have died from; only its own exit status is affected. Multithreaded routing ("deliver to any thread not blocking it") needs procmgr to know masks; v1 targets single-threaded processes exactly and treats multi-thread routing as best-effort.

### Pattern 6: Bootstrap as the only ambient authority

**What:** the kernel creates one root agent and maps one flat binary (`bootstrap`) with a cspace holding: the Agent cap (SPAWN), a Space-create cap, the console cap, and the initrd memory. bootstrap builds everything else and then `cap_drop`s its own root caps.
**When to use:** boot only. It is the seL4 root-task and Redox `bootstrap` pattern (`microkernel-posix.md` section 1).
**Trade-offs:** the kernel carries no ELF loader, so there is one loader (the `elf` crate) instead of two. The cost is a flat-binary build step (`objcopy -O binary`) and a fixed load address.

## Data Flow

### Server bootstrap and capability distribution

Order is forced by dependencies: vfs (no deps), exec (needs vfs), procmgr (needs vfs, exec), then pid 1.

```
kernel boot ──► root agent R + bootstrap thread (flat image)
bootstrap (agent R):
  1. create endpoints: vfs_ep, exec_ep, procmgr_ep (R keeps RECV+GRANT on each, derives)
  2. spawn child agent VFS  (carved budget; cspace: vfs_ep RECV, initrd Memory RO, console cap)
     load vfs ELF via the elf crate into a fresh Space; start thread
  3. spawn child agent EXEC (cspace: exec_ep RECV, vfs_ep SEND, Space-create)
  4. spawn child agent PM   (cspace: procmgr_ep RECV, vfs_ep SEND, exec_ep SEND,
                             Agent cap for USER with CHARGE|SPAWN-THREAD rights, timer cap)
     procmgr is also the exception/exit endpoint of every user thread
  5. spawn child agent USER (the POSIX domain; budget carved; cspace holds ONLY
                             procmgr_ep SEND, vfs_ep SEND — attenuated, no GRANT)
  6. ask procmgr to create pid 1 = /bin/sh from the initrd (goes through exec)
  7. cap_drop everything; bootstrap exits
```

| Holder | Caps held | Why it is exactly this |
|--------|-----------|------------------------|
| USER agent (all user processes) | `procmgr_ep` SEND, `vfs_ep` SEND | Authority is the set of handles (Core Value). A user process has no Space, Thread or Agent cap |
| vfs | `vfs_ep` RECV, console, initrd RO | A server cannot reach exec or procmgr except by `send` to a procmgr endpoint it is given for notifications |
| exec | `exec_ep` RECV, `vfs_ep` SEND, Space-create | Receives a *file cap* from procmgr per request, not ambient vfs access (see exec flow) |
| procmgr | `procmgr_ep` RECV, `vfs_ep` SEND, `exec_ep` SEND, Agent(USER) cap, all Thread/Space caps it creates | The only holder of thread and space authority over user processes |

B4 (not in scope) replaces step 5 with the broker minting USER-like agents with narrower grants; keep `procmgr` agnostic by reading the domain from the message credential, never from a constant.

### fork

```
libc fork():  push callee-saved regs; call procmgr FORK(resume_rip, resume_rsp)
procmgr:  1. pid' = alloc; check pid/process limits for the agent
          2. call vfs FORK(parent_pid, pid')          # copy fd table, bump OFD refcounts
          3. space' = space_clone(parent_space_cap)   # lazy CoW, vm::cow_clone
          4. thread' = thread_create(space', ctx{rip=resume_rip, rsp=resume_rsp, rax=0},
                                     tag=pid')        # same agent, so same cspace and budget
          5. record process, reply pid' to the parent
          on failure after 2: send vfs FORK_ABORT(pid'); the parent gets ENOMEM/EAGAIN
child:    first runs at resume_rip with rax=0, pops callee-saved regs, returns 0
```

The child's registers come from libc, not from the kernel: fork is an ordinary call, so only callee-saved registers, `rsp` and a resume address matter (Redox does this in `redox-rt` `fork_impl`, `microkernel-posix.md` section 1). No kernel API exposes a blocked thread's `UserContext`. Order matters: vfs first because it can fail and be rolled back; space clone and thread create last, so nothing needs undoing after the point of no return (MINIX forkexit.c comment, section 5). Only the forking thread exists in the child (POSIX); other threads' locks are libc's problem (QNX `forksafe`).

### exec

```
libc execve(path, argv, envp):  call procmgr EXEC(path, argv, envp)  [bulk: packed strings]
procmgr:  1. call vfs EXEC_OPEN(pid, path)   -> permission (x bit, nosuid for agents), returns
                                                a one-shot read-only FILE CAP (cap transfer)
          2. call exec LOAD(file_cap, packed argv/envp, owner=Agent(USER) cap derive)
exec:        reads the ELF via the file cap (calls on that cap, not on vfs_ep),
             space2 = space_create(owner); map_anon + space_write each PT_LOAD,
             build stack (argc, argv, envp, auxv), reply {space2 cap, entry, rsp}
procmgr:  3. call vfs EXEC(pid): close O_CLOEXEC fds, reset handlers' fd state
          4. thread_kill old threads; thread_create(space2, entry, rsp, tag=pid); drop old Space
          5. no reply to the old call (the old thread is dead); the new thread starts at entry
          on failure in 1-2: reply ENOENT/EACCES/ENOEXEC to the still-live caller
```

Pages of the new image are charged to `space.owner` (the user agent), never to exec's budget; otherwise exec would pay for every program it loads. If cap transfer in IPC slips, the fallback is `vfs OPEN_FOR(pid, path)` accepted only from the exec agent's credential. That is a deputy by design: use it only as a bring-up shortcut, labelled `shortcut:` in code.

### open / read (the common path)

```
libc open(path, flags, mode) -> posix-client -> call vfs OPEN  [path in send_buf]
vfs:  caller = (msg.sender.agent_id, msg.tag)
      ns   = namespace_of(agent_id)             # per-domain root: "can you name it"
      node = resolve(ns, proc.root, proc.cwd, path)       # ENOENT if outside the grant
      check mode bits against proc.uid/gid, mount flags   # "may you"; EACCES
      ofd  = Ofd { node, offset, flags, refs: 1 }; fd = lowest free in proc.fdtable
      reply fd
libc read(fd, buf, n) -> call vfs READ(fd, n)  [reply_buf = buf]
vfs:  ofd = proc.fdtable[fd]; backend.read(ofd, n) -> either data now, or defer:
      pipe empty / tty no input -> store (reply handle, ofd) in the backend's waiter list
      later write wakes it -> reply(handle, data)
```

One round trip per `read` and `write`; there is no fd fast path. An fd is a number in vfs, never a kernel cap, so `dup`, `dup2`, `fork` and `O_CLOEXEC` are table edits in one place, and shared offsets (POSIX) fall out of the shared `Ofd`. uid/gid are inputs to the mode-bit check only; they grant no authority (option-b section 3.1).

### kill, signals and faults

```
kill(pid, sig):  libc -> call procmgr KILL(pid, sig)
procmgr:  1. target = procs[pid]; same agent_id (or capability: B4) else EPERM;
             POSIX uid rule on top; sig == 0 -> existence check only
          2. SIGKILL: thread_kill each thread; destroy Space; run the exit path
             SIGSTOP/SIGCONT: thread_suspend / thread_resume; wait-status update
             other: pick thread; reply to the caller FIRST, then
                    thread_interrupt(thr, entry=proc.sig_entry, arg=sig)
target:   if blocked in a vfs call -> kernel sends Cancel to vfs -> vfs replies EINTR;
          kernel pushes context, jumps to __sig_entry(sig); libc runs the handler;
          sigreturn pops; the interrupted wrapper sees -EINTR and restarts (SA_RESTART) or returns
```

- **Self-signal:** `raise` and `kill(getpid())` are handled in libc without procmgr, or procmgr's reply carries a `deliver_now` flag. A thread running in user mode only notices a redirect at the next timer interrupt (B2 friction 3, up to one tick), so a plain interrupt would let `kill` return before the handler ran.
- **Faults:** user exceptions arrive at procmgr's endpoint from the kernel with `tid` and `tag` stamped (this removes the B2 `deliver` shortcut). procmgr maps vector to signal (`#PF`/`#GP` to SIGSEGV, `#DE` to SIGFPE, `#UD` to SIGILL) and either redirects the thread or terminates the process with a signaled status.
- **Exit notification:** every thread death (`exit`, fatal fault, `Exit::Budget`, kill) makes the kernel send `ThreadExit{tid, tag, reason, code}` to the process endpoint. libc `_exit` is just the `exit` syscall. procmgr learns of every death, including ones no libc cooperated in.
- **Exit and wait:** on last-thread exit procmgr (1) `send`s vfs EXIT(pid) so fds close and pipe readers see EOF, (2) destroys the Space (uncharging its pages), (3) reparents children to pid 1, (4) records a zombie with the status, (5) interrupts the parent with SIGCHLD, (6) answers a deferred `wait` reply handle if the parent is waiting.

### State Management

```
procmgr:  procs[pid] -> { ppid, pgrp, sid, uid/gid, agent_id, thread caps, space cap,
                          sig_entry, pending set, status, wait-waiters }
vfs:      namespaces[agent_id] -> mount table + root node
          procs[(agent_id, tag)] -> { fdtable[fd -> Ofd], cwd, root, uid/gid, ctty }
          ofds: refcounted; pipes/devices hold waiter lists of reply handles
libc:     sigaction table, masks, pending bits, errno, fork handlers (user memory)
kernel:   cspaces, endpoints (blocked callers), threads (tid, tag, saved contexts)
```

The only state in two places is uid/gid and the controlling tty (procmgr authoritative, vfs a pushed copy). Everything else has one owner.

## Kernel Work B3 Forces (gap list against the B2 code)

| # | Gap in B2 | Evidence | Needed for |
|---|-----------|----------|-----------|
| K1 | User mode has `exit`, `sched_yield`, `futex` only | `user.rs` `run` match | Every syscall below |
| K2 | `CSpace::SLOTS = 64`, linear `position(is_none)` insert | `cap.rs` | procmgr holds a Thread and Space cap per process; needs thousands. Growable table plus a free list |
| K3 | `Message` has no tid or tag, 4 words, no reply, no bulk, no cap transfer | `ipc.rs` | Patterns 1, 2; exec's file cap; bootstrap wiring |
| K4 | No `Object::Space`; `cow_clone` takes `&VmSpace` | `vm.rs`, `b2-spike-report.md` | fork, exec, exit |
| K5 | No `Object::Thread`; exception handler replies via a kernel-held `Arc<Thread>` | `user.rs` `deliver` shortcut | thread create/kill/suspend/interrupt by procmgr; fault messages that name the thread |
| K6 | Redirect clobbers `rdi`, saves no context, no `sigreturn` | `user.rs` end of `run` | Pattern 5 |
| K7 | `Thread` is bound to one `VmSpace` and one `run` call; no `thread_create` | `thread.rs`, `lib.rs` | fork, clone, exec each become `thread_create(space, ctx, tag)` |
| K8 | `Agent::charge_pages` has no inverse; teardown returns nothing | `agent.rs` (grep: no uncharge) | A shell that forks 1000 times exhausts the budget even though every child exited. Highest-impact B2 gap for B3 |
| K9 | No idle task | `sched.rs`, `lib.rs` (boot task is the idle fallback) | An idle system (shell waiting for input) has no runnable task; `pick_next` must not return `None` after `update_current` asked for a switch |
| K10 | No timeouts: futex `WAIT` with a timeout is `ENOSYS`; no timer-to-message | `user.rs` `sys_futex` | `sleep`, `nanosleep`, `alarm`, `poll` timeouts, `pthread_cond_timedwait` |
| K11 | Console: no user-visible device | none | `/dev/console`, BusyBox output. OSTD 0.18.1 docs list an `arch::serial` module; confirm in 0.19.0 whether RX is interrupt-driven (LOW confidence) |
| K12 | Initrd source | none | `ostd::boot::boot_info()` exists in 0.18.1; whether it exposes the initramfs is unverified. Fallback: `include_bytes!` of a tar or cpio built into the kernel |

K1 to K3 and K8 to K10 are single-purpose and small. K4 to K7 are the bulk. Everything is expected to be safe Rust on the public API, as in B2; a need for `unsafe` or an OSTD patch is the recorded Redox re-score signal (`PROJECT.md` Constraints).

## Build Order

```
 A  userland foundation: workspace, flat bootstrap image, syscall.rs, Rust ring-3 tests
 │   (K1)
 B  IPC v2: tag/tid, call/recv/reply/cancel, bulk copy, cap transfer, growable cspace
 │   (K2, K3)   ──► measure call round-trip here (cost basis for everything)
 C  Space + Thread objects, thread_create, saved-context redirect + sigreturn, exit
 │   notification, uncharge on teardown, idle task, timers   (K4-K10)
 D  bootstrap + server runtime: elf crate, server-rt, cap wiring, spawns a null server
 │
 ├─ E  vfs: tmpfs/initrd, devfs console, fd tables, OFDs, pipes, per-domain namespace (K11, K12)
 │      tested with a Rust client, no libc
 │
 F  exec + procmgr core: pid table, fork, exec, exit, wait        (needs C, D, E)
 │
 G  signals + sessions: kill, sig_entry, exceptions -> signals, pgrp, setsid, alarm;
 │   ttyd with a line discipline                                  (needs F)
 │
 H  posix-client crate, then libc port (relibc Pal or musl shim)  (needs E, F; G for signals)
 │
 I  BusyBox sh runs; os-test in CI with allowlists; fork+exec / signal / pipe latency vs Linux
```

**Why this order**

- **A first:** the first Rust ring-3 binary de-risks the Docker image (new target, `objcopy`), the flat-image mapping and the syscall ABI before any design depends on them. It also retires hand-assembled test programs.
- **B before C:** call/reply is the single mechanism every server uses and the one whose cost decides whether the layered design is viable (option-b section 6, "Server round trips make fork/exec, signals and pipes slow"). Measure it as soon as it exists.
- **C before any server:** K8 (uncharge) and K9 (idle) are correctness conditions for long-running servers, not polish.
- **E before F:** vfs can be built and tested alone with a Rust client, and exec needs it to read files. vfs is also where `open` semantics are decided; per option-b section 6, test pty, session and ctty from the first procmgr build, not last.
- **Parallelism for one person plus agents:** after D, E and the `elf`/exec parsing work are independent. The `posix-client` crate can start as soon as `proto` types exist, since it is mechanical.
- **H gated only on the libc choice:** nothing in A to G depends on relibc versus musl. Defer decision B3 to the start of H, with posix-client already measured.

**Suggested phase cut for the roadmap:** (1) A, (2) B, (3) C, (4) D+E, (5) F, (6) G, (7) H, (8) I. Phases 2 and 3 are the riskiest kernel work; phases 4 to 6 are server work with known designs (MINIX PM/VFS).

## Scaling Considerations

This is a single-CPU research artifact. "Scale" means process count and call rate.

| Scale | Architecture Adjustments |
|-------|--------------------------|
| Tens of processes (BusyBox, os-test) | Single-threaded server loops, linear tables, one global futex lock. Fine on `-smp 1` |
| Hundreds of processes (build, shell loops) | `fork` cost: page-table walk in `cow_clone` is O(mapped pages); `exec` cost: ELF copy via `space_write`. Measure; do not optimise first |
| SMP | Server loops serialize on one thread each; per-CPU run queues, hashed futex buckets and remote TLB wait are the known B2 gaps. Out of scope unless B3 forces it |

### Scaling Priorities

1. **First bottleneck:** server round trip count per POSIX call (`read` and `write` on a pipe are two calls plus two context switches). Mitigation after measurement: batch in posix-client, then give pipes a shared-memory ring (needs a Memory object; defer).
2. **Second bottleneck:** budget accounting: servers do work for clients but run on the server agent's CPU quota, so a client can consume service time it is not charged for. Record it; budget donation is a B4-era design question.

## Anti-Patterns

### Anti-Pattern 1: Per-process caps in the agent cspace

**What people do:** give each process a Space or Thread cap to itself in the shared agent cspace, because "that's where caps live".
**Why it's wrong:** every sibling in the agent can use it, so fork inside an agent becomes a memory-write primitive against siblings.
**Do this instead:** Pattern 1 corollary. Self operations are cap-less syscalls; Space and Thread caps live only in procmgr and exec.

### Anti-Pattern 2: Servers that block on a client

**What people do:** implement pipe `read` or `wait` as a blocking wait inside the server thread.
**Why it's wrong:** one idle pipe reader stops the server; with an acyclic call graph it also deadlocks signals (the Cancel cannot be served).
**Do this instead:** deferred replies. The server loop only ever blocks in `recv`.

### Anti-Pattern 3: Letting vfs call procmgr

**What people do:** vfs asks procmgr for the caller's uid on each request.
**Why it's wrong:** closes a cycle (procmgr to vfs during fork or exec, vfs to procmgr during open); first concurrent fork and open deadlock.
**Do this instead:** Pattern 3. procmgr pushes credentials on every change, before it replies to the client.

### Anti-Pattern 4: Fork that needs the kernel to read a blocked thread's registers

**What people do:** add a `thread_get_context` syscall so procmgr can build the child's state.
**Why it's wrong:** widens the kernel's trusted surface and invents a race with signal redirect.
**Do this instead:** libc passes resume rip, rsp (callee-saved registers on the stack) in the FORK message.

### Anti-Pattern 5: Hard-wiring the libc choice into the servers

**What people do:** shape procmgr and vfs protocols around relibc's Redox scheme calls or musl's Linux syscalls.
**Why it's wrong:** the libc is an open decision that can change after measurement; protocols would inherit Linux or Redox quirks that the capability model does not have.
**Do this instead:** `proto` is ours; posix-client is the single adapter; the libc binds to posix-client only.

### Anti-Pattern 6: ELF loading in the kernel

**What people do:** parse the first user binary in the kernel to avoid a bootstrap stage.
**Why it's wrong:** a second loader to maintain; parsing untrusted input in the TCB's most privileged part.
**Do this instead:** flat bootstrap image, one `elf` crate used from user mode.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Build image `agentic-os-kernel-dev:0.19.0` | Add the `x86_64-unknown-none` target and `objcopy`; `userland/` built before the kernel | Do not run it on the planning host; CI runs it. `kernel/osdk.sh` passes only `/dev/kvm` |
| relibc | A new `platform` backend over posix-client | MIT, mostly Rust; ceiling on os-test unknown (option-b section 3.2) |
| musl | Shim at `__syscall`, plus a Linux-layout signal frame built by libc's `__sig_entry` (musl's `rt_sigaction` expects a `__restore_rt` restorer) | 94% os-test baseline; more shim code |
| os-test, BusyBox testsuite | Cross-compiled, run in QEMU, results diffed against an allowlist in CI | Kernel-heavy suites (`process`, `signal`, `pty`, `io`) reported separately |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| user process to procmgr | `call procmgr_ep`, tag stamped | fork, exec, exit-wait, kill, getpid family, setsid, sigentry |
| user process to vfs | `call vfs_ep`, tag stamped | open, read, write, dup, pipe, stat, chdir, getdents |
| procmgr to vfs | `call` for FORK, EXEC, SETID; `send` for EXIT | Never the reverse as a `call` |
| procmgr to exec | `call` LOAD with a file cap | Returns a Space cap |
| exec to vfs | `call` on the transferred file cap | Not on `vfs_ep` |
| kernel to procmgr | `send`: ThreadExit, Exception, Timer | Kernel-stamped with tid and tag. Needs a reserved queue slot so a kernel notification is never dropped |
| process to kernel | native syscalls | Self-memory ops, futex, yield, exit, sigreturn, call/recv/reply |
| libc to posix-client | Rust/C function calls | The only place POSIX signatures meet Rust types |

## Open Questions and Phase Research Flags

| Phase | Flag | Why |
|-------|------|-----|
| B (IPC v2) | Needs deeper research and a spike | Frame-level cross-space copy and CoW-break order were inferred, never run. The cancel-then-wait protocol (Pattern 4) has no tested precedent in this repo; the interrupt-vs-block race was reasoned, not stress-tested (b2 report) |
| C (Space/Thread) | Spike first | `thread_create` must spawn an OSTD `Task` from a syscall; the scheduler protocol was reverse-engineered (friction 4). Idle task interacts with `pick_next` rules |
| D (bootstrap) | Verify OSTD APIs | Whether `boot_info()` exposes an initramfs; whether the serial module gives interrupt-driven RX (K11, K12). Docs.rs failed for 0.19.0: read the source, not the 0.18.1 docs |
| G (signals) | Deeper research | Multithread signal routing, `SA_RESTART` interplay with Cancel, async-signal-safety of `__sig_entry`, job control and ctty. pty is where Redox scores 6% and MINIX 16% on process (os-test) |
| H (libc) | Decision B3 | relibc `Pal` surface versus musl shim; only this phase needs it |
| E, F | Standard patterns | MINIX PM/VFS, Redox procmgr and QNX cover them. Unlikely to need research beyond reading those sources |

## Sources

- `/home/rakdos/repos/agentic-os-kernel/kernel/src/` (`cap.rs`, `agent.rs`, `ipc.rs`, `thread.rs`, `vm.rs`, `user.rs`, `sched.rs`, `futex.rs`, `lib.rs`): the public surface, HIGH confidence for every gap in "Kernel Work B3 Forces".
- `/home/rakdos/repos/agentic-os-kernel/docs/architecture/option-b-microkernel.md` sections 2, 3, 6, 7; `b2-spike-report.md` "Not verified" and "Friction"; `research/microkernel-posix.md` sections 1, 5, 6, 8 (Redox `fork_impl`, MINIX PM/VFS split and forkexit ordering, QNX connection model, Hurd `intr-msg`): HIGH for what those systems do, MEDIUM for transfer to this kernel.
- `/home/rakdos/repos/agentic-os-kernel/.planning/PROJECT.md` Active requirements and Constraints.
- https://docs.rs/ostd/0.18.1/ostd/boot/index.html and https://docs.rs/ostd/0.18.1/ostd/arch/index.html: existence of `boot_info`, `arch::serial`. LOW for 0.19.0 (docs.rs build failed for that version).
- Design choices in Patterns 1 to 6, the call graph, and the build order are *(interpretation)*, not source claims.

---
*Architecture research for: POSIX personality on a Rust capability microkernel (B3)*
*Researched: 2026-10-23*
