# Roadmap: agentic-os-kernel

## Milestones

- **B3 microkernel** (Phases 1-6): archived 2026-10-27, superseded by
  [ADR 0004](../docs/architecture/adr/0004-linux-prototype-first.md). Record:
  [milestones/b3-microkernel/README.md](milestones/b3-microkernel/README.md). Code frozen at tag
  `archive/ostd-b2-spike`, commit `1f1333e`.
- **Linux prototype** (Phases 8-14): current. ADR 0004 (status Proposed) and
  [scoping-linux.md](../docs/architecture/scoping-linux.md).
- **Recursion and cheap-inference spikes** (Phase 15): follows the Linux prototype, on no critical
  path. Phase 7 held the same spikes and was retired 2026-10-27, because its anchor (the dropped
  B3 Phase 6) no longer exists.

## Overview

The Linux prototype tests whether stock Linux can carry the core value: every action an agent
takes on files, network, tools, other agents or the model passes a kernel-enforced check against
that agent's authority, while ordinary POSIX software runs unchanged (scoping-linux.md section 1).
ADR 0004's design: one cgroup v2 cgroup per agent, BPF-LSM as the deny-by-default mediation point,
Landlock and a seccomp allowlist as defence in depth, and a userspace broker for identity, budgets,
egress, escalation and inference. A mainline fork is the escalation path, taken only when trigger
(a), (b), (c) or (d) fires. The fork is permitted, but ADR 0004 advises against it until measured.

The milestone is ordered to falsify that design as early and cheaply as it can. Phase 8 gets a
privileged reference environment, measures BPF-LSM cost, reads S5 early against a stub broker,
and closes the threshold gate. Phases 9 and 10 build the launcher and the broker. Phase 11 adds
BPF-LSM mediation and runs the adversarial suite, which reads triggers (a), (b) and (c). Phase 12
takes the stack to the target distros, which reads trigger (d), and measures POSIX
non-interference. Phase 13 scores the fork triggers. Phase 14 adds inference scheduling and
admission control.

**Evidence so far.** All of it comes from one run (`20261028T034700Z`) on one host: WSL2 on
Hyper-V, kernel 6.18.33.2, no `/dev/kvm`, no root, no `bpf` in the active LSM list
([linux-baseline.md](../docs/architecture/research/linux-baseline.md)).

- **VERIFIED:** C0 `getppid` 130 ns and `openat_tmpfs` 1763 ns at p50; Landlock ABI 7; cgroup
  delegation through `systemd-run --user`; `CLONE_INTO_CGROUP`; caller identity through
  `SO_PEERPIDFD` and `PIDFD_GET_INFO`.
- **The S3 openat bound fails.** With seccomp and Landlock, and no BPF-LSM, the proposed S3 bound
  fails for openat: `openat_tmpfs` adds 25.01% at p50 and 30.09% at p99, and `openat_rootfs` adds
  17.48% at p50. Every other S3 leg is within its bound.
- **Broker and deadline rows are recorded, not usable for thresholds.** The broker adds 19116 ns
  at p99 for identity and 20118 ns for egress; section 11 marks both "none: unstable on this host".
  Deadline-to-reaped p99 with four competitors is 17948443 ns through `pidfd_send_signal` and
  3870886 ns through `cgroup.kill`, both noise-dominated.
- **NOT MEASURED:** BPF-LSM per-hook overhead, `BPF_LSM_CGROUP`, S7 and S8.
- **ASSUMED:** that these numbers transfer to bare metal.
- **Proposed, not decided:** every threshold, the floor tiers, the Landlock ABI 6 minimum and the
  3-of-5 rule. Each is labelled "proposed, awaiting user acceptance" until DEC-01 to DEC-03 record
  the user's decision.

## Structure rationale

48 requirements in 7 phases (config: coarse). The orchestrator proposed seven hypothesis steps,
then a fork gate. The mapping below departs from that order where the merits say so. Every
departure is *(interpretation)*.

1. **The environment and the cheapest falsifiers come first (Phase 8).** The step "(i) harness,
   adversarial skeleton and privileged environment" takes two items pulled forward from (iv) and
   (v):
   - BPF-LSM per-hook cost and the `BPF_LSM_CGROUP` attach (HARN-02, HARN-03).
   - An early S5 read against a stub broker (ADV-03).

   These are the cheapest reads of triggers (b) and (c). Each needs only the privileged
   environment and a stub, not the launcher or the broker. If BPF-LSM with an empty map already
   breaks S3, or a same-uid agent kills the stub broker with the hooks loaded, the design is in
   trouble before anything is built.

   Trigger (a) needs the full stack, so it cannot come earlier than Phase 11. Trigger (d) reads
   0 of 5 distros unreachable on paper and needs 3 to fire, so it is the least likely to fire and
   waits for Phase 12.
2. **The threshold gate is folded into Phase 8, not given its own phase.**
   - DEC-01 to DEC-03 are the last plan of Phase 8, after the reference-host run. DEC-02, the
     openat choice, can then rest on reference data and on a measured BPF-LSM share instead of
     WSL2 numbers alone.
   - A phase holding only user decisions would have no observable behaviour of its own.
   - DEC-01 allows a row to be deferred to a named phase, so S7 and S8 need not be decided before
     a backend exists. ADR 0004 stays Proposed while any row is deferred.
3. **Launcher (Phase 9) and broker (Phase 10) keep the hypothesis order.** The broker keys every
   caller on the agent cgroup id that the launcher creates. Both run unprivileged on the current
   host. If ENV-01 stalls, they can run before Phase 8 finishes; the user can reorder that with
   `/gsd-phase`.
4. **BPF-LSM (iv) and the adversarial run (v) merge into Phase 11.** Mediation is verified by the
   suite. Split, the BPF-LSM phase's success criteria would be the next phase's tests. ADV-04 and
   ADV-05 also need the broker in place, because S1 has tool, egress and escalation cases and S5
   attacks the real broker.
5. **The distro matrix (vi) and S9 merge into Phase 12.**
   - The fallbacks differ by distro: `CLONE_PIDFD_AUTOKILL` and `CLONE_NNP` exist only on Fedora
     and Arch, `PIDFD_GET_INFO` is absent on Debian, and io_uring task restrictions are absent on
     Debian and RHEL. So non-interference is a per-kernel result, and S9 runs where the boots
     already are.
   - POSX-02 (non-agent behaviour with the prototype loaded against nothing loaded) goes there
     too. It comes from goal 5's wording, not from an S row.
6. **The fork gate (Phase 13) comes before inference and admission (Phase 14), reversing (vii) and
   the gate.**
   - S7 and S8 read no ADR 0004 trigger, so measuring them first would delay the fork decision
     without informing it.
   - A fork that promotes rank 4 (an in-kernel credential and budget controller) would change the
     path Phase 14 charges inference against.
   - The hard dependency of Phase 14 is Phase 10. Phase 14 can run beside Phases 11 to 13 if the
     user wants.
7. **The spikes follow the prototype as Phase 15; Phase 7 is retired, not renumbered away.**
   *(interpretation)* Removing Phase 7 with renumbering would have turned Phases 8-14 into 7-13,
   and REQUIREMENTS.md cites them by number 53 times. Phase 15 carries Phase 7's goal and source
   with its dependencies restated against the Linux phases.

**Mapping rule.** Each requirement maps to the phase that first builds or decides it. Thresholds
are read against the S row as DEC-01 and DEC-02 record it. Until then, every number in a success
criterion is "proposed, awaiting user acceptance".

**Blocker.** No privileged environment exists. ENV-01 needs root, a kernel with `bpf` in the
active LSM list, `bpftool`, clang with the BPF target and `libbpf`. No document names where that
environment comes from, and this roadmap does not either: it is a user decision. Phases 8 and 11
to 13 cannot complete without it.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): planned milestone work
- Decimal phases (8.1, 8.2): urgent insertions (marked with INSERTED)
- Numbers are never reused. 1-6 belong to the archived milestone, 7 was retired on 2026-10-27 (its
  spikes continue as Phase 15), and the Linux prototype is 8-14.

### Linux prototype (current)

- [ ] **Phase 8: Reference environment, early falsifiers and threshold gate** - privileged reference host, BPF-LSM per-hook and per-cgroup cost, adversarial skeleton, early S5 read, S1-S9 and floor acceptance
- [ ] **Phase 9: Agent launcher and containment** - cgroup per agent via `CLONE_INTO_CGROUP`, pidfd-resolvable identity, Landlock and seccomp narrowing, inheritance, freeze and kill
- [ ] **Phase 10: Broker** - kernel-supplied caller identity, tool and egress mediation with held credentials, token, money and wall-time budgets, trusted-path escalation, attribution
- [ ] **Phase 11: BPF-LSM mediation and adversarial run** - deny-by-default per-agent policy, identity propagation, io_uring closure, S1, S2, S3 and S5 on the full stack
- [ ] **Phase 12: Distro floor and POSIX non-interference** - stock-kernel boots, TO-VERIFY items, per-distro verdicts and S1, S9 and POSX-02
- [ ] **Phase 13: Fork decision gate** - triggers (a) to (d) scored from runs, try-first steps, candidate and LTS pin if a fork is indicated
- [ ] **Phase 14: Inference scheduling and admission control** - local or remote backend, one accounted queue, S7, S8, shared-daemon memory attribution

### After the Linux prototype

- [ ] **Phase 15: Recursion and cheap-inference spikes on the Linux prototype** - containment, first self-improvement loop, ignition and reward-hacking measurement, 16 GB inference baseline (retired Phase 7, re-anchored)
- ~~Phase 7: Recursion and cheap-inference spikes~~ - retired 2026-10-27, moved to Phase 15

<details>
<summary>Archived milestone B3 (microkernel), Phases 1-6: archived 2026-10-27, superseded by ADR 0004</summary>

The Rust capability microkernel on Asterinas OSTD 0.19.0. Full roadmap and requirements as of
2026-10-27: [milestones/b3-microkernel/ROADMAP.md](milestones/b3-microkernel/ROADMAP.md),
[milestones/b3-microkernel/REQUIREMENTS.md](milestones/b3-microkernel/REQUIREMENTS.md). The phase
directories stay under `.planning/phases/`.

- [x] **Phase 1: Userland, syscall ABI and IPC v2** - archived (completed 2026-10-26)
- [ ] **Phase 2: Process objects, teardown and timers** - archived: paused and superseded, never marked complete
- [ ] **Phase 3: Bootstrap, server runtime and vfs** - dropped, never started
- [ ] **Phase 4: exec and procmgr** - dropped, never started
- [ ] **Phase 5: Signals, sessions and tty** - dropped, never started
- [ ] **Phase 6: libc, BusyBox, conformance and measurement** - dropped, never started

### Phase 1: Userland, syscall ABI and IPC v2
**Status:** Complete 2026-10-26, 8/8 plans. Archived 2026-10-27.
**Record:** `.planning/phases/01-userland-syscall-abi-and-ipc-v2/`; detail in the archived ROADMAP.md.

### Phase 2: Process objects, teardown and timers
**Status:** Paused and superseded on 2026-10-27, and never marked complete.
- All 11 of 11 plans were executed and merged.
- UAT stopped at test 4 of 5: tests 1 to 3 passed, and tests 4 (`cow_clone` huge-page refusal)
  and 5 (ring-3 kill mid-run) are pending.
- Verification status is `human_needed`. The user, not Claude, reported test 1's KVM re-run.
- KERN-07 and AGNT-05 are still Pending in the archived traceability table.

Closing the UAT as superseded, or finishing tests 4 and 5 on the archived tree, is a user decision.
**Record:** `.planning/phases/02-process-objects-teardown-and-timers/`.

### Phase 3: Bootstrap, server runtime and vfs
**Status:** Dropped 2026-10-27. Never started.

### Phase 4: exec and procmgr
**Status:** Dropped 2026-10-27. Never started.

### Phase 5: Signals, sessions and tty
**Status:** Dropped 2026-10-27. Never started.

### Phase 6: libc, BusyBox, conformance and measurement
**Status:** Dropped 2026-10-27. Never started.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Userland, syscall ABI and IPC v2 | 8/8 | Complete (archived) | 2026-10-26 |
| 2. Process objects, teardown and timers | 11/11 executed | Superseded (UAT 3 of 5) | - |
| 3. Bootstrap, server runtime and vfs | 0/0 | Dropped | - |
| 4. exec and procmgr | 0/0 | Dropped | - |
| 5. Signals, sessions and tty | 0/0 | Dropped | - |
| 6. libc, BusyBox, conformance and measurement | 0/0 | Dropped | - |

</details>

## Phase Details

Every threshold below is quoted from scoping-linux.md section 4 or linux-baseline.md section 11
and is "proposed, awaiting user acceptance" until DEC-01 or DEC-02 records otherwise.

### Phase 7: Recursion and cheap-inference spikes

**Status:** Retired 2026-10-27, never planned: 0 plans, no requirements. Its anchor, "after Phase 6" (decided 2026-10-25), went when B3 was archived. Goal, source and restated dependencies moved to Phase 15. The number is not reused, and Phases 8-14 keep theirs. Its directory held only `.gitkeep` and was deleted.

### Phase 8: Reference environment, early falsifiers and threshold gate

**Goal:** As the researcher, I want to measure BPF-LSM cost and the same-uid broker attack on a named privileged reference host and record my acceptance of S1-S9, so that the cheapest ADR 0004 falsifiers are read before any prototype component is built.
**Mode:** mvp
**Depends on:** Nothing in this milestone. Blocked on the user's ENV-01 decision (the source of a privileged environment); no document names one.
**Requirements:** ENV-01, ENV-02, HARN-01, HARN-02, HARN-03, ADV-01, ADV-02, ADV-03, DEC-01, DEC-02, DEC-03
**Success Criteria** (what must be TRUE):
  1. **The environment is recorded.** On the host the user names as the reference, `bpf` appears in `/sys/kernel/security/lsm`, and a root shell loads and unloads an `lsm/file_open` program. A run-directory environment file records the kernel version, command line, Landlock ABI and the present or absent state of every scoping-linux.md section 3 floor row (ENV-01, ENV-02).
  2. **The WSL2 numbers are checked for transfer.** C0 to C3 re-run on the reference host. A side-by-side table shows, per S3, S4 and S6 leg, whether the WSL2 numbers of run `20261028T034700Z` transfer within spread (HARN-01).
  3. **BPF-LSM cost is measured on every S3 leg.**
     - Global `lsm/` programs, with empty-map and populated-map variants, including a unix-socket send hook that the prepared programs lack.
     - A per-cgroup `lsm_cgroup` config. An `lsm_cgroup` deny program makes `openat` fail with an errno in its own cgroup and succeed in a sibling cgroup. If that attach type fails on the host, the failure is recorded and the global fallback is measured instead.

     Rows are read against S3: p50 at most 10% added, p99 at most 25% added (proposed, awaiting user acceptance). The section 11 R1 arithmetic is redone with the BPF-LSM share included (HARN-02, HARN-03).
  4. **The adversarial skeleton reaches real paths.**
     - Run by a non-agent, every valid case's side effect occurs, and invalid cases are marked.
     - Every S5 attempt succeeds for a non-agent against a stub broker.
     - With the `task_kill`, `ptrace_access_check` and unix-socket hooks loaded, the count of successful attempts from an agent cgroup is recorded against S5: 0 (proposed, awaiting user acceptance) (ADV-01, ADV-02, ADV-03).
  5. **The threshold gate is closed.**
     - Each of S1 to S9, and the POSX-02 row, reads accepted, changed, rejected or deferred to a named phase, dated, in scoping-linux.md section 4.
     - The S3 openat option is chosen from the four on record, and the run directories it rests on are named.
     - The floor tiers, the reachability rule, the ABI 6 minimum, the 3-of-5 count and the five distros are accepted or changed (DEC-01, DEC-02, DEC-03).

     No later phase reads a threshold that is not recorded here.
**What would show this was the wrong thing to do:**
  - **No privileged environment can be obtained.** The phase cannot finish, and the order is wrong: Phases 9 and 10 need no privilege and should run first while the source is found. ADR 0004 decision 2 also stays untested, and that is itself a finding for the user.
  - **The early reads add nothing.** If the empty-map BPF-LSM rows are within noise of C3 on every leg, `lsm_cgroup` attaches as assumed, and ADV-03 counts 0, the early reads bought little over doing them in Phase 11 *(interpretation; the cost was one harness config and one stub)*.
  - **Design falsifiers, not phase errors:**
    - BPF-LSM with an empty map alone exceeding the accepted S3 bound is a trigger (b) candidate before anything is built. ADR 0004's try-first step (move the decision into a BPF map) has little room there, because the prepared programs are already one map lookup *(interpretation)*.
    - An ADV-03 count above 0 is a trigger (c) candidate, and its try-first step is the broker under its own uid.
**Research:** The `bpftool` `lsm_cgroup` attach and the return-1-to-allow convention (both ASSUMED, linux-baseline.md section 9). Which unix-socket send hook is attachable (the gap analysis lists `socket_sendmsg` and `unix_stream_connect`; the disabled set is in `kernel/bpf/bpf_lsm.c`). The WSL2 `kernelCommandLine` route matters only if the reference host is WSL2 (ASSUMED).
**Decisions due:** ENV-01, the source of the privileged environment. ENV-02, the reference kernel and host; trigger (b) reads "the reference kernel" and no document defines it. DEC-01, S1 to S9 and the POSX-02 row. DEC-02, the S3 openat bound. DEC-03, the floor, rule and distros.
**Plans:** 3/6 plans executed

Plans:
**Wave 1**
- [x] 08-01-PLAN.md — r1.py and transfer.py self-tests, floor.c, run.sh write_env extension (ENV-02, HARN-01, HARN-02)
- [x] 08-02-PLAN.md — aok-adv crate: pidfd/SO_PEERPIDFD stub, S5 route table, S1 class suite and report (ADV-01, ADV-02, ADV-03)
- [x] 08-03-PLAN.md — BPF-LSM sources (global, per-cgroup, adversarial) and the root wrapper, dry-run reviewable (ENV-01, HARN-02, HARN-03, ADV-03)

**Wave 2** *(blocked on Wave 1 completion)*
- [ ] 08-04-PLAN.md — 08-GATE.md template, the OQ1/OQ2/OQ7 decisions, and the dual-boot reference-host install + probe (ENV-01, ENV-02, DEC-01, DEC-02, DEC-03)

**Wave 3** *(blocked on Wave 2 completion)*
- [ ] 08-05-PLAN.md — the reference-host measurement runs: HARN-01 transfer, HARN-02/03 variants, ADV baseline and agent (HARN-01, HARN-02, HARN-03, ADV-01, ADV-02, ADV-03)

**Wave 4** *(blocked on Wave 3 completion)*
- [ ] 08-06-PLAN.md — the cited run report, the user marking 08-GATE.md, and the transcription into scoping-linux.md and ADR 0004 (DEC-01, DEC-02, DEC-03, HARN-01, HARN-02)

### Phase 9: Agent launcher and containment

**Goal:** As the researcher, I want to spawn each agent into its own cgroup with a pidfd-resolvable identity, narrowed Landlock and seccomp layers, and subtree freeze and kill, so that every later component has an unprivileged principal to key on.
**Mode:** mvp
**Depends on:** Phase 8. DEC-03 fixes which floor fallbacks the launcher must carry, and ADV-01 supplies the shell-out cases. *(interpretation)* The phase can start before ENV-01 is met, because every LNCH requirement runs unprivileged on the current host. That host has a delegated subtree through `systemd-run --user`, Landlock ABI 7 and working `CLONE_INTO_CGROUP` (linux-baseline.md sections 7 and 9).
**Requirements:** LNCH-01, LNCH-02, LNCH-03, LNCH-04, LNCH-05, LNCH-06
**Success Criteria** (what must be TRUE):
  1. A launched agent's first action finds itself in its own cgroup, and a sub-agent's cgroup is a descendant of its parent's. A peer holding a pidfd for any task in the subtree resolves the same `agent_id`. On a kernel without `CLONE_INTO_CGROUP` or `PIDFD_GET_INFO`, the fallback runs and the test reports the window or race it leaves (LNCH-01, LNCH-02).
  2. A sub-agent granted a path its parent lacks still cannot open it. It cannot remove an inherited Landlock rule or seccomp filter, and `/proc/self/status` shows `NoNewPrivs: 1` (LNCH-03).
  3. A `sh -c` grandchild of an agent reports the agent's cgroup id and is refused the same out-of-grant path as the agent (LNCH-04).
  4. After a freeze, `cgroup.events` reads `frozen 1` and no task in the subtree runs. After `cgroup.kill`, every task is reaped and the parent sees the exit through a pidfd. This retires the ASSUMED item that `cgroup.freeze` works on a delegated leaf (LNCH-05).
  5. Every denial in the phase's test set returns a documented errno rather than killing the process, and the seccomp filter inspects no pointer argument (LNCH-06).
**What would show this was the wrong thing to do:**
  - **Landlock fs may be too expensive for the accepted S3 bound.** Landlock fs alone (C2a) adds 21.04% at p50 to `openat_tmpfs` and 14.95% to `openat_rootfs` on the baseline host; seccomp alone (C1b) adds 4.03% and 3.31% (linux-baseline.md section 4). If DEC-02 keeps the 10% p50 bound, the launcher's Landlock fs layer by itself breaks S3 on openat. Then file narrowing moves to BPF-LSM, or the Landlock fs ruleset shrinks *(interpretation)*. That reopens ADR 0004 decision 3, not just this phase.
  - **The subset rule may not hold.** If a sub-agent ends with authority its parent lacks under Landlock layering and seccomp stacking, launcher-side narrowing is the wrong place for authority, and it moves to the per-agent BPF-LSM policy in Phase 11.
**Research:** Landlock layering at ABI 6 against ABI 7, because Debian 13 and RHEL 10 sit at ABI 6. `clone3` with `CLONE_INTO_CGROUP` from an unprivileged delegated subtree. Whether seccomp filters can be removed (the gap analysis 1c labels "cannot be removed" A, not verified).
**Decisions due:** The seccomp default action; LNCH-06 assumes ERRNO *(interpretation)*. Where the identity record lives before Phase 11 adds cgroup or task storage.
**Plans:** 3/6 plans executed across 6 waves, one plan per wave. The waves are serial on purpose: 09-03 needs the real registry and runs 09-02's tests under default-deny, 09-03 and 09-04 both write the shared registry, and 09-05's CPU-pinned depth reads must not run beside a cargo test.
- **Wave 1:**
  - [x] 09-01-PLAN.md — crate scaffold, sys module, spawn into cgroup, freeze/thaw/kill, the fork+move+exec fallback and the D-13 shim (LNCH-01, LNCH-05)
- **Wave 2:**
  - [x] 09-02-PLAN.md — pidfd identity resolution, whois, the registry, the SO_PEERCRED fallback, xattr-forgery negative control (LNCH-02)
- **Wave 3:**
  - [x] 09-03-PLAN.md — Landlock layer and floor, hand-assembled seccomp allow-list, errno contract, inheritance across `sh -c`, depth cap, non-widening, the registry out of an agent's reach, and the whole crate green under default-deny (LNCH-02, LNCH-03, LNCH-04, LNCH-06)
- **Wave 4:**
  - [ ] 09-04-PLAN.md — shim-driven fallback tests (spawn-fallback window, identity-fallback race), labelled simulated (LNCH-01, LNCH-02)
- **Wave 5:**
  - [ ] 09-05-PLAN.md — additive `--wrap` hook in run.sh with a byte-identity proof, D-16 indicative depth reads (LNCH-03)
- **Wave 6:**
  - [ ] 09-06-PLAN.md — whole-crate fmt/clippy, full suite in one scope, 09-ALLOWLIST.md, 09-REPORT.md and the VALIDATION sign-off (LNCH-01..LNCH-06)

### Phase 10: Broker

**Goal:** As the researcher, I want to route tool calls, egress, budgets and escalation through a broker that identifies callers from kernel-supplied identity, so that an agent holds no credential, cannot exceed its budget and cannot answer its own escalation.
**Mode:** mvp
**Depends on:** Phase 9, for the agent cgroups and identity records the broker keys on. Phase 8, for the S4 and S6 rows as DEC-01 records them. Runs unprivileged unless the broker gets its own uid, which needs a second account.
**Requirements:** BRK-01, BRK-02, BRK-03, BRK-04, BRK-05, BRK-06, BRK-07
**Success Criteria** (what must be TRUE):
  1. A caller that writes another agent's `agent_id` into a request is served and recorded as itself (BRK-01).
  2. An ungranted tool call is refused with an errno. Tool and egress round trips through the broker are measured on the reference host against S4: p99 at most 1 ms added (proposed, awaiting user acceptance) (BRK-02, BRK-03).
  3. An agent's direct connect to the remote endpoint fails with an errno and the brokered request succeeds. No credential appears in the agent's environment, open fds or readable files (BRK-03).
  4. Agents run to token or money exhaustion overshoot by at most one in-flight request's max_tokens and price. An agent past its wall-time deadline has its cgroup killed within 1 s. Both are read against S6 (proposed, awaiting user acceptance), beside a non-agent run with unbounded overshoot (BRK-04, BRK-05).
  5. The agent cannot open, connect to or write the escalation channel. A human answer releases the held action, and a refusal returns an errno. Every brokered action leaves one record naming `agent_id` and `on_behalf_of` (BRK-06, BRK-07).
**What would show this was the wrong thing to do:**
  - **The same-uid hardening may be moot.** If ADV-03 in Phase 8 already counted successes with the hooks loaded, and the user picks the trigger (c) try-first step (the broker under its own uid), then hardening a same-uid broker here is wasted. Build it under its own uid from the start.
  - **Egress may escape the broker.** If an agent reaches the remote endpoint despite the Landlock or cgroup-BPF connect deny, for example by IP, then the credential-holding broker is the only control on egress, and gap analysis 2c's design does not hold.
**Research:** The Landlock unix-socket restriction against a cgroup-BPF connect deny for "UDS to the broker only" egress (gap analysis 2c). The escalation channel medium (gap analysis 2g names a pty or socket the agent has no fd for; secure-attention-key behaviour is unverified there). The S6 wall-time leg on the reference host under same-CPU load.
**Decisions due:**
  - The broker under the agents' uid or under its own uid. *(interpretation)* Under its own uid, no agent holds the broker's uid, so S5 as written has no attempts to count. The user then decides whether S5 is met by construction.
  - Whether a sub-agent's budget is carved from its parent's *(interpretation; goal 1 states the subset rule for authority, not budgets)*.
**Plans:** TBD

### Phase 11: BPF-LSM mediation and adversarial run

**Goal:** As the researcher, I want to enforce a deny-by-default BPF-LSM policy per agent and run the adversarial suite against the full stack, so that ADR 0004 triggers (a), (b) and (c) are read from measurements.
**Mode:** mvp
**Depends on:**
  - Phase 8: the ENV-01 environment, the HARN-02 and HARN-03 attach results, and the ADV-01 and ADV-02 suite.
  - Phase 9: the agent cgroups.
  - Phase 10: the broker that ADV-05 attacks, and the tool, egress and escalation cases of S1.
**Requirements:** MED-01, MED-02, MED-03, MED-04, MED-05, MED-06, ADV-04, ADV-05
**Success Criteria** (what must be TRUE):
  1. For each hook in the per-agent policy, an out-of-grant action from an agent fails with an errno and the same action from a non-agent succeeds. A grandchild forked inside the subtree carries the identity and is denied. Every mediated event leaves a record naming `agent_id` and `on_behalf_of` (MED-01, MED-02, MED-03).
  2. The io_uring cases fail from an agent on the reference kernel, through task-level restrictions or `kernel.io_uring_disabled=2` (MED-04).
  3. S3 is measured with the production policy on openat, connect, execve and unix-socket sendmsg, against the S3 row DEC-02 records. S2 is measured on the committed task suite: median across tasks at most 3%, worst task at most 8.4% (proposed, awaiting user acceptance). Any excess carries a profile that attributes it to the kernel mediation path or to the broker (MED-05, MED-06).
  4. The full-stack suite reports S1 against 0 (proposed, awaiting user acceptance). Each success is classified as closable or not by an attachable hook, a Landlock right or a cgroup-BPF program, with a seccomp-only closure recorded as not closing it (ADV-04).
  5. S5 against the real broker, with the `task_kill`, `ptrace_access_check` and unix-socket hooks in place, reports its successes against 0 (proposed, awaiting user acceptance), and the non-agent baseline still succeeds (ADV-05).
**What would show this was the wrong thing to do:**
  - **Trigger (a) fires.** An S1 success that no attachable hook, Landlock right or cgroup-BPF program can close fires trigger (a). Stock Linux then cannot carry the core value as designed, and Phase 13's try-first steps decide what happens next.
  - **Trigger (c) fires.** An S5 success with the three hooks in place fires trigger (c).
  - **BPF-LSM may close nothing.** If Landlock rights or cgroup-BPF programs close every S1 case without BPF-LSM, then BPF-LSM adds TCB (the BPF programs and the privileged loader) for no closure, and ADR 0004 decision 2 should be revisited *(interpretation)*.
**Research:**
  - Check the attach-disabled hook set (`kernel/bpf/bpf_lsm.c` lines 39-57 per the gap analysis) against each ADV-01 case.
  - Find the sleepable-hook subset, if any decision must sleep.
  - Define the S2 task suite.
**Decisions due:** The S2 task suite, which no document defines. The per-cgroup attach or the global-program fallback, per the HARN-03 result.
**Plans:** TBD

### Phase 12: Distro floor and POSIX non-interference

**Goal:** As the researcher, I want to run the prototype on each target distro's stock kernel and diff os-test results between agent and non-agent runs, so that I know whether the floor holds and whether agent features leave POSIX behaviour unchanged.
**Mode:** mvp
**Depends on:** Phase 11, for the full stack to install. Phase 8, for DEC-03's accepted floor, rule and distro list. ENV-03 needs distro boots, which the current host cannot give (no `/dev/kvm`); the route is part of the user's ENV-01 decision.
**Requirements:** ENV-03, DIST-01, DIST-02, DIST-03, POSX-01, POSX-02
**Success Criteria** (what must be TRUE):
  1. Each confirmed distro's stock kernel boots in the test environment and prints its `uname -r`, active LSM list and Landlock ABI (ENV-03).
  2. The TO-VERIFY items that decide the floor carry VERIFIED readings: the Landlock ABI at which scopes first appear (note 7), Debian 13 `CONFIG_CGROUP_BPF` (note 4), the bootloader `lsm=` default (note 2) and per-cgroup PSI files (note 5) (DIST-01).
  3. A per-distro verdict table under the accepted rule reports the trigger (d) count (fires at 3 of 5, proposed, awaiting user acceptance). On paper it is 0 of 5 VERIFIED unreachable, with Ubuntu 24.04 reachable on HWE only and Debian 13 and RHEL 10 exactly at ABI 6 (DIST-02).
  4. On each reachable distro the stack runs with that distro's fallbacks, and the suite gives an S1 count against 0 (proposed, awaiting user acceptance) that names which fallbacks ran (DIST-03).
  5. os-test results differ between non-agent and full-grant agent runs in a number of tests read against S9: 0 (proposed, awaiting user acceptance), beside a non-agent control run. A non-agent run with the prototype loaded matches one with nothing loaded; that is POSX-02, which is not an S row (POSX-01, POSX-02).
**What would show this was the wrong thing to do:**
  - **Trigger (d) fires.** Three or more distros VERIFIED unreachable fire trigger (d), and the try-first step is a narrower distro list.
  - **Agent features change POSIX behaviour.** A POSX-01 or POSX-02 difference that the control run does not explain goes against goal 5 and the compatibility constraint. The mediation policy is then what changes, not the distro list.
  - **The boots may add little.** If the count is 0 and every TO-VERIFY item resolves as the paper matrix predicts, the boots added little over the matrix for trigger (d). The per-distro S1 count under differing fallbacks is then what earns the phase *(interpretation)*.
**Research:** Cross-building os-test and running it on stock Linux. Each distro's fallback set: `CLONE_PIDFD_AUTOKILL` and `CLONE_NNP` are absent on Ubuntu, Debian and RHEL; `PIDFD_GET_INFO` is absent on Debian; io_uring task restrictions are absent on Debian and RHEL, where RHEL defaults `io_uring_disabled` to 2.
**Decisions due:** The os-test commit. Whether S9 runs on every reachable distro or only the reference kernel; POSX-01 proposes every distro *(interpretation)*.
**Plans:** TBD

### Phase 13: Fork decision gate

**Goal:** As the researcher, I want to score ADR 0004 triggers (a) to (d) from the recorded runs and try each named alternative before any fork, so that a mainline fork happens only on measured evidence.
**Mode:** standard (a decision gate with no vertical slice to build)
**Depends on:** Phase 11, for triggers (a), (b) and (c). Phase 12, for trigger (d).
**Requirements:** FORK-01, FORK-02
**Success Criteria** (what must be TRUE):
  1. A scoring record gives each trigger as fired, not fired, or not readable because its row was not accepted. Each verdict links the run directory it rests on (FORK-01).
  2. For each fired trigger, the try-first step ADR 0004 names was run and its result recorded before any fork is named (FORK-02):
     - (a): an adjacent attachable hook, or removing the path from the grant.
     - (b): a BPF map lookup for the hot decision.
     - (c): the broker under its own uid.
     - (d): a narrower distro list.
  3. If a fork is still indicated, a new ADR names the promoted candidate and the LTS pin, 6.18 or 6.12 (both end of life December 2028 per scoping-linux.md section 5). The promoted candidate is rank 1 for (d) and rank 4 for (a) and (c); (b) promotes none, and the profile picks the change. If no trigger fired, the record says the prototype continues on stock Linux (FORK-02).
**What would show this was the wrong thing to do:**
  - **A trigger is not readable.** Its S row was never accepted, so the gate has nothing to score. That is a DEC-01 failure in Phase 8, not something this phase can repair.
  - **A fork is named without its try-first step.** Naming a fork on a trigger whose try-first step was not run is the failure ADR 0004 warns against; this gate exists to prevent it.
**Research:** None beyond the recorded runs. For a fork, read the carrying-cost figures in scoping-linux.md section 5 (Asahi's 858 patches at 6.18.8).
**Decisions due:** The LTS pin (scoping-linux.md section 6 question 5), only if a fork is indicated.
**Plans:** TBD

### Phase 14: Inference scheduling and admission control

**Goal:** As the researcher, I want to serve every agent's inference through one accounted queue and admit agents only within context capacity, so that inference and context memory are scheduled and charged per agent.
**Mode:** mvp
**Depends on:** Phase 10, for the broker budgets that inference charges against. Phase 9, for the LNCH-05 freeze that ADM-02 uses. DEC-04 and DEC-05 are decided inside this phase. *(interpretation)* It does not depend on Phase 13, because S7 and S8 read no fork trigger. It is placed after Phase 13 so that a rank-4 fork cannot change INF-02's charging path after it is built. It can run beside Phases 11 to 13 once Phase 10 is done.
**Requirements:** DEC-04, DEC-05, INF-01, INF-02, INF-03, ADM-01, ADM-02, ADM-03
**Success Criteria** (what must be TRUE):
  1. Local GPU or remote API is recorded, and a snapshot-cost criterion is either added in the section 4 format (labelled proposed) or left to the runtime (DEC-04, DEC-05).
  2. An agent's direct request to the backend fails with an errno. Per-agent sums for a run equal the totals the backend or the API reports (INF-01, INF-02).
  3. S7 at N = 1, 4 and 16 is read against mean at most 1.05x and p99 at most 1.10x the direct baseline (proposed, awaiting user acceptance). The AIOS FIFO numbers (aios p. 9) are reported alongside, not as the threshold (INF-03).
  4. An over-capacity admission sequence admits a number of agents read against S8 (i): 0. The time from a `memory.pressure` trigger to the lowest-priority agent frozen is read against S8 (ii): at most 1 s. Both are proposed, awaiting user acceptance, and a non-agent run without admission control reports peak pressure and OOM kills (ADM-01, ADM-02).
  5. A request flood from one agent shows up as that agent's memory, in its cgroup or in a ledger, and not as another agent's or as unattributed daemon usage (ADM-03).
**What would show this was the wrong thing to do:**
  - **A remote provider may dominate S7.** If DEC-04 picks a remote API and the provider's own queueing dominates turnaround, S7 measures the provider, not this queue *(interpretation)*. The S7 row then needs a local backend or a changed baseline.
  - **Shared-daemon memory may need a kernel change.** If that memory cannot be attributed to the agent without one, ADR 0003's problem recurs on Linux: memcg charges the allocating task (gap analysis 4c). The PATCH route of 4c is then the only one left, and it is not among the ranked fork candidates *(interpretation)*.
**Research:**
  - S8 needs a memory-pressure generator and an observer, which the baseline harness lacks (linux-baseline.md section 8).
  - Choose the backend per DEC-04.
  - Phase 15's S4 numbers follow this phase, so DEC-04 cannot use them unless S4 is run first (see Phase 15).
**Decisions due:** DEC-04. DEC-05. The context-capacity unit, tokens or bytes (gap analysis 4a).
**Plans:** TBD

### Phase 15: Recursion and cheap-inference spikes on the Linux prototype

**Goal:** As the researcher, I want the recursion and cheap-GPU inference spikes run on the Linux prototype, so that I know whether a contained self-improvement loop works at this scale before building on it.
**Note on the goal:** Carried from retired Phase 7, whose goal read "after the B3 MVP". The B3 MVP will not happen, so "on the Linux prototype" replaces it.
**Mode:** standard *(interpretation: each spike ends in a recorded measurement, not a vertical slice)*
**Depends on:** Phase 14, the last phase of the Linux prototype. Restated per spike, from the source document:
  - S0 (containment) needs no roadmap phase: the source specifies a throwaway git worktree inside a container. *(interpretation)* The Phase 9 launcher could serve as that sandbox, with the grader and held-out tasks outside the agent's Landlock grant, and Phase 11 would add BPF-LSM mediation. Either route makes S0 depend on those phases.
  - S1 to S3 need S0 to pass first (S2 and S3 also need S1).
  - S4 and S7 need a 16 GB card: the source's hardware envelope names an RTX 5070 Ti or an RX 9070 XT. No document in this repo records either card on this host. S7 also follows S4.
  - S5 and S6 depend on S1, which the source's ordering sentence does not say. S5 replays agent turns recorded in S1, and S6 gates on the seed spread of the S1 task set. "S4 to S6 are independent of S0" therefore holds only for S4.
  - **S4 now follows DEC-04.** Phase 7 expected S4's numbers to inform DEC-04 (local GPU or remote API). DEC-04 is decided in Phase 14, so S4 cannot inform it from here unless S4 runs before Phase 14. S4 needs only the card, so it can. Moving it is a user decision.
  - K1 (compute-unit accounting) landed in the broker and Phase 14: BRK-04, INF-02 and the context-unit decision. K4 (KV cache as a managed resource) maps onto ADM-01 and ADM-03. *(interpretation)* Both are built or decided by Phase 14, so what remains is checking them against the S4 and S7 numbers, or dropping them. That is a user decision.
  - K2 is written against the B2 endpoint, and K3 against OSTD ktests in QEMU. Both are archived substrates, so each needs restating against the Linux prototype or dropping. That is a user decision.
  - The source is internally inconsistent: its section 4 says K1 to K3 consume S4 numbers, and its section 5 says K1 to K4 run after S4 and S7.
**Requirements:** TBD
**Source:** [recursion-and-cheap-inference-spikes.md](../docs/architecture/research/recursion-and-cheap-inference-spikes.md). Thresholds there are *proposed*; each spike measures its baseline first and fixes the threshold before the experiment. Success criteria are TBD until `/gsd-plan-phase 15`.
**Order:** S0 gates S1, and S1 gates S2, S3, S5 and S6. S4 gates S7. The S0 chain and the S4 chain are independent of each other.
**What would show this was the wrong thing to do** (from the source):
  - **S0 hits its kill condition.** If the grader needs files the agent must also read, the split is redesigned before any loop runs, and S1 to S3 do not start.
  - **S2 shows no ignition.** The record says the improver is not improving, and nothing is built on that assumption.
  - **A loop runs before S0 passes.** The source's one stated objection (section 6): "fully autonomous" before S0 would give the loop write access to its own measurement.
**Decisions due:** Whether S4 runs before Phase 14. Which 16 GB card, if any, is available. K2 and K3: restate against Linux or drop. K1 and K4: check or drop.
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd-plan-phase 15 to break down)

## Traceability

| Phase | Requirements | Count |
|-------|--------------|-------|
| 8 | ENV-01, ENV-02, HARN-01, HARN-02, HARN-03, ADV-01, ADV-02, ADV-03, DEC-01, DEC-02, DEC-03 | 11 |
| 9 | LNCH-01, LNCH-02, LNCH-03, LNCH-04, LNCH-05, LNCH-06 | 6 |
| 10 | BRK-01, BRK-02, BRK-03, BRK-04, BRK-05, BRK-06, BRK-07 | 7 |
| 11 | MED-01, MED-02, MED-03, MED-04, MED-05, MED-06, ADV-04, ADV-05 | 8 |
| 12 | ENV-03, DIST-01, DIST-02, DIST-03, POSX-01, POSX-02 | 6 |
| 13 | FORK-01, FORK-02 | 2 |
| 14 | DEC-04, DEC-05, INF-01, INF-02, INF-03, ADM-01, ADM-02, ADM-03 | 8 |
| 15 | TBD (spike track) | 0 |

**Coverage:** 48 of 48 v1 requirements mapped, each to exactly one phase. Unmapped: 0.

| Criterion or trigger | Read in | Requirement |
|----------------------|---------|-------------|
| S1 mediation completeness | Phase 11; per distro in Phase 12 | ADV-04, DIST-03 |
| S2 workload overhead | Phase 11 | MED-06 |
| S3 per-call overhead | Phase 8 (BPF-LSM share), Phase 11 (production policy) | HARN-02, HARN-03, MED-05 |
| S4 broker round trip | Phase 10 | BRK-02, BRK-03 |
| S5 broker integrity | Phase 8 (stub), Phase 11 (real broker) | ADV-03, ADV-05 |
| S6 budget enforcement | Phase 10 | BRK-04, BRK-05 |
| S7 inference scheduling | Phase 14 | INF-03 |
| S8 admission control | Phase 14 | ADM-01, ADM-02 |
| S9 POSIX non-interference | Phase 12 | POSX-01 |
| Trigger (a) | Phase 11, scored in Phase 13 | ADV-04, FORK-01 |
| Trigger (b) | Phases 8 and 11, scored in Phase 13 | HARN-02, MED-05, MED-06, FORK-01 |
| Trigger (c) | Phases 8 and 11, scored in Phase 13 | ADV-03, ADV-05, FORK-01 |
| Trigger (d) | Phase 12, scored in Phase 13 | DIST-02, FORK-01 |
| Threshold acceptance gate | Phase 8 | DEC-01, DEC-02, DEC-03 |

## Progress

**Execution Order:**
Phases execute in numeric order: 8 → 9 → 10 → 11 → 12 → 13 → 14 → 15. Phases 9 and 10 can run
before Phase 8 completes if ENV-01 stalls, and Phase 14 can run once Phase 10 is done (see
Structure rationale). Phase 15 follows Phase 14, except that its S4 can run earlier (see Phase 15).
Phase 7 is retired. Phases 1-6 are archived (see the archived milestone block above).

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 7. Recursion and cheap-inference spikes | 0/0 | Retired (moved to Phase 15) | - |
| 8. Reference environment, early falsifiers and threshold gate | 3/6 | In Progress | - |
| 9. Agent launcher and containment | 3/6 | In Progress | - |
| 10. Broker | 0/0 | Not started | - |
| 11. BPF-LSM mediation and adversarial run | 0/0 | Not started | - |
| 12. Distro floor and POSIX non-interference | 0/0 | Not started | - |
| 13. Fork decision gate | 0/0 | Not started | - |
| 14. Inference scheduling and admission control | 0/0 | Not started | - |
| 15. Recursion and cheap-inference spikes on the Linux prototype | 0/0 | Not started | - |
