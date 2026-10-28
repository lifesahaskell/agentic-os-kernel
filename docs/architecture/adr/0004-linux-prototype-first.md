# ADR 0004: Linux prototype first, mainline fork as the escalation path

- Status: **Proposed** (direction chosen by Corey 2026-10-27; it moves past Proposed only when the user accepts the success-criteria thresholds in scoping-linux.md section 4, which are unset)
- Date: 2026-10-27
- Supersedes: [ADR 0002](0002-rust-microkernel.md) and [ADR 0003](0003-server-cpu-exemption.md), in direction only. Both stay as records, unedited.
- Scoping: [scoping-linux.md](../scoping-linux.md)
- Archived work: git tag `archive/ostd-b2-spike`, commit `1f1333e`
- Inputs:
  - [261027-ttd-INPUT-gap-analysis.md](../../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md)
  - [261027-ttd-INPUT-fork-and-prior-art.md](../../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md)
  - [261027-ttd-INPUT-distro-kernels.md](../../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md)

## Context

On 2026-10-27 Corey dropped the OSTD/B2 microkernel path and chose Linux integration. The work
done so far is archived at the tag above.

ADR 0002 chose the microkernel on a hard requirement, not on score: agents are kernel objects,
not processes with policy attached. Its own re-score left option A highest at 25. ADR 0003
settled how microkernel servers are charged for client work.

The gap analysis compared mainline Linux 7.3 with goals 1 to 4. Its headline: it found no gap
that must be a kernel patch. The PATCH items are tamper-resistance, atomicity or overhead
improvements, plus per-process GPU time sharing on a local GPU. It measured nothing
(261027-ttd-INPUT-gap-analysis.md, Headline).

## Why stock before fork

- The gap analysis closes with an argument: scoping.md lists mediation overhead and layer cost
  with no targets. A stock prototype produces those baselines cheaply, and a fork should answer a
  measured miss, not an assumed one.
- Carrying cost. Asahi carries about 858 patches and 83,000 lines with weekly rebase hours
  (fork-and-prior-art A1, VERIFIED). The long-lived out-of-tree efforts either shrank by
  upstreaming or had a funded team (A2, interpretation there).
- Prior art. Sandlock, ActPlane and AgentSight build agent confinement and observation on
  unmodified kernels (fork-and-prior-art B1, VERIFIED at abstract or README level). The warning
  from B3 applies: AgentCgroup depends on the unmerged `memcg_bpf_ops` series (README VERIFIED,
  RFC status SNIPPET), so a no-fork prototype can still end up carrying patches.

## Decision

Build a stock-kernel prototype first:

1. One cgroup v2 cgroup per agent, sub-agents as child cgroups, agent identity as the cgroup id
   readable from the pidfd. Spawn with `CLONE_INTO_CGROUP` where the floor provides it (gap
   analysis 1a and the cross-cutting table).
2. BPF-LSM as the deny-by-default mediation point, with per-agent attach through
   `BPF_LSM_CGROUP` (gap analysis 2a, 2b).
3. Landlock as monotone-narrowing self-restriction inherited by children (1c, 2i).
4. seccomp as a syscall allowlist for defence in depth, never the enforcement point for
   pointer-argument decisions (ADR 0001 correction; gap analysis 2b).
5. A userspace broker that holds token, money and wall-time budgets and enforces them with
   `cgroup.kill`, `cgroup.freeze` or handle revocation (1d, 1e). The broker also owns escalation
   over a trusted path the agent holds no fd for (2g), holds the real credentials for egress (2c,
   2h), and runs the inference queue (3a).

A mainline fork is the escalation path. The user accepted it as permissible and did not choose it
first. It is taken only when a trigger below fires.

Corey was advised on 2026-10-27 that a fork is unjustified until measured, because no goal 1-4
gap requires a patch and no criterion has a baseline yet.

## What would show this was wrong

Each trigger reads a row or section of scoping-linux.md and cannot fire until the user accepts
that row.

- **(a) Unclosable bypass.** Reads S1. Fires when the adversarial suite finds a side-effect path
  that no attachable LSM hook, Landlock right or cgroup-BPF program closes. A seccomp-only closure
  does not count, because seccomp is not an enforcement point (ADR 0001 correction). The gap
  analysis lists a set of hooks that cannot take a BPF-LSM attach, among them `vm_enough_memory`,
  `file_alloc_security` and `getprocattr` (goal 1 table, V-src). Promotes rank 4 in
  scoping-linux.md section 5, or a patch adding the hook. Try first: an attachable hook on an
  adjacent path, or removing the path from the agent's grant *(interpretation)*.
- **(b) Mediation overhead above the bound.** Reads S2 (worst task) and S3 (p99). Fires when
  either exceeds its accepted bound on the reference kernel and profiling attributes the excess
  to the kernel mediation path rather than the broker, which a fork does not change.
  *(interpretation)* No ranked candidate targets overhead; the profile picks the change. Try
  first: move the hot decision from the broker into a BPF map lookup *(interpretation)*.
- **(c) Killable or impersonable broker.** Reads S5. Fires when S5 is above 0 with the BPF-LSM
  `task_kill`, `ptrace_access_check` and unix-socket hooks in place. Try first: run the broker
  under its own uid *(interpretation)*; the trigger concerns the same-uid design. Promotes rank 4
  (an agent credential held in the kernel, gap analysis 1b).
- **(d) Feature floor unreachable.** Reads scoping-linux.md section 3 (floor tiers and the
  reachability rule). Fires when 3 or more of the 5 target distros are VERIFIED unreachable.
  Promotes rank 1 (choose an LTS base and carry backports). Try first: narrow the supported
  distro list *(interpretation)*.

## Consequences

- ADR 0002 and ADR 0003 stay as records. The microkernel work is frozen at the archive tag.
  `kernel/` and `userland/` stay in the tree; this ADR deletes nothing.
- ADR 0002's hard requirement no longer excludes option A. On the prototype an agent is a cgroup
  plus BPF-LSM, Landlock and seccomp policy, the shape ADR 0002 excluded. ADR 0001's meaning of
  native (kernel-enforced, not kernel-resident) applies again until a fork trigger fires
  *(interpretation)*.
- Goal 5 comes from Linux. Row S9 replaces the adherence tier.
- The TCB becomes the whole Linux kernel plus the BPF programs, the privileged loader and the
  broker. ADR 0001 rated option A's mediation strength 2 for this reason.
- ADR 0003's problem recurs for the broker and the inference daemon: memcg charges the allocating
  task's cgroup (gap analysis 4c and the goal 4 table, A). ADR 0003's per-client ledger is a
  candidate design for the broker *(interpretation)*.
- PROJECT.md, ROADMAP.md and REQUIREMENTS.md describe the archived design, including the
  core-value statement about capability handles. This ADR does not change them. Aligning them is
  a separate task.
