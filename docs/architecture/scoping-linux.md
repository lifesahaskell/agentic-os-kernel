# Scoping: Linux prototype first

Status: draft 1, 2026-10-27. Companion to ADR 0004 ([adr/0004-linux-prototype-first.md](adr/0004-linux-prototype-first.md)).
Supersedes the direction of [scoping.md](scoping.md), which stays unedited as the record of the
microkernel scoping. Evidence labels follow the inputs this document is built from; section
"Sources" lists each legend. Reasoning that no input states is marked *(interpretation)*.

## 1. The ask, restated

Build an operating system in which LLM agents are principals the system itself schedules,
accounts for and constrains, and which runs ordinary POSIX software unchanged. This is the ask of
scoping.md section 1 with the substrate changed. On 2026-10-27 the OSTD/B2 microkernel path was
dropped and archived at git tag `archive/ostd-b2-spike` (commit `1f1333e`); the work moves to stock
Linux ([ADR 0004](adr/0004-linux-prototype-first.md)). The test for "natively agentic" carries
over: the protection boundary knows what an agent is, so every action an agent takes on files,
network, tools, other agents or the model passes a kernel-enforced check against that agent's
authority. *(interpretation)* On Linux that boundary is the kernel's LSM layer keyed on a
per-agent cgroup. This document fixes the goals, the kernel features the prototype needs, nine
success criteria proposed for the user to accept or change, and the ranked fork candidates.

## 2. Goals

1. Agents are principals. The OS holds each agent's identity (`agent_id`, `on_behalf_of`, parent),
   its authority, its budgets (tokens, money, wall time) and its lifecycle. A sub-agent gets a
   subset of its parent's authority, never more. A non-agent child process inherits the agent's
   identity, so shelling out does not escape (scoping.md 3.1).
2. Every side effect an agent causes is mediated by a deterministic check that the agent cannot
   bypass or disable, and is attributed to `agent_id` and `on_behalf_of`. Denials map to standard
   errno values. Actions that need a human go through an escalation path the agent cannot answer
   itself (scoping.md 3.2, 3.3).
3. Inference is a shared, scheduled, accounted device. A local model or a remote API sits behind
   one interface (scoping.md 3.4).
4. Agent context is accounted memory with admission control (scoping.md 3.5).
5. POSIX now comes from Linux. Agent features stay additive and change no POSIX semantics for
   non-agent processes (scoping.md section 5). This goal is measured by row S9, not by an
   adherence tier.

### 2.1 Prototype mechanisms

- Goal 1: one cgroup v2 cgroup per agent, sub-agents as child cgroups; identity kept in cgroup or
  task storage and propagated by a BPF-LSM program (gap analysis 1a, 1c, 1d, 1e; labels in the
  input: 1a classed BPF with V-src helpers, 1c classed US with a V-doc basis and an A claim on
  seccomp immutability, 1d and 1e classed US). Budgets are counted by a userspace broker, which
  enforces with `cgroup.kill`, `cgroup.freeze` or handle revocation.
- Goal 2: BPF-LSM as the deny-by-default mediation point, Landlock and a seccomp allowlist as
  defence in depth, a broker for egress and credentials (gap analysis 2a and 2b classed BPF; 2c,
  2f, 2g, 2h classed US; 2d needs no patch on 7.0 or later, V-src and V-search; 2e classed BPF or
  US). Completeness is an adversarial-test result: the input found no document claiming LSM hook
  coverage is complete (2a).
- Goal 3: one inference daemon with one queue, identity taken from `SO_PASSPIDFD` and the cgroup id
  of the peer (gap analysis 3a, 3b, 3e, all classed US).
- Goal 4: admission control from `memory.current`, `memory.pressure` triggers and per-agent
  `memory.max` (gap analysis 4a, 4b classed US; 4c classed US or PATCH, with the memcg
  charge-to-allocator rule labelled A in the input).

### 2.2 Trust boundary on Linux

The delta from scoping.md section 6:

- The TCB becomes the Linux kernel, the loaded BPF programs, the privileged BPF loader and the
  broker. ADR 0001 rated option A's mediation strength 2 for the size of that base.
- Model, agent and content stay untrusted. The human user stays the source of authority.
- New threat: an agent that shares a uid with the broker acts against it (row S5, ADR 0004
  trigger (c)).
- New threat: io_uring operations bypass seccomp (gap analysis 2d, V-search); closed by task-level
  restrictions or the `kernel.io_uring_disabled=2` sysctl.
- New threat: an agent that holds an API key calls a remote service directly (gap analysis 2h);
  closed only by a broker that holds the credential.

## 3. Feature floor

The matrix lists the kernel features the prototype's mechanisms use and what each target distro
ships. Every value and label is carried from
[261027-ttd-INPUT-distro-kernels.md](../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md),
whose evidence date is 2026-10-27. "Present" means in the source tree of that distro kernel;
"enabled" means the shipped config turns it on by default (that input's definitions). Ubuntu ships
two kernels on 24.04, so each Ubuntu cell gives GA and HWE. A cell with no entry in that input is
TO-VERIFY.

| Feature | First in mainline | Ubuntu 24.04 LTS (GA 6.8.0-147; HWE 7.0.0-42) | Debian 13 (6.12.113) | Fedora (current) (44, 7.2.13) | RHEL 10 family (6.12.0-211.68.1.el10_2) | Arch (7.2.11) |
|---|---|---|---|---|---|---|
| CLONE_INTO_CGROUP | 5.7 VERIFIED | GA present; HWE present VERIFIED | present VERIFIED | present VERIFIED | present VERIFIED (base tag) | present VERIFIED |
| CLONE_PIDFD_AUTOKILL | 7.1 VERIFIED | GA absent; HWE absent VERIFIED | absent VERIFIED | present VERIFIED | absent VERIFIED (base tag) | present VERIFIED |
| CLONE_NNP | 7.1 VERIFIED | GA absent; HWE absent VERIFIED | absent VERIFIED | present VERIFIED | absent VERIFIED (base tag) | present VERIFIED |
| PIDFD_GET_INFO (with PIDFD_INFO_CGROUPID) | 6.13 VERIFIED | GA absent; HWE present VERIFIED | absent VERIFIED | present VERIFIED | present, backport VERIFIED (base tag) | present VERIFIED |
| BPF_LSM_CGROUP attach type | 6.0 VERIFIED | GA source present, needs lsm= override; HWE -42 usable, -38 needs override VERIFIED | usable on BPF_LSM=y and bpf in lsm VERIFIED; CONFIG_CGROUP_BPF TO-VERIFY | usable VERIFIED | usable VERIFIED | usable VERIFIED |
| CONFIG_BPF_LSM=y | 5.7 (BPF LSM) VERIFIED | GA y; HWE y VERIFIED | y VERIFIED | y VERIFIED | y VERIFIED | y VERIFIED |
| bpf in the default LSM order (else the lsm= boot parameter must add it) | TO-VERIFY | GA no; HWE no in -38, yes in -39 and -42 VERIFIED | yes VERIFIED | yes VERIFIED | yes VERIFIED | yes VERIFIED |
| Landlock ABI level | 5.13 (Landlock); ABI 4 at 6.7, 6 at 6.12, 10 at 7.2 VERIFIED | GA ABI 4; HWE ABI 8 VERIFIED | ABI 6 VERIFIED | ABI 10 VERIFIED | ABI 6, backport level VERIFIED | ABI 10 VERIFIED |
| io_uring task-level restrictions | 7.0 VERIFIED | GA absent; HWE present VERIFIED | absent VERIFIED | present VERIFIED | absent; io_uring_disabled defaults to 2 VERIFIED (base tag) | present VERIFIED |
| memcg (cgroup v2 memory controller) | 4.5 VERIFIED | GA y; HWE y VERIFIED | y VERIFIED | y VERIFIED | y VERIFIED | y VERIFIED |
| PSI per-cgroup pressure files | 4.20 (PSI), 5.2 (triggers) VERIFIED; per-cgroup files TO-VERIFY | GA CONFIG_PSI y, on by default; HWE same VERIFIED | CONFIG_PSI y, on by default VERIFIED | CONFIG_PSI y, on by default VERIFIED | CONFIG_PSI y, off by default, needs psi=1 VERIFIED | CONFIG_PSI y, on by default VERIFIED |
| dmem cgroup controller | 6.14 VERIFIED | GA absent; HWE y VERIFIED | absent VERIFIED | y VERIFIED | y, backport VERIFIED | y VERIFIED |

Notes:

1. "VERIFIED (base tag)" marks RHEL cells whose source checks read the base tag
   `kernel-6.12.0-211.el10`; the shipped z-stream source was not read. Its configs come from the
   shipped rpm (distro input, section 5).
2. Row 7 first-in-mainline is TO-VERIFY because the distro input gives no mainline version for a
   distro CONFIG_LSM choice. *(interpretation)* The column does not apply to that row. The "yes"
   cells state the default CONFIG_LSM; the input did not examine bootloader `lsm=` overrides, so a
   default of yes is not a statement about a running system (distro input, section 5).
3. Rows 6 and 7 split the user's single "CONFIG_BPF_LSM plus lsm=bpf" item, and rows 10 to 12 split
   "memcg/PSI/dmem", because each is a separate fact per distro *(interpretation)*.
4. Debian row 5: the input did not record CONFIG_CGROUP_BPF for Debian and says to recheck it
   before relying on BPF_LSM_CGROUP there (distro input, section 5).
5. Row 11: the distro input records PSI (4.20) and its triggers (5.2) and the distro CONFIG_PSI
   values; it has no entry for per-cgroup pressure files, so that part is TO-VERIFY here. The gap
   analysis reads `memory.pressure` per cgroup from cgroup-v2 documentation (V-doc).
6. Row 9: the gap analysis puts task-level io_uring restrictions at 7.0 on a search summary
   (261027-ttd-INPUT-gap-analysis.md 2d, V-search) and says a 6.18-based fork needs a backport; the
   distro input verifies 7.0 by adjacent-tag presence. The values agree; each label stays with its
   source.
7. Row 8: Sandlock needs Linux 6.12 or later for Landlock ABI v6 and uses IPC scoping
   (261027-ttd-INPUT-fork-and-prior-art.md B1, VERIFIED at README level). No input states the ABI
   at which abstract-unix-socket and signal scopes first appear: TO-VERIFY. The distro input
   records TCP network rules at ABI 4 and UDP at ABI 10, both VERIFIED.
8. Row 3: the distro input located the CLONE_NNP implementation in `kernel/fork.c` at v7.2.13 but
   not its commit id (VERIFIED for presence).
9. Open conflict: the distro input lists Fedora 44 kernel 7.2.13 as "stable 2026-10-28" while its
   evidence date is 2026-10-27, and the fork-and-prior-art input dates stable 7.2.13 to
   2026-10-26 (VERIFIED, A0). The cell values do not depend on this date; the date itself is
   TO-VERIFY.
10. The distro input also covers sched_ext, the io_uring BPF opcode filter, seccomp user
    notification, Landlock network rules, unprivileged user namespaces (including Ubuntu's
    AppArmor restriction, partly TO-VERIFY there) and the cgroup v2 default. They are not rows
    here.

Floor tiers (proposed, awaiting user acceptance):

- Required *(interpretation)*: CONFIG_BPF_LSM=y; bpf in the active LSM order, by default or via the
  `lsm=` boot parameter; Landlock at ABI 6 or higher, the level Sandlock requires for a feature set
  that includes IPC scoping (note 7; that scopes first appear at or below ABI 6 is TO-VERIFY);
  memcg; PSI.
- Fallback available, CLONE_INTO_CGROUP: fork, move the child into the cgroup, then exec. This
  leaves the spawn-time window the flag removes (gap analysis 1a).
- Fallback available, CLONE_PIDFD_AUTOKILL: a broker timer plus `cgroup.kill` (gap analysis 1e).
- Fallback available, CLONE_NNP: set `no_new_privs` with `prctl` in the child before exec
  *(interpretation; not in the inputs)*.
- Fallback available, PIDFD_GET_INFO: `SO_PEERCRED` plus a read of `/proc/PID/cgroup`, which is
  racy against pid reuse *(interpretation; not in the inputs)*.
- Fallback available, BPF_LSM_CGROUP: one global BPF-LSM program keyed on
  `bpf_get_current_ancestor_cgroup_id` (gap analysis 2a).
- Fallback available, io_uring task-level restrictions: the sysctl `kernel.io_uring_disabled=2`
  (gap analysis 2d).
- Fallback available, dmem: needed only for local-GPU VRAM limits, not for remote-API inference
  (gap analysis 3e).

Reachability rule (proposed, awaiting user acceptance): the floor is reachable on a distro when
every Required row is present in its stock kernel. A kernel command-line change counts as
reachable and a kernel rebuild counts as unreachable. A TO-VERIFY cell is unknown and counts
neither way. ADR 0004 trigger (d) fires when 3 or more of the 5 target distros are VERIFIED
unreachable. Verdicts from the matrix, under the Required list above:

- Ubuntu 24.04 LTS: reachable on the HWE kernel from -39 on (bpf in the default order, Landlock
  ABI 8); unreachable on the GA 6.8 kernel, deciding row Landlock ABI (4, no command-line fix).
  *(interpretation)* Counted as reachable, because HWE is a stock opt-in kernel of the same
  release.
- Debian 13: reachable; the deciding row is Landlock ABI, exactly at the proposed minimum of 6.
- Fedora: reachable; no Required row falls short.
- RHEL 10 family: reachable after one command-line change, `psi=1`; the Landlock row is at the
  proposed minimum of 6.
- Arch: reachable; no Required row falls short.

Count for trigger (d): 0 of 5 VERIFIED unreachable, with Landlock ABI 6 as the open question for
Debian and RHEL (note 7).

## 4. Success criteria (proposed)

Every threshold below was proposed by the author of this document. None is accepted. The user
accepts, changes or rejects each row before ADR 0004 can leave Proposed. Where a Grounding cell
says "ungrounded", no input measures the quantity.

| ID | Criterion | Measure | Proposed threshold | Baseline | Grounding | Status |
|---|---|---|---|---|---|---|
| S1 | Mediation completeness (goal 2) | Adversarial-suite cases in which an agent causes a side effect its policy denies | 0 | The same suite run by a non-agent process on the same kernel, where every case's side effect occurs, which shows each case reaches a real path | scoping.md section 2 (target 0); gap analysis 2a and "Not found" (no source shows LSM hook coverage is complete; completeness is an adversarial-test result) | proposed, awaiting user acceptance |
| S2 | Workload overhead (goal 2) | End-to-end wall time of an agent task suite under full mediation | Median across tasks at most 3%, worst task at most 8.4% | The same tasks run by a non-agent process on the same kernel | AgentSight under 3% (authors' claim) and ActPlane 1.9-8.4%, both VERIFIED at abstract level (fork-and-prior-art B1). *(interpretation)* These are other systems' overheads on their own workloads: a bar, not a measurement of this design | proposed, awaiting user acceptance |
| S3 | Per-call overhead on kernel-enforced paths (goal 2) | Added latency per mediated call for openat, connect, execve and unix-socket sendmsg microbenchmarks with BPF-LSM, Landlock and seccomp active | p50 at most 10% added, p99 at most 25% added | The same call by a non-agent process on the same kernel | Ungrounded: the gap analysis ("Not found") looked up no BPF-LSM or Landlock overhead numbers. *(interpretation)* Revise after the first baseline run | proposed, awaiting user acceptance |
| S4 | Broker round trip (goals 2 and 3) | Added latency for a tool invocation and for an egress connect routed through the broker | p99 at most 1 ms added | The same call made directly by a non-agent process on the same kernel | Ungrounded: no input measures this. *(interpretation)* Broker-routed egress is gap analysis 2c | proposed, awaiting user acceptance |
| S5 | Broker integrity (goal 1) | Attempts by an agent running with the broker's uid to kill, signal, ptrace or impersonate the broker (bind its socket name, or present another agent's identity on its socket) that succeed | 0 | The same attempts by a non-agent process with that uid on the same kernel, which succeed, showing the protection is agent-specific and that the test reaches the target | Gap analysis 1a (agents unprivileged in a delegated subtree), 2f (`SO_PASSPIDFD` plus `PIDFD_INFO_CGROUPID`), and the goal 2 table (`task_kill` and `ptrace_access_check` hooks, V-src) | proposed, awaiting user acceptance |
| S6 | Budget enforcement (goal 1) | Overshoot after a token, money or wall-time budget is exhausted | Tokens and money: at most one in-flight request per agent (its max_tokens and its price). Wall time: the agent's cgroup killed within 1 s of its deadline | The same workload by a non-agent process on the same kernel, which has no budget, so overshoot is unbounded | Gap analysis 1d, 1e (mechanism, classed US). The numbers are ungrounded: no input measures this. *(interpretation)* | proposed, awaiting user acceptance |
| S7 | Inference scheduling (goal 3) | Mean and p99 request turnaround through the broker's inference queue at N = 1, 4 and 16 concurrent agents | Mean at most 1.05x and p99 at most 1.10x the baseline at each N | The same requests sent directly to the same backend by N non-agent processes on the same kernel | Gap analysis 3a (mechanism). scoping.md section 2 named the AIOS FIFO numbers (aios p. 9) as its baseline; report them alongside, not as the threshold. The numbers are ungrounded: no input measures this. *(interpretation)* | proposed, awaiting user acceptance |
| S8 | Admission control (goal 4) | (i) Agents admitted while summed context capacity is exceeded; (ii) time from a memory.pressure trigger to freezing the lowest-priority agent | (i) 0; (ii) at most 1 s | The same workload by non-agent processes on the same kernel with no admission control, reporting peak memory.pressure and OOM kills | Gap analysis 4b (mechanism, classed US). The numbers are ungrounded: no input measures this. *(interpretation)* | proposed, awaiting user acceptance |
| S9 | POSIX non-interference (goal 5) | Tests whose result differs between a non-agent run and a run inside an agent with a full grant; suite: os-test, as scoping.md section 2 names it | 0 | The same suite run by a non-agent process on the same kernel | scoping.md section 5 rule 1 (the scores must match) | proposed, awaiting user acceptance |

Rows of scoping.md section 2 not carried:

- POSIX adherence tier: Linux supplies POSIX; replaced by S9.
- POSIX layer cost: there is no personality layer on Linux; S2 and S3 measure the added cost
  instead.
- Snapshot cost: an inference-runtime property (gap analysis 3d); no proposal here, listed in
  section 6.

## 5. Fork candidates

Ranked as in the gap analysis ("Where a fork buys something real, ranked",
261027-ttd-INPUT-gap-analysis.md). Each names the gap rows it closes and the ADR 0004 trigger that
would promote it.

1. **Backports for an LTS base.** Closes 2d and the choice between a 6.18 LTS base and 7.x: a
   6.18 base needs io_uring task-level restrictions, the BPF filter and recent sched_ext
   sub-schedulers carried (V-search for the 7.0 landing; V-src for the code). The gap analysis
   calls this a decision, not a gap. Promoted by trigger (d).
2. **Audit records with agent identity.** Closes 2e: auditd cannot carry an agent identifier
   without a patch to `kernel/audit.c` (V-src); a BPF event stream is the alternative. Needed only
   if auditd output itself is a requirement. No trigger promotes it.
3. **DRM scheduling cgroup controller.** Closes 3c, local GPU only: weight-based GPU-time sharing
   is an unmerged RFC (V-search). Depends on scoping.md open question 3, local GPU or remote API.
4. **In-kernel agent credential and budget controller.** Closes 1b and 1f: hardening and
   uniformity, not capability (gap analysis). Promoted by triggers (a) and (c). *(interpretation)*
   Trigger (b) promotes no ranked candidate: the profile picks the change.
5. **`SO_PEERCGROUPID`.** Closes 2f: ergonomics, a small patch to `net/core/sock.c` and
   `net/unix/af_unix.c` (A that those are the files). No trigger promotes it.

Carrying cost (261027-ttd-INPUT-fork-and-prior-art.md):

- Asahi Linux carries 858 patches and about 83,000 lines at 6.18.8, with rebase work of hours on
  an almost weekly basis during the 6.13 cycle (A1, VERIFIED). The input's reading that cost is
  driven by the number of existing files touched rather than line count is labelled ASSUMED there
  (A2).
- Pin one LTS, 6.18 or 6.12, both end of life December 2028 (A0, VERIFIED; the dates have moved
  before, SNIPPET), and rebase about once a year. The input labels that policy ASSUMED (A2).

fork-and-prior-art B3 names a kernel-recognised credential, a trusted escalation path and
kernel-enforced token and money budgets as fork candidates (ASSUMED there). The gap analysis
places the credential and budget controller at rank 4 and the trusted path in userspace (2g).
This document keeps the gap analysis ranking.

## 6. Open questions

1. Accept, change or reject each of S1 to S9. ADR 0004 stays Proposed until then.
2. Accept or change the floor tiers and the reachability rule, and confirm the five target
   distros: Ubuntu 24.04 LTS, Debian 13, Fedora (current), RHEL 10 family, Arch.
3. Local GPU or remote API (scoping.md open question 3). It decides whether the dmem row and fork
   candidate 3 matter.
4. Add a snapshot-cost criterion, or leave it to the inference runtime.
5. Which LTS a fork would pin if a trigger fires.
6. Resolve the TO-VERIFY items that decide the floor: the Landlock ABI at which scopes first
   appear (note 7), Debian CONFIG_CGROUP_BPF (note 4), and bootloader `lsm=` defaults per distro
   (note 2).

## Sources

- [261027-ttd-INPUT-gap-analysis.md](../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md):
  V-src is source read at tag v7.3; V-doc is a fetched page summarised by a small model
  (second-hand); V-search is a search snippet or news page only; A is ASSUMED.
- [261027-ttd-INPUT-fork-and-prior-art.md](../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md):
  VERIFIED is a page fetched and read; SNIPPET is a search summary only; ASSUMED is inference or
  memory.
- [261027-ttd-INPUT-distro-kernels.md](../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md):
  VERIFIED is a primary source read in that session; TO-VERIFY is not established, with a reason
  given. That file's own legend is the authority for its labels.
- [scoping.md](scoping.md), [adr/0001-substrate.md](adr/0001-substrate.md),
  [adr/0002-rust-microkernel.md](adr/0002-rust-microkernel.md),
  [adr/0004-linux-prototype-first.md](adr/0004-linux-prototype-first.md).
