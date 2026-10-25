# agentic-os-kernel

## What This Is

A research effort toward an operating system where LLM agents are kernel objects: principals the
OS itself schedules, accounts for and constrains, on a new Rust capability microkernel built on
Asterinas OSTD. The OS must also run ordinary POSIX software unchanged, through a libc and
userspace servers (procmgr, vfs, exec). It is built by one person plus agents as a research
artifact. Sources: [scoping.md](../docs/architecture/scoping.md),
[option-b-microkernel.md](../docs/architecture/option-b-microkernel.md),
[ADR 0002](../docs/architecture/adr/0002-rust-microkernel.md).

## Core Value

The protection boundary knows what an agent is: authority is exactly the set of capability
handles an agent holds, and the kernel stamps its identity on every message, while unmodified
POSIX programs still run.

## Requirements

### Validated

Built and verified in the B2 spike on OSTD 0.19.0 (no `unsafe`, no OSTD patches; 27 ktests and
the boot-time budget check pass in QEMU/KVM). Evidence:
[b2-spike-report.md](../docs/architecture/b2-spike-report.md).

- ✓ Capability space with rights masks, attenuating derive, per-grant revocation via forwarders — B2
- ✓ Agent object: credential (`agent_id`, `on_behalf_of`, parent), root cspace, memory budget carved from parent — B2
- ✓ Rights-checked IPC endpoints that stamp the sender's credential — B2
- ✓ Thread interrupt (aborts blocked IPC with EINTR) and redirect (the kernel half of signals) — B2
- ✓ Lazy copy-on-write address-space clone; the faulting agent pays for the copied page — B2
- ✓ CPU budget per agent, carved from the parent and enforced by a custom scheduler — B2
- ✓ Per-task address-space switch on every context switch — B2
- ✓ Exceptions delivered as messages to a handler endpoint — B2
- ✓ Futex wait/wake/requeue across address spaces, keyed by physical address — B2
- ✓ Boots in QEMU on x86-64 — B2

### Active

The B3 milestone (POSIX core). Each is a hypothesis until shipped and verified; the roadmap
turns them into numbered requirements.

- [ ] A syscall surface from ring 3 for capability, IPC, thread and address-space operations (user mode has only `exit` today)
- [ ] Capability-gated address-space clone and a thread capability type, so procmgr can fork and signal on another process's behalf
- [ ] procmgr: pid, ppid, process groups, sessions, `wait`, `kill` routing, signal delivery over the kernel interrupt/redirect
- [ ] exec server: ELF loading into a fresh address space
- [ ] vfs: per-domain namespaces, open-file descriptions shared by `dup` and `fork`, permission bits
- [ ] libc running on the system (relibc or musl plus shim, open question B3 in option-b section 7) covering fork, exec, signals and fds over IPC
- [ ] A BusyBox shell runs on the system
- [ ] Sortix os-test wired into CI with per-suite allowlists that only grow; kernel-heavy suites (process, signal, pty, io) reported separately
- [ ] fork+exec, signal round-trip and pipe latency measured against Linux on the same hardware
- [ ] Agent-visible files are ordinary POSIX files; a process without an agent credential sees an unmodified POSIX system (scoping section 5 rules 1-4)

### Out of Scope

- Formal POSIX certification and the UNIX trademark — dropped 2026-10-23; adherence is measured with free suites ([posix-certification.md](../docs/architecture/posix-certification.md) kept for reference)
- Agents running POSIX tools under a grant, inference server, context store, trusted-path escalation (B4) — next milestone, held in v2 requirements
- Choosing and reaching an os-test adherence tier (B5) — B3 measures and reports; the tier needs a date first
- B1 Linux broker prototype — B2 built grant, attenuate and revoke in-kernel, so the prototype is optional
- GUI — not a goal
- Training or hosting frontier models — inference device can be a local model or a remote API later
- Putting the LLM in the trusted computing base — the broker is deterministic code and never asks a model
- SMP — everything runs `-smp 1`; revisit only if B3 forces it (see Context)
- Side channels between agents sharing an inference device — v2 of the threat model

## Context

- Kernel crate: `kernel/` (~1000 non-test lines of Rust), OSTD 0.19.0 pinned in `Cargo.toml` and
  `kernel/Dockerfile`. All build and test commands run through `kernel/osdk.sh` in Docker (see
  `AGENTS.md`); `osdk.sh` passes only `/dev/kvm`, never `--privileged`.
- Knowledge base: `wiki/` (about 90 concept pages from AIOS and OSTEP) and
  `docs/architecture/research/` (microkernel POSIX personalities; Rust kernels and adherence
  measurement). Planning docs cite these; they do not copy them.
- Known B2 gaps B3 inherits (b2-spike-report.md, "Not verified" and "Friction"):
  - SMP: one run queue on the boot CPU; `cow_clone` TLB flush does not wait for remote CPUs;
    interrupt-vs-block race reasoned, not stress-tested.
  - No real idle task; the integrated scheduler is only checked by the boot check, not ktests.
  - Exception messages do not name the faulting thread; futex has no timeouts.
  - Teardown: carved budgets and caps of dead agents are never returned.
  - Kernel writes into user memory skip the CoW path (`VmWriter` errors instead of faulting).
  - A custom scheduler re-implements part of OSTD's undocumented scheduling protocol; OSTD is 0.x
    with no stated API stability, so each upgrade is a deliberate task.
- Adherence baselines (option-b section 5): os-test Minix 73%, Redox 84%, Sortix 85%, FreeBSD
  90%, Linux 92%, musl 94%. os-test's `include`, `namespace` and `limits` suites mostly test
  libc and headers, so kernel-heavy suites are reported separately.
- Threat model and confused-deputy framing: scoping section 6.
- Novelty is assumed, not established: AgenticOS (arXiv 2606.21129) and Agent libOS (2606.03895)
  are not yet ingested.

## Constraints

- **Team and schedule**: one person plus agents, no deadline — research artifact, not a product (answers scoping section 9 questions 1 and 5; revisit if either changes)
- **Tech stack**: Rust on Asterinas OSTD 0.19.0, x86-64 only for v1 — the spike validated this base
- **Safety**: `#![deny(unsafe_code)]` stays crate-wide. `unsafe` is allowed only where no safe workaround exists, confined to small named modules, each block ledgered in `docs/architecture/unsafe-ledger.md` and revisited in a later hardening pass (decided 2026-10-24). Needing an OSTD patch, or an `unsafe` need that cannot be contained, is the recorded signal to re-score the Redox fork in ADR 0002
- **Build/test**: Docker image `agentic-os-kernel-dev:0.19.0` and `kernel/osdk.sh`; KVM host required
- **Compatibility**: agent features are additive; they never change the semantics of existing POSIX interfaces, and denials map to standard errno values
- **Docs**: wiki claims cite PDF pages; design reasoning not stated by a source is marked *(interpretation)*; `raw/` is gitignored and never edited

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| New Rust capability microkernel, not Linux plus an agent layer (ADR 0002) | Agents must be kernel objects; Linux cannot express this natively | — Pending |
| Build on Asterinas OSTD; Redox fork is the fallback | B2 built every primitive plus the budget scheduler and address-space switch on the public API with no unsafe and no patches | ✓ Good (SMP still open) |
| Drop formal POSIX certification, measure adherence with free suites (2026-10-23) | Certification cost and publication limits outweigh benefit for a research artifact | — Pending |
| Per-grant forwarders for revocation | Matches per-delegation revocation; EROS/Coyotos precedent | ✓ Good (B2 tests) |
| uid/gid is a server-side attribute, never kernel authority | Namespace answers "can you name it"; mode bits answer "may you" | — Pending |
| This GSD roadmap covers B3 only | B4 and B5 depend on B3 outcomes (layer cost, libc choice, adherence tier) | — Pending |
| Contained `unsafe` is acceptable; Redox re-score only after safe and contained-unsafe routes fail (2026-10-24) | Avoid abandoning the OSTD base over one low-level need such as FPU state; the cost is paid in a ledger and a hardening pass | — Pending |
| Solo, no deadline recorded as an explicit constraint | Scoping section 9 left schedule open; roadmap needs a stated assumption | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-10-24 after relaxing the unsafe constraint*
