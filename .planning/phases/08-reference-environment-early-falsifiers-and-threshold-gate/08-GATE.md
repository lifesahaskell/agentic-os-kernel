Marked on:

# Phase 8 threshold gate sheet

This sheet is for the user to mark. The agent writes no Mark and no Answer. Every Mark line and every Answer line below ships blank (DEC-01, T-08-10). A recommendation printed in a block is information only; it fills nothing.

Allowed marks: accepted; changed followed by the new value; rejected; deferred to Phase N.

How to use it: read docs/architecture/research/bpflsm-reference.md, then type one allowed mark after each of the 17 Mark colons and the date after "Marked on:". Type the bare value after each of the 3 Answer colons. Evidence and Recommendation lines that read as placeholders are filled by plan 08-06 Task 1 from the run directories before you mark; they are not yours to fill.

Evidence labels used on this sheet: VERIFIED (read from a cited source or a recorded run), ASSUMED (kernel behaviour restated from 08-RESEARCH.md assumptions A1-A17 and not measured here), NOT MEASURED, and *(interpretation)*.

Host note: this sheet was written on a WSL2 host with no root, no /dev/kvm and no active BPF-LSM. Nothing below was measured on the reference host.

## Part 1. Success criteria S1-S9 and POSX-02

Measure, Proposed threshold and Baseline are copied from docs/architecture/scoping-linux.md section 4 (S1-S9) and .planning/REQUIREMENTS.md POSX-02, not recomputed.

### S1. Mediation completeness (goal 2)

Measure: Adversarial-suite cases in which an agent causes a side effect its policy denies
Proposed threshold: 0
Baseline: The same suite run by a non-agent process on the same kernel, where every case's side effect occurs, which shows each case reaches a real path
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

### S2. Workload overhead (goal 2)

Measure: End-to-end wall time of an agent task suite under full mediation
Proposed threshold: Median across tasks at most 3%, worst task at most 8.4%
Baseline: The same tasks run by a non-agent process on the same kernel
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

### S3. Per-call overhead on kernel-enforced paths (goal 2)

Measure: Added latency per mediated call for openat, connect, execve and unix-socket sendmsg microbenchmarks with BPF-LSM, Landlock and seccomp active
Proposed threshold: p50 at most 10% added, p99 at most 25% added
Baseline: The same call by a non-agent process on the same kernel
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

### S4. Broker round trip (goals 2 and 3)

Measure: Added latency for a tool invocation and for an egress connect routed through the broker
Proposed threshold: p99 at most 1 ms added
Baseline: The same call made directly by a non-agent process on the same kernel
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

### S5. Broker integrity (goal 1)

Measure: Attempts by an agent running with the broker's uid to kill, signal, ptrace or impersonate the broker (bind its socket name, or present another agent's identity on its socket) that succeed
Proposed threshold: 0
Baseline: The same attempts by a non-agent process with that uid on the same kernel, which succeed, showing the protection is agent-specific and that the test reaches the target
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Per-route evidence (D-16). One row per aok-adv list-routes row.

| route | topology | reach | agent_result | deny_counter_delta |
|-------|----------|-------|--------------|--------------------|

Note on the table: plan 08-06 Task 1 adds one row per aok-adv list-routes row from the merged ADV report, 23 rows in all (17 sibling, 6 ancestor). The reach column is either reachable or void (baseline failed). A void row means the non-agent baseline could not perform the attempt, so the route was never tested; for S5 a void row is read as untested, never as a pass (B5, D-16).

Mark:

### S6. Budget enforcement (goal 1)

Measure: Overshoot after a token, money or wall-time budget is exhausted
Proposed threshold: Tokens and money: at most one in-flight request per agent (its max_tokens and its price). Wall time: the agent's cgroup killed within 1 s of its deadline
Baseline: The same workload by a non-agent process on the same kernel, which has no budget, so overshoot is unbounded
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

### S7. Inference scheduling (goal 3)

Measure: Mean and p99 request turnaround through the broker's inference queue at N = 1, 4 and 16 concurrent agents
Proposed threshold: Mean at most 1.05x and p99 at most 1.10x the baseline at each N
Baseline: The same requests sent directly to the same backend by N non-agent processes on the same kernel
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: deferred 14 (D-19)
Note: the broker's inference queue does not exist before Phase 14, so there is nothing to time in Phase 8. If S7 or S8 is marked deferred, ADR 0004 stays Proposed (D-19). The user may override the recommendation.

Mark:

### S8. Admission control (goal 4)

Measure: (i) Agents admitted while summed context capacity is exceeded; (ii) time from a memory.pressure trigger to freezing the lowest-priority agent
Proposed threshold: (i) 0; (ii) at most 1 s
Baseline: The same workload by non-agent processes on the same kernel with no admission control, reporting peak memory.pressure and OOM kills
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: deferred 14 (D-19)
Note: admission control is a Phase 14 component, so there is nothing to time in Phase 8. If S7 or S8 is marked deferred, ADR 0004 stays Proposed (D-19). The user may override the recommendation.

Mark:

### S9. POSIX non-interference (goal 5)

Measure: Tests whose result differs between a non-agent run and a run inside an agent with a full grant; suite: os-test
Proposed threshold: 0
Baseline: The same suite run by a non-agent process on the same kernel
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

### POSX-02. A non-agent process sees no change with the prototype loaded

Measure: The os-test results of a non-agent run with the prototype's BPF programs, launcher and broker active, against those of a non-agent run with none of them loaded (REQUIREMENTS.md POSX-02). Goal 5 states this, but S9 compares agent and non-agent runs and does not measure it. *(interpretation)* The gap matters most for the global-program fallback, which runs for every task.
Proposed threshold: 0 (a count of differing test results; follows from goal 5's wording, D-19; it is not an S row)
Baseline: A diff of the two non-agent runs, with the control run beside it
Evidence: to be filled by 08-06 Task 1 from the named run directories
Recommendation: to be filled by 08-06 Task 1

Mark:

## Part 2. DEC-02, the S3 bound

### DEC-02. Keep the S3 bound (pre-committed)

Pre-committed: option 1, keep the S3 bound (D-18)
The bound: p50 at most 10% added, p99 at most 25% added (the S3 row above).
Consequence, VERIFIED from the cited run: openat_tmpfs already adds 25.01% at p50 and 30.09% at p99 on WSL2 run 20261028T034700Z (tools/linux-baseline/results/20261028T034700Z/summary.md, C3 rows), with seccomp and Landlock and no BPF-LSM. If the reference host reproduces that, ADR 0004 trigger (b) is read as fired once a profile attributes the excess to the kernel mediation path rather than the broker, and the try-first step is taken (move the hot decision from the broker into a BPF map lookup, *(interpretation)*).
Rationale: option 2, relaxing the bound to fit the measurement, is rejected because R1 sets a bound from the same data it is tested against.
Reversibility: costly
The S3 row it applies to: see S3 above.
Evidence, run directories this rests on (written by plan 08-05, none exist yet): tools/linux-baseline/results/ref-harn01, tools/linux-baseline/results/ref-bpflsm-global-miss, tools/linux-baseline/results/ref-bpflsm-global-hit, tools/linux-baseline/results/ref-bpflsm-global-miss-nocount, and tools/linux-baseline/results/ref-harn01-bpfboot if the OQ2 Answer is yes.
The pre-commitment above does not fill the Mark line; the user marks it like any other.

Mark:

## Part 3. DEC-03, the feature floor (D-21)

Each block restates the item as scoping-linux.md section 3 words it. Evidence for all six: the ref-probe floor rows (tools/linux-baseline/results/ref-probe/env.txt, written by the user-run privileged probe in Task 3 of this plan; not present yet) and the VERIFIED distro-kernels table (261027-ttd-INPUT-distro-kernels.md, as summarized in the scoping-linux.md section 3 matrix).

### DEC-03a. Floor tiers

Proposed: Required *(interpretation)*: CONFIG_BPF_LSM=y; bpf in the active LSM order, by default or via the lsm= boot parameter; Landlock at ABI 6 or higher; memcg; PSI. Fallback available: CLONE_INTO_CGROUP (fork, move the child into the cgroup, then exec), CLONE_PIDFD_AUTOKILL (a broker timer plus cgroup.kill), CLONE_NNP (prctl no_new_privs in the child before exec), PIDFD_GET_INFO (SO_PEERCRED plus a read of /proc/PID/cgroup, racy against pid reuse), BPF_LSM_CGROUP (one global BPF-LSM program keyed on bpf_get_current_ancestor_cgroup_id), io_uring task-level restrictions (the sysctl kernel.io_uring_disabled=2), dmem (needed only for local-GPU VRAM limits).
Evidence: the ref-probe floor rows and the distro-kernels table named above.

Mark:

### DEC-03b. Reachability rule

Proposed: the floor is reachable on a distro when every Required row is present in its stock kernel. A kernel command-line change counts as reachable and a kernel rebuild counts as unreachable. A TO-VERIFY cell is unknown and counts neither way.
Evidence: the ref-probe floor rows and the distro-kernels table named above.

Mark:

### DEC-03c. Landlock ABI 6 minimum

Proposed: Landlock at ABI 6 or higher, the level Sandlock requires for a feature set that includes IPC scoping (scoping-linux.md note 7; that scopes first appear at or below ABI 6 is TO-VERIFY). Debian 13 and the RHEL 10 family sit exactly at 6, so this is the open question for both.
Evidence: the ref-probe floor rows and the distro-kernels table named above.

Mark:

### DEC-03d. Ubuntu counted as reachable through its HWE kernel

Proposed *(interpretation)*: Ubuntu 24.04 LTS is reachable on the HWE kernel from -39 on (bpf in the default order, Landlock ABI 8) and unreachable on the GA 6.8 kernel (Landlock ABI 4, no command-line fix). It is counted as reachable, because HWE is a stock opt-in kernel of the same release.
Evidence: the ref-probe floor rows and the distro-kernels table named above.

Mark:

### DEC-03e. The 3-of-5 count for trigger (d)

Proposed: ADR 0004 trigger (d) fires when 3 or more of the 5 target distros are VERIFIED unreachable. Current count from the matrix: 0 of 5 VERIFIED unreachable, with Landlock ABI 6 as the open question for Debian and RHEL.
Evidence: the ref-probe floor rows and the distro-kernels table named above.

Mark:

### DEC-03f. The five target distros

Proposed: Ubuntu 24.04 LTS, Debian 13, Fedora (current), the RHEL 10 family, Arch. Matrix kernels as recorded in scoping-linux.md section 3: Ubuntu 24.04 GA 6.8.0-147 and HWE 7.0.0-42; Debian 13 6.12.113; Fedora 44 at 7.2.13; RHEL 10 family 6.12.0-211.68.1.el10_2; Arch 7.2.11.
Evidence: the ref-probe floor rows and the distro-kernels table named above.

Mark:

## Part 4. Open questions that block later tasks

Type the bare value after the colon. Nothing else goes on the Answer line.

### OQ1. Yama ptrace_scope while 08-05 Task 3 runs

Question: which kernel.yama.ptrace_scope value should be set on the reference host while the S5 suite runs.
Options: 0, 1, 2 or 3.
Yama gates all six ptrace routes (PTRACE_ATTACH, PTRACE_SEIZE, process_vm_readv, process_vm_writev, open of /proc/<pid>/mem, pidfd_getfd), because each passes the ptrace attach check. Kernel behaviour below is ASSUMED (08-RESEARCH.md A5), not measured on the reference host.
- 0: any same-uid process may attach.
- 1: only an ancestor of the target, a process the target named with PR_SET_PTRACER, or a holder of CAP_SYS_PTRACE; a same-uid sibling is refused.
- 2: only CAP_SYS_PTRACE.
- 3: no attach, and the value cannot be lowered until reboot.
Reach of the 12 ptrace rows (6 routes, sibling and ancestor topology each), per value: 0, all 12 reachable; 1, the 6 sibling rows void (baseline failed) and the 6 ancestor rows reachable; 2 or 3, all 12 void, since the suite runs as an unprivileged uid without CAP_SYS_PTRACE. The 11 non-ptrace rows do not depend on OQ1. A void row is read as untested for S5, never as a pass.
Evidence: 08-RESEARCH.md Pitfall 4 and assumption A5 (Ubuntu ships 1, ASSUMED; ref-probe records yama_ptrace_scope). The wrapper never writes the sysctl, so the user sets it before 08-05 Task 3 and restores it after.
Recommendation (information only): 0, per RESEARCH Open Question 1.
Consumed by: 08-05 Task 3.

Answer:

### OQ2. A second GRUB entry without bpf for HARN-01

Question: whether to add a second GRUB entry whose lsm= list omits bpf, so HARN-01 runs without the bpf stub cost.
Options: yes or no.
- yes: a copy of the default entry whose lsm= list is the /sys/kernel/security/lsm list minus bpf. HARN-01 (ref-harn01) runs on it, and the user also runs ref-harn01-bpfboot on the default entry as the same-boot C0/C3 base for the BPF-LSM variant rows.
- no: HARN-01 runs on the default entry and its table is labelled "includes bpf stub cost".
Evidence: 08-RESEARCH.md F12. The WSL2 baseline had no bpf in its LSM list; the reference kernel is expected to have it (NOT MEASURED on the reference host).
Recommendation (information only): yes, per RESEARCH Open Question 2.
Consumed by: the GRUB step in Task 3 of this plan and the HARN-01 boot in 08-05 Task 1.

Answer:

### OQ7. A per-cgroup socket_sendmsg row

Question: whether 08-05 Task 2 builds the per-cgroup socket_sendmsg program.
Options: yes or no.
- yes: 08-05 Task 2 rebuilds with `make -B -C tools/bpflsm CG_SENDMSG=1` (-B because a plain make sees the existing object as current), so bpflsm_cg.bpf.o carries the lsm_cgroup socket_sendmsg program and --harn03 times it. This spends one more of the 10 shared lsm_cgroup hook slots.
- no: the plain build, where that program is compiled out (the 08-03 default).
Evidence: 08-RESEARCH.md F6. A per-cgroup socket_sendmsg row is not comparable to the global row for sockets created elsewhere. The global variants time socket_sendmsg either way.
Recommendation (information only): no.
Consumed by: 08-05 Task 2.

Answer:
