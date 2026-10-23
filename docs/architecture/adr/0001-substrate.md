# ADR 0001: Kernel substrate

- Status: **Proposed** (needs Corey's decision)
- Date: 2026-10-23
- Inputs: [scoping.md](../scoping.md), [posix-certification.md](../posix-certification.md),
  [wiki bridge page](../../../wiki/concepts/aios-ostep-mapping.md)

## Decision to make

Pick the base for an OS that has to (a) earn formal POSIX certification and (b) treat agents as
first-class, kernel-mediated principals. AIOS takes a different approach: its agent services
are a library-level layer with no protection boundary
([aios-ostep-mapping](../../../wiki/concepts/aios-ostep-mapping.md)).

## Options

**A. Linux kernel + agent subsystem.** Agents run as Linux processes under a dedicated
credential/namespace. Mediation is kernel-enforced through existing hooks: an LSM (or Landlock)
for files, seccomp user-notification for trapping selected syscalls to a broker, cgroups for
accounting, and namespaces for isolation. A privileged agent broker (the agentic "kernel
server") holds capabilities and policy. Inference is a scheduled device behind that broker.

**B. New Rust microkernel + POSIX personality.** Agents are native kernel objects with
capability handles. POSIX is provided by a libc and personality servers (the Redox/relibc
pattern).

**C. Fork an existing non-Linux kernel.**
- *C1, Redox*: Rust microkernel. POSIX conformance work is underway, but it has never been
  certified.
- *C2, seL4*: formally verified and capability-native, with a thin POSIX story. No seL4 system
  has been certified.

## Scoring

The scores are judgments, with the evidence noted. 3 is best.

| Criterion | Weight | A. Linux | B. New kernel | C1. Redox | C2. seL4 |
|---|---|---|---|---|---|
| Certification feasibility | 3 | **3**: a Linux system has passed UNIX 03 (EulerOS 2016); glibc, bash and coreutils already target POSIX | 1: must pass ~34,600 tests from zero | 1-2: years in, no attempt yet | 1: no POSIX userland to speak of |
| Agent mediation strength | 3 | 2: seccomp-unotify and LSM are real trap points, but the TCB is huge (the whole Linux kernel) | **3**: designed in | 2-3: capability-ish schemes | **3**: capabilities plus a verified kernel |
| "Natively agentic" fit | 2 | 2: agents are processes with extra kernel-enforced policy, not new kernel objects | **3** | 2 | 3 |
| Time to first measurable prototype | 2 | **3**: weeks | 1: years | 2 | 1 |
| Reversibility | 1 | **3**: the broker/policy API can be ported to B or C later | 1 | 2 | 1 |
| **Weighted total** (max 33) | | **28** | 21 | 19-25 | 21 |

## Recommendation

**Option A.** Build the agent principal, mediation broker and inference scheduler on Linux, and
certify the system to 1003.1-2016 Base. Keep the agent API (the syscalls and broker protocol in
[scoping.md](../scoping.md)) independent of Linux, so a later move to B or C2 changes only the
implementation.

The assumption this rests on is that **Linux's existing trap points (LSM hooks, seccomp
user-notification, fanotify, eBPF) can express every mediation rule the agent model needs, at
an acceptable cost.** Prototype milestone M1 in the scoping doc measures exactly this. If it
fails, for example because a required rule cannot be enforced, or the per-call broker round
trip is too slow for agent workloads, revisit this ADR in favor of C2 (seL4) with a POSIX
compatibility layer, and accept that certification moves out by years.

## What would show this was wrong

- M1 finds a mediation requirement that cannot be enforced without patching the core kernel in
  ways upstream will not take. The fork's maintenance cost then erodes A's advantage.
- The Linux kernel's TCB size becomes the dominant threat in the threat model, for example
  because the target market requires verified isolation.
- Certification turns out to require a kernel the team doesn't control. This is unlikely: the
  certification is for the product build, not for upstream.

## Consequences

- "Native" means kernel-enforced, not kernel-resident. That is a deliberate narrowing of the
  original ask, recorded here so it can be challenged.
- The team inherits Linux's attack surface. Mitigation: agents run in restrictive namespaces and
  seccomp filters by default, and every privilege is granted, never inherited.
