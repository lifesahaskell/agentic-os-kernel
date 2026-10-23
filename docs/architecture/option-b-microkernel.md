# Option B: a Rust capability microkernel with a POSIX personality

Status: exploration draft, 2026-10-23. Decision record: [ADR 0002](adr/0002-rust-microkernel.md).
Evidence: [research/microkernel-posix.md](research/microkernel-posix.md) (how Redox, Fuchsia,
seL4, Genode, QNX, MINIX 3 and Hurd do POSIX) and
[research/rust-kernels-and-adherence.md](research/rust-kernels-and-adherence.md) (Rust kernels,
capability models, free conformance suites). Claims about those systems are cited there, not
repeated here. Everything else in this doc is design reasoning *(interpretation)*.

## 1. The bet in one paragraph

The kernel knows three things the Linux option could not express natively: **an agent is a
protection domain, authority is a capability, and identity is stamped by the kernel on every
message.** POSIX is a personality made of a libc and userspace servers, as in Redox, MINIX 3 and
Hurd. Because an agent's authority is the set of handles it holds, most mediation happens
**when a capability is granted**, not on every syscall. That removes the per-call trap and the
time-of-check/time-of-use window that sank seccomp user-notify in ADR 0001. Per-call checks
remain only where a capability is coarse: network destinations and tool arguments.

## 2. What must be in the kernel

The research gives a short, evidence-backed list. Every system in the survey that runs real
POSIX programs has the first three in its kernel; the systems that left them out (Fuchsia
native, seL4 with muslcsys, LionsOS) return `ENOSYS` for fork and signals, and Fuchsia had to
build a separate Linux executive (Starnix) to get them back.

| Kernel primitive | Why it cannot live in a server | Evidence |
|---|---|---|
| Address space with **lazy copy-on-write clone**, callable by a process manager on another process's behalf | `fork` cost and semantics; Genode's eager-copy fork forced local-only signals and stub sessions | Redox, MINIX 3, Hurd, Starnix ([[copy-on-write]]) |
| **Thread interrupt and redirect**: abort a blocked IPC, then rewrite registers atomically with respect to syscall entry | Signal delivery; Hurd needs a whole interruptible-RPC layer because Mach lacked it | Redox, MINIX `sys_sigsend`, QNX, Hurd, Zircon kick |
| **Exception delivery as a message** to a handler | Synthesizing `SIGSEGV`/`SIGFPE`; user-level pagers | seL4 fault IPC, Zircon exceptions ([[trap-and-interrupt]]) |
| **Futex**: wait/wake/requeue keyed so shared memory works across processes | Every system with real pthreads put it in the kernel; MINIX has neither | Redox (physical-address keyed), QNX, Hurd `gsync`, Zircon |
| **IPC endpoints with cancel** | Clean `EINTR` and restart without an intr-msg layer | Hurd's absence of it is the cautionary case |
| **Capability space** with rights masks on derive and **per-grant revocation** | The whole security model | seL4, Zircon, EROS/Coyotos |
| **Agent object** (below) | The "native" requirement | None: this is the novel part |

Everything else (process table, fd tables, VFS, exec, pty, shared memory, sockets, uid/gid
policy) lives in servers.

### 2.1 The agent object

An agent is a kernel object that owns:

- a **root capability space**. No ambient authority: no global namespace, no root file
  system, no "open any path".
- an **unforgeable credential**: `agent_id`, `on_behalf_of` (the human principal), parent
  agent. The kernel stamps it on every IPC message the agent's threads send, like an seL4
  badge, so servers never have to trust a claimed identity.
- **budgets**: CPU time and memory, enforced by the kernel. Token and money budgets are
  enforced by the inference server and tool proxies, which charge them against the stamped
  credential.
- a **parent link**. Child agents are created only with an attenuated copy of a subset of the
  parent's capabilities ([[capabilities]], ostep pp. 733-734).

Processes run *inside* an agent. `fork` and `exec` inside an agent stay inside it: the child
shares the agent object, so shelling out cannot shed the credential or gain authority. Plain
POSIX programs that are not agents run inside a "user" domain whose capability space holds the
ordinary POSIX world, so they see an unmodified system.

### 2.2 Revocation: forwarders, not a derivation tree

The research finds attenuation solved the same way everywhere (derive with a subset of rights,
never more). Revocation is the real choice:

- seL4's derivation tree revokes transitively, with partial-completion hazards.
- EROS/Coyotos interpose a forwarder per grant and revoke that one grant, at the cost of an
  extra IPC hop; Shapiro reports real-world depth never exceeded 3.
- Zircon and Capsicum document no transitive revoke.

**Choice: per-grant forwarders.** "End this tool grant" or "this sub-agent is done" is a
per-delegation operation, which is exactly what a forwarder revokes. The broker pays for and
owns the forwarders, which matches the EROS principle that whoever pays for revocation
bookkeeping can deallocate it.

## 3. The POSIX personality

```
 agents and POSIX programs ─────────────────────────────────────────────────────
   libc (relibc or musl + shim): fork/exec/signals/fds → IPC to servers
 servers (user mode) ───────────────────────────────────────────────────────────
   procmgr     pid, ppid, pgrp, session, ctty, uid/gid, wait, kill routing
   vfs         per-domain namespaces, open-file descriptions, permission bits
   net         sockets; destination policy for agents
   pty, ipc    pseudo-terminals; POSIX/SysV shared memory, semaphores
   exec        ELF loading into a fresh address space
 agentic servers (TCB, deterministic) ───────────────────────────────────────────
   broker       grants, forwarders, policy, audit log, budgets ledger
   inference    request queue, scheduler, context snapshot/restore
   context      per-agent context store, admission control
   tool proxies hold real credentials; agents hold handles
   console      trusted path: broker-owned prompt no agent has a capability for
 kernel ────────────────────────────────────────────────────────────────────────
   address spaces + CoW clone │ threads + interrupt/redirect │ exceptions as IPC
   futex │ IPC endpoints + cancel │ capability spaces │ agent objects + budgets
```

### 3.1 How POSIX meets capabilities

| POSIX mechanism | Implementation | What an agent sees |
|---|---|---|
| `fork` | libc asks procmgr; procmgr calls the kernel CoW clone and copies the fd table | Child in the same agent; no new authority |
| `exec` | exec server loads the image; the agent's namespace is mounted `nosuid` | setuid bits do nothing for agents. POSIX permits this through `ST_NOSUID`, so it is adherent ([[setuid-and-privilege-escalation]]) |
| `open` | vfs resolves the path **within the caller's namespace**, then checks mode bits | Paths outside the grant return `ENOENT`; granted read-only paths return `EACCES` on write |
| fds | an fd is a handle to an open-file description held by vfs; `dup` and `fork` share it, so offsets are shared as POSIX requires ([[file-descriptor]]) | Ordinary fd semantics |
| `kill` | procmgr routes it; it needs a process in the same domain, or a capability to the target | Cross-domain `kill` returns `EPERM` |
| signals | procmgr decides; the kernel interrupts and redirects the thread | Ordinary semantics |
| uid/gid | **a server-side attribute, never kernel authority.** The namespace is the authority, and mode bits are a second check inside it | An agent can run with its human's uid and still see only its grant |
| `socket`/`connect` | net server; a socket capability is required, and agent destinations are checked per call | Denied destinations return `EACCES` or `ECONNREFUSED` |
| `/dev/inference` | a file served by the inference server | `read`/`write` on a device, scheduled and charged |

The uid rule settles the clash the research flagged: no surveyed system documents a clean
answer for capability authority versus POSIX identity. Here, uid/gid answers "which mode bits
apply", and capabilities answer "can you even name it". That is why agents need no separate
uid. *(interpretation)*

### 3.2 The libc

There are two candidates, and the choice is open question B3:

- **relibc** (Redox, MIT, mostly Rust). It already has a userspace fork, signals and exec over
  a microkernel (`redox-rt`). Its os-test ceiling is unknown apart from the kernel.
- **musl plus a syscall shim.** musl on Linux scores 94% on os-test, the highest published row,
  so it sets the libc ceiling. Its syscall interface is Linux-shaped, so the shim translates
  syscalls into server IPC.

## 4. Build versus reuse

"New" does not have to mean "from zero". This is open question B1.

| Path | What it gives | What it costs |
|---|---|---|
| **Asterinas OSTD** as the hardware layer | ~10k lines, the only crate allowed `unsafe`; x86-64/arm64/riscv64; on crates.io; MPL-2.0 (file-level copyleft) | Built for a monolithic Linux-ABI kernel; API stability not stated. **Spike it first.** |
| Fork the **Redox** kernel | ~43k lines, microkernel, fork/signals/futex already in place, os-test 84% with relibc | Inherits scheme semantics; capabilities are being retrofitted (NLnet, 2026-03) |
| **seL4 + rust-sel4** | The only proof story; best capability model | The kernel is C. There is no futex or CoW clone: both become pager and server work. Fails "Rust kernel". |
| From zero | Full control of the object model | Every primitive in section 2. Moss reached bash and coreutils on 109 syscalls in under a year, which shows the scale, not a promise. |

**Recommendation:** a time-boxed spike that builds the section 2 primitives (address space with
CoW clone, threads, IPC, capability space) on OSTD. If OSTD's abstractions fight a
capability microkernel, fall back to forking Redox's kernel and replacing schemes with the
agent object model.

## 5. POSIX adherence bar (replaces certification)

From the test-suite research:

- **Headline: Sortix os-test**, mandatory POSIX only. It is the only free suite that is current
  (POSIX.1-2024) and cross-compilable. Published baselines: Minix 73%, Redox 84%, Sortix 85%,
  FreeBSD 90%, Linux 92%, musl 94%. Proposed tiers: **70% (boots, mostly works), 84%
  (Redox-class), 90% (FreeBSD-class)**.
- **Enforcement: per-suite allowlists in CI**, as Asterinas does. Any regression fails the
  build, and the allowlist only grows. Suites: os-test, Open POSIX Test Suite interface areas
  (old but broad), musl libc-test.
- **Milestone gate: application suites.** The BusyBox testsuite, bash `run-all`, and later
  coreutils `make check`, each against a triaged expected-fail list.
- **Skipped:** The Open Group's free 12-month licence. Results cannot be published or called a
  pass, it expires, and it tests the 2016 standard. Revisit only for a private gap analysis at
  the 84% tier.

**Known weakness, to check before committing:** os-test's `include`, `namespace` and `limits`
suites mostly exercise libc and headers. A good libc can lift the headline without any kernel
progress. Report the kernel-heavy suites (`process`, `signal`, `pty`, `io`) separately. Those
are where microkernels score worst: Redox pty 6%, MINIX process 16%.

## 6. Risks

| Risk | Signal | Response |
|---|---|---|
| POSIX work crowds out the agent work | The agent object model is unbuilt when the 70% tier lands | Prototype the broker protocol early on the host (scoping milestone B1); it is a protocol, not a kernel feature |
| Process-global state leaks conformance | pty, session and ctty suites stall below 50% | Test them from the first procmgr build, not last |
| Server round trips make fork/exec, signals and pipes slow | No surveyed system publishes POSIX-layer cost; Hurd warns about fork with no numbers | Measure fork+exec, signal round trip and pipe latency in the first milestone, against Linux on the same hardware |
| Ambient authority creeps back | A POSIX program needs `/` to work, and the fix is to grant it | Treat each such grant as a bug in the namespace model, not a config change |
| Novelty is assumed, not established | One search pass found no kernel-level capability system evaluated with LLM agents as the processes | Ingest AgenticOS (arXiv 2606.21129) and Agent libOS (2606.03895) before claiming novelty |

## 7. Open questions for Corey

- **B1. Build base:** an OSTD spike first, or go straight to a Redox fork? **Spiked 2026-10-23:** OSTD
  held up for the B2 primitives; continue on it ([b2-spike-report.md](b2-spike-report.md)).
- **B2. Target architecture:** x86-64 only for v1? Asterinas tiers it first; Moss is AArch64 only.
- **B3. libc:** relibc or musl plus shim? This sets the ceiling on the os-test headline.
- **B4. Adherence tier and date:** which of 70%, 84% or 90%, and by when? Without a date the tier
  is a wish.
- **B5. Team and deadline:** still open from scoping section 9. On this path they decide whether
  the project is research or product.
