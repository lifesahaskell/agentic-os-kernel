# Requirements: agentic-os-kernel (Linux prototype)

**Defined:** 2026-10-27
**Core Value (working statement):** The protection boundary knows what an agent is: every action an agent takes on files, network, tools, other agents or the model passes a kernel-enforced check against that agent's authority, while ordinary POSIX software runs unchanged. This is taken from scoping-linux.md section 1. PROJECT.md still states the archived microkernel core value and has not been rewritten.

**Scope.** This is the Linux prototype-first milestone of
[ADR 0004](../docs/architecture/adr/0004-linux-prototype-first.md) (status Proposed). It is a
stock-kernel prototype with:

- one cgroup v2 cgroup per agent
- BPF-LSM as the deny-by-default mediation point
- Landlock and a seccomp allowlist as defence in depth
- a userspace broker for identity, budgets, egress, escalation and inference.

A mainline fork is the escalation path, taken only when an ADR 0004 trigger fires. The B3
microkernel milestone is archived in [milestones/b3-microkernel/](milestones/b3-microkernel/README.md).

**Sources.**

- [scoping-linux.md](../docs/architecture/scoping-linux.md): goals 1-5, the floor matrix in
  section 3, success criteria S1-S9 in section 4, fork candidates in section 5, open questions in
  section 6.
- [ADR 0004](../docs/architecture/adr/0004-linux-prototype-first.md): decisions 1-5 and triggers
  (a)-(d).
- The gap analysis
  [261027-ttd-INPUT-gap-analysis.md](quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md):
  rows 1a-4e.
- [linux-baseline.md](../docs/architecture/research/linux-baseline.md): run `20261028T034700Z`.

Every requirement is testable by a harness run, a scripted test, a recorded measurement or a
recorded user decision. "Test" names what would show it worked. "Source" names the S row, goal,
gap-analysis row or ADR 0004 trigger it comes from.

**Threshold rule.** Every number from scoping-linux.md section 4 or linux-baseline.md section 11 is
carried exactly and labelled "proposed, awaiting user acceptance". No requirement accepts a
threshold; the user does that in DEC-01 and DEC-02.

**Evidence rule.** All measured numbers come from one host:

- WSL2 on Hyper-V, kernel 6.18.33.2-microsoft-standard-WSL2
- no `/dev/kvm`, no root
- active LSMs: capability, landlock, yama, safesetid, selinux, ima (no `bpf`)
- Landlock ABI 7.

It is ASSUMED, not shown, that they transfer to bare metal or to the target distros
(linux-baseline.md section 9). NOT MEASURED anywhere: BPF-LSM per-hook overhead,
`BPF_LSM_CGROUP`, the S7 inference queue, and S8 trigger-to-freeze.

## v1 Requirements

### Test environment (ENV)

- [ ] **ENV-01**: A privileged test environment exists. It has root, a kernel with `bpf` in the active LSM list, `bpftool`, clang with the BPF target, `libbpf` headers and a delegated cgroup v2 subtree. Where it comes from is the user's decision; no document in this repo names a source. The current host does not qualify: it has no root, `unprivileged_bpf_disabled=2`, and no `bpf` in the active LSM list. Test: the environment probe on the new host records `bpf` in `/sys/kernel/security/lsm`, a uid-0 shell, the `bpftool` version, and an `lsm/file_open` program loaded and unloaded. Source: linux-baseline.md section 8 (the "Needs" lists for BPF-LSM and `BPF_LSM_CGROUP`).
- [ ] **ENV-02**: The user names one reference kernel and host. ADR 0004 trigger (b) reads S2 and S3 "on the reference kernel", and every S row's baseline is "on the same kernel", but no document defines that kernel. Its floor rows (scoping-linux.md section 3) are probed on the running system, not read from config files. Test: a run-directory environment file gives the kernel version, command line, active LSM list, Landlock ABI and whether each floor row is present, and the user's decision record points to it. Source: ADR 0004 trigger (b); scoping-linux.md section 3.
- [ ] **ENV-03**: The test environment boots the stock kernel of each target distro confirmed in DEC-03. The route, virtual machines or bare metal, is part of the user's ENV-01 decision; this host has no KVM. Test: each distro kernel prints `uname -r`, `/sys/kernel/security/lsm` and its Landlock ABI from a running system. Source: scoping-linux.md section 3 note 2 (a default CONFIG_LSM is not a statement about a running system); ADR 0004 trigger (d).

### Measurement harness (HARN)

- [ ] **HARN-01**: The `tools/linux-baseline` suites (micro, broker, deadline, cgroup; configs C0 to C3) re-run on the reference host. The report states, per S3, S4 and S6 leg, whether the WSL2 numbers of run `20261028T034700Z` transfer within spread. Test: a new committed run directory, and a side-by-side table of the C3-versus-C0 added percent and the broker and deadline p99 rows for both hosts, with stability marks. Source: linux-baseline.md section 9 (transfer ASSUMED) and section 10.
- [ ] **HARN-02**: BPF-LSM per-hook overhead is measured. Config: C3 plus the global `lsm/` programs of linux-baseline.md section 8 (`file_open`, `socket_connect`, `bprm_check_security`), extended with a hook on the unix-socket send path. *(interpretation)* The extension is needed because S3 names unix-socket sendmsg and the prepared programs do not hook it. Legs: `openat_tmpfs`, `openat_rootfs`, `unix_connect`, `tcp_connect`, `fork_exec_wait`, `posix_spawn` and `unix_dgram_pingpong`, each with an empty-map (miss) and a populated-map (hit) variant. Test: summary rows with N, p50, p99, spreads and stability marks; added percent against C0 and against C3; and the R1 arithmetic of linux-baseline.md section 11 redone with the BPF-LSM share included. Source: S3; linux-baseline.md section 8; ADR 0004 trigger (b).
- [ ] **HARN-03**: `BPF_LSM_CGROUP` is exercised and measured. Two items ASSUMED in linux-baseline.md section 9 are checked on the reference host: the `lsm_cgroup` attach through `bpftool`, and the return-1-to-allow convention. A per-cgroup config is then measured on the HARN-02 legs. If the attach type does not work there, the failure is recorded and the floor fallback is measured instead: one global program keyed on `bpf_get_current_ancestor_cgroup_id`. Test: an `lsm_cgroup/file_open` program attached to one cgroup makes `openat` fail with an errno in that cgroup and succeed in a sibling cgroup, and the per-cgroup rows are recorded. Source: ADR 0004 decision 2; gap analysis 2a; scoping-linux.md section 3 (BPF_LSM_CGROUP fallback).

### Adversarial suite (ADV)

- [ ] **ADV-01**: An adversarial suite exists before the mediation it tests. Each case attempts one side effect an agent policy would deny. The cases cover:
  - the classes scoping-linux.md section 1 names: files, network, tools, other agents, the model
  - the section 2.2 threats: io_uring operations, a directly used API key
  - escape through a non-agent child (shell-out).

  Run by a non-agent process on the same kernel, every case's side effect occurs. Test: the runner reports, per case, the side effect observed in the non-agent run; a case whose side effect does not occur there is marked invalid and excluded from the S1 count. Source: S1 (baseline column); goal 2; gap analysis 2a ("completeness is an adversarial-test result"), 2d, 2h, 2i.
- [ ] **ADV-02**: The suite holds the S5 attempt set, each attempt made by a process running with the broker's uid: kill, signal, ptrace, binding the broker's socket name, and presenting another agent's identity on the broker's socket. Against a stub broker every attempt succeeds for a non-agent process. Test: the non-agent baseline run shows each attempt succeeding, which shows the test reaches its target. Source: S5 (measure and baseline columns).
- [ ] **ADV-03**: An early S5 read. With the BPF-LSM `task_kill`, `ptrace_access_check` and unix-socket hooks loaded, the ADV-02 attempts run from an agent cgroup against the stub broker, and the successes are counted. S5 proposed threshold: 0 (proposed, awaiting user acceptance). Test: a count per attempt type, and for each success the path it took. Source: S5; ADR 0004 trigger (c), which names those three hooks.
- [ ] **ADV-04**: S1 on the full stack, with BPF-LSM, Landlock, seccomp and the broker in place: count the adversarial cases in which an agent causes a side effect its policy denies. S1 proposed threshold: 0 (proposed, awaiting user acceptance). Every case that succeeds is classified as closable or not by an attachable LSM hook, a Landlock right or a cgroup-BPF program; a seccomp-only closure is recorded as not closing it. Test: the suite report on the reference kernel, with the per-case classification. Source: S1; ADR 0004 trigger (a), including the hooks that cannot take a BPF-LSM attach (`vm_enough_memory`, `file_alloc_security` and `getprocattr` among them).
- [ ] **ADV-05**: S5 against the real broker, with the `task_kill`, `ptrace_access_check` and unix-socket hooks in place: count the successful kill, signal, ptrace or impersonation attempts by an agent with the broker's uid. S5 proposed threshold: 0 (proposed, awaiting user acceptance). Test: the suite report against the Phase 10 broker, with the non-agent baseline run beside it still succeeding. Source: S5; ADR 0004 trigger (c).

### User decisions (DEC)

- [ ] **DEC-01**: The user accepts, changes or rejects each of S1 to S9, and the POSX-02 row, which no S row covers. A row may also be deferred to a named phase. *(interpretation)* The documents name only accept, change and reject; deferral is added so the gate can close before S7 and S8 have a backend. ADR 0004 stays Proposed while any row is deferred or unset. Test: each row's Status cell in scoping-linux.md section 4 is dated and reads accepted, changed (with the new value), rejected, or deferred (with the phase), and the ADR 0004 status line records the outcome. Source: ADR 0004 status line; scoping-linux.md section 4 and section 6 question 1.
- [ ] **DEC-02**: The S3 openat bound is decided; this is a tracked item.
  - The proposed S3 bound is p50 at most 10% added and p99 at most 25% added (proposed, awaiting user acceptance).
  - It fails for openat on the baseline host with seccomp and Landlock and no BPF-LSM. `openat_tmpfs` adds 25.01% at p50 and 30.09% at p99. `openat_rootfs` adds 17.48% at p50; its 19.29% at p99 is under 25.

  Options on record:
  1. Keep the bound, which fails.
  2. Take the R1 revisions of linux-baseline.md section 11 (all proposed, awaiting user acceptance):
     - p50: 45% for `openat_tmpfs`, 19% for `openat_rootfs`, 14% for `unix_dgram_pingpong`, and 10% kept for the other legs.
     - p99: "none: unstable on this host" for `openat_tmpfs`, `openat_rootfs` and `posix_spawn`, and 25% kept for the other legs.
  3. An absolute added-ns bound for openat. That report names it as an alternative and does not propose it.
  4. Another value.

  Test: the decision names the option, the resulting S3 row, and the run directories it rests on. Those include HARN-02 and HARN-03, so the BPF-LSM share is no longer ungrounded. Source: S3; linux-baseline.md sections 1 and 11.
- [ ] **DEC-03**: The user accepts or changes the floor tiers, the reachability rule and the five target distros: Ubuntu 24.04 LTS, Debian 13, Fedora (current), the RHEL 10 family and Arch. This includes the Landlock ABI 6 minimum, counting Ubuntu as reachable through its HWE kernel, and the 3-of-5 count for trigger (d). All are proposed, awaiting user acceptance. Test: the decision is recorded and the "proposed" labels in section 3 are updated. Source: scoping-linux.md section 3 and section 6 question 2; ADR 0004 trigger (d).
- [ ] **DEC-04**: Local GPU or remote API for inference. This decides whether the dmem row and fork candidate 3 matter, and which backend S7 measures against. Test: the decision is recorded before Phase 14 plans, and the S7 baseline names the backend. Source: scoping-linux.md section 6 question 3 and section 5 candidate 3.
- [ ] **DEC-05**: Add a snapshot-cost criterion, or leave snapshot cost to the inference runtime. Test: the decision is recorded. If a criterion is added, it carries a measure, threshold and baseline in the section 4 format, labelled proposed until accepted. Source: scoping-linux.md section 4 ("rows not carried") and section 6 question 4; gap analysis 3d.

### Agent launcher and containment (LNCH)

- [ ] **LNCH-01**: Each agent is spawned with `clone3` and `CLONE_INTO_CGROUP` into its own cgroup v2 cgroup in a delegated subtree; sub-agents are child cgroups of their parent's. On a kernel without the flag, the fork, move, exec fallback runs, and the test reports the spawn-time window it leaves. Test: the child's first action reads its own cgroup and finds the agent cgroup, and a sub-agent's cgroup is a descendant of its parent's. Source: goal 1; ADR 0004 decision 1; gap analysis 1a; scoping-linux.md section 3 (CLONE_INTO_CGROUP fallback). linux-baseline.md section 9 already shows `CLONE_INTO_CGROUP` working on the baseline host.
- [ ] **LNCH-02**: An agent's identity record (`agent_id`, `on_behalf_of`, parent) is bound to its cgroup id. A peer can resolve it from a pidfd through `PIDFD_GET_INFO` with `PIDFD_INFO_CGROUPID`. Where that is absent, the fallback is `SO_PEERCRED` plus `/proc/PID/cgroup`, which is racy against pid reuse. Test: a peer holding a pidfd for any task in the agent's subtree resolves the same `agent_id`; on the fallback path the test reports the race it cannot close. Source: goal 1; gap analysis 1a, 2f; scoping-linux.md section 3 (PIDFD_GET_INFO fallback).
- [ ] **LNCH-03**: A sub-agent gets a subset of its parent's authority, never more:
  - its Landlock ruleset is layered on the parent's domain
  - its seccomp filter stacks on the parent's
  - `no_new_privs` is set, by `CLONE_NNP` where present, else by `prctl` before exec.

  Test: a sub-agent granted a path its parent lacks still cannot open it; a sub-agent cannot remove an inherited Landlock rule or seccomp filter; and its `/proc/self/status` shows `NoNewPrivs: 1`. Source: goal 1; ADR 0004 decisions 3 and 4; gap analysis 1c; scoping-linux.md section 3 (CLONE_NNP fallback).
- [ ] **LNCH-04**: A non-agent child of an agent (fork, exec of an ordinary binary, `sh -c`) inherits the agent's cgroup, Landlock domain and seccomp filter, so shelling out does not escape. Test: a `sh -c` grandchild reports the agent's cgroup id and is refused the same out-of-grant path as the agent. Source: goal 1; gap analysis 2i.
- [ ] **LNCH-05**: The launcher freezes, thaws and kills an agent's whole subtree with `cgroup.freeze` and `cgroup.kill`, and reports exit to the parent through a pidfd. Test: after a freeze, `cgroup.events` reads `frozen 1` and no task in the subtree runs; after `cgroup.kill` every task is reaped. This retires the ASSUMED item that `cgroup.freeze` works on a delegated leaf. Source: goal 1 (lifecycle); ADR 0004 decision 5; linux-baseline.md section 9 (ASSUMED list).
- [ ] **LNCH-06**: Launcher denials return standard errno values. The seccomp filter is an allowlist that makes no pointer-argument decision. Test: every denial in the Phase 9 test set returns a documented errno rather than killing the process, and the filter source inspects no pointer argument. Source: goal 2 (errno); ADR 0004 decision 4.

### Broker (BRK)

- [ ] **BRK-01**: The broker identifies every caller from kernel-supplied identity (`SO_PASSPIDFD` or `SO_PEERPIDFD`, plus `PIDFD_INFO_CGROUPID`), never from a field in the message. Test: a caller that writes another agent's `agent_id` into a request is served and recorded as itself. Source: gap analysis 2f; ADR 0004 decision 5. linux-baseline.md section 9 shows `SO_PEERPIDFD`, `PIDFD_GET_INFO` and `SO_PEERCRED` returning the caller's identity on the baseline host.
- [ ] **BRK-02**: Tool invocations go through the broker, which checks each against the agent's grant; an ungranted tool is refused with an errno. S4 tool leg: added p99 at most 1 ms over a direct call (proposed, awaiting user acceptance). Test: a granted and an ungranted call with their attribution records, and an S4 run on the reference host. Baseline: on the WSL2 host, `broker_id` added 19116 ns at p99 over `direct`. The p99 spreads, 30.00% (`direct`) and 23.35% (`broker_id`), exceed the 10% eligibility limit of linux-baseline.md section 11, which therefore derives no revision ("none: unstable on this host"). Source: gap analysis 1c (tool and IPC authority in the broker); S4; linux-baseline.md sections 5 and 11.
- [ ] **BRK-03**: Egress goes through the broker at hostname level, and only the broker holds real credentials: the agent holds no API key, and its direct connect to the remote endpoint is denied. S4 egress leg: added p99 at most 1 ms over a direct connect (proposed, awaiting user acceptance). Test:
  - the agent's direct connect fails with an errno
  - the brokered request succeeds
  - no credential appears in the agent's environment, open fds or readable files
  - an S4 egress run on the reference host.

  Baseline: on the WSL2 host, `egress_broker` added 20118 ns at p99 over `egress_direct`. The p99 spreads, 13.41% and 12.63%, exceed the 10% eligibility limit, so section 11 derives no revision. Source: gap analysis 2c, 2h; scoping-linux.md section 2.2; S4.
- [ ] **BRK-04**: The broker counts tokens and money per agent and enforces exhaustion with `cgroup.kill`, `cgroup.freeze` or handle revocation. S6 tokens and money: after exhaustion, overshoot is at most one in-flight request per agent, that is, its max_tokens and its price (proposed, awaiting user acceptance). Test: agents run to exhaustion, and each agent's recorded overshoot is compared with one request's max_tokens and price; a non-agent run with no budget shows that overshoot is unbounded. NOT MEASURED in the baseline. Source: S6; gap analysis 1d; ADR 0004 decision 5.
- [ ] **BRK-05**: Wall-time budgets are enforced by a broker timer plus `cgroup.kill`, or by `CLONE_PIDFD_AUTOKILL` where present. S6 wall time: the agent's cgroup is killed within 1 s of its deadline (proposed, awaiting user acceptance). Test: the deadline-to-reaped distribution with the real broker on the reference host, under same-CPU load. Baseline: on the WSL2 host with four competitors, p99 was 17948443 ns through `pidfd_send_signal` and 3870886 ns through `cgroup.kill`. Both rows are noise-dominated, so section 11 derives no revision; neither exceeds the proposed 1 s. Source: S6; gap analysis 1e; scoping-linux.md section 3 (CLONE_PIDFD_AUTOKILL fallback); linux-baseline.md section 6.
- [ ] **BRK-06**: An action that needs a human goes through an escalation channel the agent holds no fd for and cannot open, so the agent cannot answer its own escalation. Test: the agent's attempts to open, connect to or write the channel fail (ADV-01 cases); a human answer on the channel releases the held action; and a refusal returns an errno. Source: goal 2; gap analysis 2g; ADR 0004 decision 5.
- [ ] **BRK-07**: Every broker-mediated action (tool call, egress request, budget charge, escalation) is recorded with `agent_id` and `on_behalf_of`. Test: each Phase 10 test action leaves one record naming both. Source: goal 2 (attribution); gap analysis 2e.

### Mediation (MED)

- [ ] **MED-01**: BPF-LSM is the deny-by-default mediation point for agents. It is attached per agent with `BPF_LSM_CGROUP`; the fallback is one global program keyed on `bpf_get_current_ancestor_cgroup_id`. The policy covers the file, exec, socket connect, unix-socket, `task_kill` and `ptrace_access_check` hooks. Filesystem decisions use resolved-object hooks rather than seccomp-notify. Denials return errno values. Test: for each hook in the policy, an out-of-grant action from an agent fails with an errno, and the same action from a non-agent succeeds. Source: goal 2; ADR 0004 decision 2; gap analysis 2a, 2b.
- [ ] **MED-02**: Agent identity is kept in cgroup or task storage, and a BPF-LSM `task_alloc` program propagates it to every new task in the agent's subtree. Test: a grandchild forked inside the subtree carries the identity and is denied by the per-agent policy. Source: goal 1; scoping-linux.md section 2.1; gap analysis 1a.
- [ ] **MED-03**: Every mediated kernel event is attributed to `agent_id` and `on_behalf_of` through a BPF ring buffer or through fanotify with a pidfd, not through auditd. Test: each ADV-04 case leaves one record naming the agent and the principal. Source: goal 2; gap analysis 2e; scoping-linux.md section 5 candidate 2 (auditd cannot carry an agent id without a patch).
- [ ] **MED-04**: io_uring cannot perform a side effect an agent's policy denies. The control is task-level io_uring restrictions where the kernel has them (7.0 and later), else `kernel.io_uring_disabled=2`. Test: the ADV-01 io_uring cases fail from an agent on the reference kernel. Source: scoping-linux.md section 2.2 and section 3 (io_uring fallback); gap analysis 2d.
- [ ] **MED-05**: S3 with the production policy. Measure the added latency per mediated call for openat, connect, execve and unix-socket sendmsg, with BPF-LSM, Landlock and seccomp active, against a non-agent on the reference kernel. Read it against the S3 row as decided in DEC-02. As proposed, that row is p50 at most 10% added and p99 at most 25% added (proposed, awaiting user acceptance), a bound openat already fails without BPF-LSM. If a bound is exceeded, a profile attributes the excess to the kernel mediation path or to the broker. Test: a harness run that applies the eligibility rule of linux-baseline.md section 11, plus the profile when a bound is exceeded. Source: S3; ADR 0004 trigger (b).
- [ ] **MED-06**: S2: end-to-end wall time of an agent task suite under full mediation, against the same tasks run by a non-agent. Proposed threshold: median across tasks at most 3%, worst task at most 8.4% (proposed, awaiting user acceptance). These are AgentSight and ActPlane figures on their own workloads: a bar, not a measurement of this design. No document defines the task suite, so it is defined and committed before the run. Test: the committed suite definition, per-task overhead with N and spread, and a profile on any excess as in MED-05. Source: S2; ADR 0004 trigger (b).

### Distro floor (DIST)

- [ ] **DIST-01**: The TO-VERIFY items that decide the floor are resolved:
  - the Landlock ABI at which the abstract-unix-socket and signal scopes first appear (note 7)
  - Debian 13 `CONFIG_CGROUP_BPF` (note 4)
  - the bootloader `lsm=` default on a running install of each distro (note 2)
  - per-cgroup PSI pressure files (note 5).

  Test: each item carries a VERIFIED reading from a primary source or a running system. Source: scoping-linux.md section 3 notes 2, 4, 5 and 7, and section 6 question 6.
- [ ] **DIST-02**: Each confirmed target distro boots its stock kernel. Every Required floor row is read from the running system, the reachability verdict is given under the rule accepted in DEC-03, and the trigger (d) count is reported. On paper the count is 0 of 5 VERIFIED unreachable:
  - Ubuntu 24.04 is reachable on HWE from -39 on, and unreachable on GA 6.8 (Landlock ABI 4).
  - Debian 13 sits exactly at ABI 6.
  - RHEL 10 needs `psi=1` and sits at ABI 6.

  Test: a per-distro probe output, and a verdict table in the section 3 format. Source: scoping-linux.md section 3; ADR 0004 trigger (d).
- [ ] **DIST-03**: On each reachable distro the launcher, broker and mediation run, using the fallback for every Fallback-tier feature that distro lacks, and the adversarial suite gives an S1 count per distro. The fallbacks include:
  - `CLONE_PIDFD_AUTOKILL` and `CLONE_NNP` on Ubuntu, Debian and RHEL
  - `PIDFD_GET_INFO` on Debian
  - io_uring task-level restrictions on Debian and RHEL.

  Test: a per-distro S1 report naming which fallbacks ran. Source: scoping-linux.md section 3 matrix and fallback tiers; S1.

### POSIX non-interference (POSX)

- [ ] **POSX-01**: S9: count the os-test results that differ between a non-agent run and a run inside an agent with a full grant. S9 proposed threshold: 0 (proposed, awaiting user acceptance). It runs on the reference kernel and, *(interpretation)* because fallbacks differ per distro, on each reachable distro. *(interpretation)* A non-agent-versus-non-agent control run measures the suite's own flakiness, so a flaky test is not counted as interference. Test: a per-kernel diff of results, with the control run beside it and the os-test commit named. Source: S9; goal 5.
- [ ] **POSX-02**: A non-agent process sees no change with the prototype loaded. The os-test results of a non-agent run with the prototype's BPF programs, launcher and broker active equal those of a non-agent run with none of them loaded. Goal 5 states this ("change no POSIX semantics for non-agent processes"), but S9 compares agent and non-agent runs and does not measure it. *(interpretation)* The gap matters most for the global-program fallback, which runs for every task. The expected count of 0 follows from goal 5's wording; it is not an S row, so it goes to the user with DEC-01. Test: a diff of the two non-agent runs, with the control run beside it. Source: goal 5; scoping-linux.md section 2.

### Inference (INF)

- [ ] **INF-01**: One inference daemon with one queue serves every agent. It identifies callers by `SO_PASSPIDFD` and the peer's cgroup id, and agents have no direct path to the backend. Test: an agent's direct request to the backend fails with an errno, and brokered requests are attributed per agent. Source: goal 3; ADR 0004 decision 5 (the broker runs the inference queue); gap analysis 3a, 2h.
- [ ] **INF-02**: Inference use is accounted per agent and per human principal in the daemon, and charged against the BRK-04 budgets. For a remote API, the counts come from API responses. Test: per-agent sums for a run equal the totals the backend or API reports. Source: goal 3; gap analysis 3b, 3e.
- [ ] **INF-03**: S7: mean and p99 request turnaround through the queue at N = 1, 4 and 16 concurrent agents, against N non-agent processes sending the same requests directly to the same backend. S7 proposed threshold: mean at most 1.05x and p99 at most 1.10x the baseline at each N (proposed, awaiting user acceptance). The AIOS FIFO numbers (aios p. 9) are reported alongside, not as the threshold. Test: a run at each N on the backend chosen in DEC-04. NOT MEASURED in the baseline, because no backend and no queue exist yet. Source: S7; linux-baseline.md section 8.

### Admission control (ADM)

- [ ] **ADM-01**: S8 (i): count the agents admitted while summed context capacity is exceeded, using `memory.current`, `memory.pressure` triggers and per-agent `memory.max`. S8 (i) proposed threshold: 0 (proposed, awaiting user acceptance). The unit of context capacity, tokens or bytes, is decided in Phase 14. Test: an over-capacity admission sequence and the admitted count; a non-agent run with no admission control reports peak `memory.pressure` and OOM kills. Source: S8; gap analysis 4a, 4b.
- [ ] **ADM-02**: S8 (ii): the time from a `memory.pressure` trigger to freezing the lowest-priority agent. S8 (ii) proposed threshold: at most 1 s (proposed, awaiting user acceptance). Test: a memory-pressure generator and an observer record the trigger-to-frozen distribution; the baseline harness has neither. NOT MEASURED in the baseline. On the WSL2 host, PSI trigger registration works and the delegated service cgroup has the memory controller. Source: S8; gap analysis 4b; linux-baseline.md section 8.
- [ ] **ADM-03**: Memory that the broker and the inference daemon allocate for an agent is attributed to that agent, because memcg charges the allocating task's cgroup. The mechanism is either a worker per agent inside the agent's cgroup, or a per-client ledger in the daemon (ADR 0003's ledger is a candidate design). Test: a request flood from one agent shows up as that agent's memory, in its cgroup or its ledger, and not as another agent's or as unattributed daemon usage. Source: ADR 0004 Consequences; gap analysis 4c.

### Fork decision (FORK)

- [ ] **FORK-01**: ADR 0004 triggers (a) to (d) are scored with measured evidence:
  - (a) from the ADV-04 classification
  - (b) from MED-05 and MED-06, with the profile's attribution
  - (c) from ADV-05
  - (d) from the DIST-02 count.

  Each trigger reads fired, not fired, or not readable (its row not accepted). Test: a scoring record in which every verdict links the run directory it rests on. Source: ADR 0004 "What would show this was wrong".
- [ ] **FORK-02**: For each fired trigger, the try-first step ADR 0004 names is tried and its result recorded before a fork is chosen. The ADR marks each step as interpretation:
  - (a): an adjacent attachable hook, or removing the path from the grant
  - (b): a BPF map lookup for the hot decision
  - (c): the broker under its own uid
  - (d): a narrower distro list.

  If a fork is still indicated, the ranked candidate the trigger promotes is named, and the LTS to pin is decided (scoping-linux.md section 6 question 5). 6.18 and 6.12 are both end of life December 2028 per scoping-linux.md section 5. Test: a record per fired trigger, and a new ADR for a fork. Source: ADR 0004; scoping-linux.md sections 5 and 6.

## v2 Requirements

Deferred. Each is a fork candidate from scoping-linux.md section 5 or a gap-analysis row this
milestone does not build. A fork candidate enters scope only through FORK-02.

- **FORK2-01**: LTS base with carried backports (candidate 1, gap 2d); promoted by trigger (d)
- **FORK2-02**: Audit records carrying agent identity via a `kernel/audit.c` patch (candidate 2, gap 2e); no trigger promotes it
- **FORK2-03**: DRM scheduling cgroup controller for local-GPU time sharing (candidate 3, gap 3c); matters only if DEC-04 picks a local GPU
- **FORK2-04**: In-kernel agent credential and budget controller (candidate 4, gaps 1b and 1f); promoted by triggers (a) and (c)
- **FORK2-05**: `SO_PEERCGROUPID` (candidate 5, gap 2f); ergonomics, no trigger
- **INF2-01**: Preemption of an in-flight inference request (gap 3d); depends on DEC-05
- **ADM2-01**: Eviction with exact reload (gap 4d) and an in-kernel memory-pressure policy (gap 4e)

## Out of Scope

| Feature | Reason |
|---------|--------|
| The Rust microkernel on OSTD (`kernel/`, `userland/`) | Archived at tag `archive/ostd-b2-spike` (ADR 0004). The code stays in the tree, unbuilt by this milestone |
| A mainline fork before a trigger fires | ADR 0004: the fork is the escalation path, advised against until measured |
| POSIX adherence tier | Linux supplies POSIX; replaced by S9 (scoping-linux.md section 4) |
| POSIX layer cost | No personality layer on Linux; S2 and S3 measure the added cost instead |
| Formal POSIX certification | Dropped 2026-10-23 |
| auditd as the attribution channel | Needs a `kernel/audit.c` patch (candidate 2); MED-03 uses a BPF ring buffer or fanotify |
| seccomp as an enforcement point for pointer-argument decisions | ADR 0001 correction; ADR 0004 decision 4 |
| The model, agent or content in the TCB | They stay untrusted (scoping-linux.md section 2.2) |
| Side channels between agents sharing an inference device | Out of the v1 threat model in scoping.md section 6; *(interpretation)* scoping-linux.md does not change this |
| GUI | Not a goal |
| Frontier model training or hosting | Out of research scope |

## Open decisions (need the user; not all are requirements)

- **DEC-01 to DEC-05** above.
- **Privileged environment source** (ENV-01) and **reference kernel** (ENV-02).
- **S2 task suite** (MED-06): no document defines it.
- **os-test commit** for POSX-01 and POSX-02. A pin exists in the B3 stack notes; scoping-linux.md names no commit.
- **Phase 7 anchor.** Its 2026-10-25 "post-MVP" decision was tied to the dropped Phase 6.
- **PROJECT.md rewrite.** Its core value, constraints and key decisions describe the archived design.
- **Phase 2 UAT.** Tests 4 and 5 are pending on a superseded phase: close them as superseded, or finish them on the archived tree.

## Traceability

Each v1 requirement maps to exactly one phase: the phase that first builds or decides it.

| Requirement | Phase | Status |
|-------------|-------|--------|
| ENV-01 | Phase 8 | Pending |
| ENV-02 | Phase 8 | Pending |
| ENV-03 | Phase 12 | Pending |
| HARN-01 | Phase 8 | Pending |
| HARN-02 | Phase 8 | Pending |
| HARN-03 | Phase 8 | Pending |
| ADV-01 | Phase 8 | Pending |
| ADV-02 | Phase 8 | Pending |
| ADV-03 | Phase 8 | Pending |
| ADV-04 | Phase 11 | Pending |
| ADV-05 | Phase 11 | Pending |
| DEC-01 | Phase 8 | Pending |
| DEC-02 | Phase 8 | Pending |
| DEC-03 | Phase 8 | Pending |
| DEC-04 | Phase 14 | Pending |
| DEC-05 | Phase 14 | Pending |
| LNCH-01 | Phase 9 | Pending |
| LNCH-02 | Phase 9 | Pending |
| LNCH-03 | Phase 9 | Pending |
| LNCH-04 | Phase 9 | Pending |
| LNCH-05 | Phase 9 | Pending |
| LNCH-06 | Phase 9 | Pending |
| BRK-01 | Phase 10 | Pending |
| BRK-02 | Phase 10 | Pending |
| BRK-03 | Phase 10 | Pending |
| BRK-04 | Phase 10 | Pending |
| BRK-05 | Phase 10 | Pending |
| BRK-06 | Phase 10 | Pending |
| BRK-07 | Phase 10 | Pending |
| MED-01 | Phase 11 | Pending |
| MED-02 | Phase 11 | Pending |
| MED-03 | Phase 11 | Pending |
| MED-04 | Phase 11 | Pending |
| MED-05 | Phase 11 | Pending |
| MED-06 | Phase 11 | Pending |
| DIST-01 | Phase 12 | Pending |
| DIST-02 | Phase 12 | Pending |
| DIST-03 | Phase 12 | Pending |
| POSX-01 | Phase 12 | Pending |
| POSX-02 | Phase 12 | Pending |
| INF-01 | Phase 14 | Pending |
| INF-02 | Phase 14 | Pending |
| INF-03 | Phase 14 | Pending |
| ADM-01 | Phase 14 | Pending |
| ADM-02 | Phase 14 | Pending |
| ADM-03 | Phase 14 | Pending |
| FORK-01 | Phase 13 | Pending |
| FORK-02 | Phase 13 | Pending |

**Coverage:**
- v1 requirements: 48 total (ENV 3, HARN 3, ADV 5, DEC 5, LNCH 6, BRK 7, MED 6, DIST 3, POSX 2, INF 3, ADM 3, FORK 2)
- Mapped to phases: 48
- Unmapped: 0

---
*Requirements defined: 2026-10-27 for the Linux prototype milestone (ADR 0004). The B3 set is archived in milestones/b3-microkernel/REQUIREMENTS.md.*
*Last updated: 2026-10-27*
