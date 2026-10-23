# ADR 0002: New Rust capability microkernel, POSIX adherence without certification

- Status: **Exploring** (direction chosen by Corey 2026-10-23; not yet Accepted)
- Date: 2026-10-23
- Supersedes: [ADR 0001](0001-substrate.md)
- Design exploration: [option-b-microkernel.md](../option-b-microkernel.md)

## Context

ADR 0001 recommended Linux plus an agent subsystem (option A). The recommendation rested on the
criterion "certification feasibility" (weight 3). Corey changed the goal:

> Adhere to POSIX, without necessarily certifying as POSIX compliant.

He also chose to explore option B: a new Rust microkernel where agents are native kernel
objects holding capability handles, with POSIX provided by a libc and personality servers.

## The scores do not pick B. The requirement does.

Re-scoring ADR 0001 honestly: drop the certification row and add a "POSIX adherence feasibility"
row (weight 2; A 3, B 1, C1 2, C2 1):

| | A. Linux | B. New kernel | C1. Redox | C2. seL4 |
|---|---|---|---|---|
| ADR 0001 total, certification corrected | 25 | 21 | 19-25 | 21 |
| minus certification row (weight 3) | 19 | 18 | 16-19 | 18 |
| plus adherence row (weight 2) | **25** | 20 | 20-23 | 20 |

A still scores highest. B is justified only by a **hard requirement**: *agents are kernel
objects, not ordinary processes with policy attached.* Under that requirement A is excluded
rather than out-scored, and the real comparison is B against C1 and C2. Writing this down
keeps the decision from looking like a numbers result when it is a values result.
*(interpretation)*

## Decision (exploratory)

Explore a **new Rust capability microkernel** with:

1. **Agents as kernel objects.** An agent is a protection domain with a credential
   (`agent_id`, `on_behalf_of`, parent, budgets) and a capability space. It is not a process
   with a label.
2. **No ambient authority for agents.** Everything an agent can touch is a handle it was
   given. Sub-agents get attenuated copies of their parent's capabilities.
3. **POSIX as a personality.** A Rust libc plus userspace servers (process manager, VFS,
   network, TTY) implement POSIX on top of capabilities. Non-agent POSIX programs run against
   that personality.
4. **POSIX adherence measured, not certified.** The bar is defined in
   [option-b-microkernel.md](../option-b-microkernel.md) using free suites. No Open Group
   registration.

"New" is not "from zero": whether to build on an existing Rust kernel or library (Redox,
Asterinas OSTD, rust-sel4) is open question B1 in the design doc.

## What would show this was wrong

- **The personality cost eats the project.** If after the first milestone budget the POSIX
  layer still cannot run a shell, coreutils and the test harness, the effort has become a
  POSIX re-implementation project with an agent feature attached. Revisit C1 (fork Redox) or D
  (microkernel hosting a Linux VM).
- **Ambient authority leaks back in.** If making real POSIX programs work forces giving agents
  a uid-wide view of the file system, then agents-as-kernel-objects buys nothing over A, and A
  is cheaper.
- **Nobody runs it.** If the deliverable is a research artifact, B is defensible. If it is a
  product, the years of POSIX work are the product, and A or D ship sooner.

## Consequences

- The certification research in [posix-certification.md](../posix-certification.md) no longer
  sets goals. It stays as the reference for what full POSIX.1 Base requires.
- Time to the first measurable prototype goes from weeks to quarters. The agent mediation
  model can still be prototyped early on any host, because it is a capability protocol and not
  a kernel feature (see the milestones in [scoping.md](../scoping.md)).
- Option D (microkernel hosting a Linux VM) stays the fallback when POSIX breadth matters more
  than POSIX-inside-the-capability-model.
