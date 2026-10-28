# Phase 8: Reference environment, early falsifiers and threshold gate - Context

**Gathered:** 2026-10-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Stand up a named privileged reference host, read the three cheapest ADR 0004 falsifiers on it, and
close the threshold gate. The falsifiers are BPF-LSM per-hook cost (HARN-02), `BPF_LSM_CGROUP`
attach and cost (HARN-03), and the S5 same-uid attack against a stub broker (ADV-01 to ADV-03).
The gate is the user's recorded decision on S1 to S9, the POSX-02 row, the S3 openat bound and the
floor (DEC-01 to DEC-03). Requirements: ENV-01, ENV-02, HARN-01 to HARN-03, ADV-01 to ADV-03,
DEC-01 to DEC-03. The boundary is fixed by `.planning/ROADMAP.md`; discussion settled how, not
whether.

</domain>

<decisions>
## Implementation Decisions

### Reference environment (ENV-01, ENV-02)

- **D-01:** The reference host is a **dual-boot bare-metal Linux install**, not WSL2. The user first
  picked the WSL2 `.wslconfig` route, then named dual-boot as the fallback, then answered the
  time-box with "just switch to dual boot". The WSL2 route is dropped and not attempted. —
  **Reversibility:** costly — every S2/S3 reading, trigger (b) and the HARN-01 transfer table are
  read on this host; changing it invalidates those runs.
- **D-02:** The reference distro is **Ubuntu 24.04 HWE, kernel 7.0, build `-42` or later**. Build
  `-38` lacks `bpf` in `CONFIG_LSM`; `-39` and `-42` fixed it (distro-kernels input, rows "bpf in
  default lsm=" and "BPF_LSM_CGROUP usable"). The Ubuntu userns AppArmor restriction is present on
  this distro and applies to the harness and the suite. — **Reversibility:** costly — same reasons
  as D-01.
- **D-03:** The install is up and probed **by 2026-11-04** (the user said "within 1 week" on
  2026-10-28). Phases 9 and 10 need no privilege and run first meanwhile. If the date passes
  unmet, the roadmap's "wrong thing to do" entry applies: run Phases 9 and 10 first and report the
  untested ADR 0004 decision 2 to the user.
- **D-04:** "Ready" means the probe shows all of: `bpf` in `/sys/kernel/security/lsm`
  (`active_lsms`), BTF present, Landlock ABI recorded, each scoping-linux.md section 3 floor row
  recorded present or absent, and a trivial **global** `lsm/file_open` program loads and attaches
  as root. `lsm_cgroup` is **not** in the ready gate: its failure is the HARN-03 finding and must not
  read as a bad host.
- **D-05:** ENV-02's environment file is `env.txt` in the run directory, extended. `run.sh` already
  writes kernel, command line, `config_lsm`, `config_bpf_lsm`, `active_lsms`, `landlock_abi`,
  `pidfd_get_info` and `so_peerpidfd` through its `kv` helper. It lacks per-row floor presence, a
  BTF check, the `bpftool` version and the attach result. The user's decision record points to this
  file.
- **D-06:** Considered and rejected for the reference host: cloud VM with a spend cap, custom WSL2
  kernel, local VM (no `/dev/kvm` on the WSL2 host), Debian 13, Fedora 44, Arch.

### BPF-LSM measurement (HARN-01 to HARN-03)

- **D-07:** BPF-LSM state lives **outside the harness, labelled with `--tag`**: a root wrapper
  loads the BPF objects with clang and `bpftool`, creates and delegates the cgroups, then calls
  `run.sh --suites micro --tag bpflsm-<variant>`. `micro.c` and `sandbox.c` are unchanged and the
  harness stays "gcc and glibc only" (Makefile header). `--tag` is valid with `--suites micro`
  only, and all HARN-02/03 legs are micro legs.
- **D-08:** The timed `micro` process runs **unprivileged**; root is used only for setup (loading,
  attaching, cgroup creation and delegation). Same credentials as run `20261028T034700Z`, so C0 and
  C3 rows stay comparable and capability checks do not enter the timed path.
- **D-09:** **Attach proof is a counter map plus a denial probe.** Each program increments a
  per-hook counter in a pinned map; the wrapper reads it before and after each leg, and the leg is
  invalid unless the delta is at least the leg's call count. Before timing, a sentinel the
  populated-map variant denies must fail with `EACCES` and succeed under the empty map. The counter
  increment lands inside the timed rows and is charged to the BPF-LSM share: a **conservative
  (upward) bias**, to be reported as such.
- **D-10:** The unix-socket send hook added for HARN-02 is **`lsm/socket_sendmsg`**. Its
  attachability is ASSUMED until the first load on the reference host; the disabled-hook set is in
  `kernel/bpf/bpf_lsm.c`. `lsm/unix_may_send` is not measured here.
- **D-11:** Variants: global programs (`file_open`, `socket_connect`, `bprm_check_security`,
  `socket_sendmsg`) each with an empty-map (miss) and a populated-map (hit) variant, plus a
  per-cgroup `lsm_cgroup` config. Legs: `openat_tmpfs`, `openat_rootfs`, `unix_connect`,
  `tcp_connect`, `fork_exec_wait`, `posix_spawn`, `unix_dgram_pingpong` (all already in
  `micro.c`). `lsm_cgroup` is checked per HARN-03: attached to one cgroup it makes `openat` fail
  with an errno there and succeed in a sibling. The return-1-to-allow convention is checked there
  too (both ASSUMED, linux-baseline.md section 9). If the attach type fails, record the failure
  and measure the fallback, one global program keyed on `bpf_get_current_ancestor_cgroup_id`.
- **D-12:** HARN-01 re-runs all four suites (micro, broker, deadline, cgroup; C0 to C3) on the
  reference host. The side-by-side table against run `20261028T034700Z` states per S3, S4 and S6
  leg whether the numbers transfer within spread. *(interpretation)* If the dual-boot is on the
  same machine as the baseline (Ryzen 9 7900X3D, per that run's `env.txt`), the table isolates the
  hypervisor from the silicon. That is ASSUMED, not confirmed; if the machine differs, the table
  confounds the two.

### Adversarial suite and stub broker (ADV-01 to ADV-03)

- **D-13:** Cases and the stub broker are written in **Rust**, with the `libc` crate. All `unsafe`
  sits in **one small `sys` module** with `// SAFETY:` comments. The crate is test tooling, not the
  TCB, so it is outside `tools/unsafe_check.py` and the unsafe ledger. The default toolchain on
  the WSL2 host is nightly 1.100.0, and stable 1.99.1 is installed, so the crate pins stable
  explicitly. The user chose Rust over the recommended C-cases-plus-Python-runner. — **Reversibility:**
  costly — ADV-05 reuses the cases against the Phase 10 broker.
- **D-14:** The suite targets the broker **by pid and socket path only**, so it runs unchanged
  against the real Phase 10 broker (ADV-05). This choice does not decide Phase 10's broker
  language.
- **D-15:** The stub identifies a peer by its **kernel-stamped cgroup id** (`SO_PEERPIDFD` plus
  `PIDFD_GET_INFO`), the mechanism `broker_id` in `broker.c` already uses. The impersonation
  attempts are: (i) move the attacker's own pid into another agent's cgroup via `cgroup.procs`;
  (ii) send on a connected socket fd obtained from another agent (`SCM_RIGHTS` or inherited);
  (iii) claim another id in the payload, a **control** that must be ignored.
- **D-16:** Results are per **enumerated route**, one row each. Indicative route lists (the planner
  finalises them): kill: `kill`, `tgkill`, `rt_sigqueueinfo`, `pidfd_send_signal`, write
  `cgroup.kill`; ptrace: `PTRACE_ATTACH`, `PTRACE_SEIZE`, `process_vm_readv/writev`, open
  `/proc/<pid>/mem`, `pidfd_getfd`; bind: pathname `unlink` plus `bind`, abstract name;
  impersonation: D-15 (i) to (iii). A route is **invalid** if the non-agent baseline run (same uid)
  does not succeed on it. S5 counts valid routes that succeed from an agent cgroup. For each
  success the recorded path is the route name plus the BPF deny counter for the relevant hook.
- **D-17:** The S1 skeleton at Phase 8's close is **one valid case per class**, about 8: files,
  network, tools, other agents, the model, io_uring, directly used API key, shell-out escape. Each
  case carries its class, a side-effect oracle and a `closable_by` field left empty for ADV-04.
  The report states that 0 of about 8 is not evidence of completeness.

### Threshold gate (DEC-01 to DEC-03)

- **D-18:** **DEC-02 is pre-committed to option 1, keep the S3 bound** (p50 at most 10% added, p99
  at most 25% added, proposed). The user chose this over the R1 revisions and over deciding at the
  gate. Consequence, stated to the user: `openat_tmpfs` already adds 25.01% at p50 and 30.09% at
  p99 on WSL2 with seccomp and Landlock and no BPF-LSM. If the reference host reproduces that, trigger
  (b) is read as fired once a profile attributes the excess to the kernel mediation path, and the
  try-first step (move the decision into a BPF map lookup) is taken. Rationale recorded: R1 sets a
  bound from the same data it is tested against, so it cannot fail on the legs it revises. —
  **Reversibility:** costly — changing the rule after the data arrives is the fitting it was meant to
  prevent.
- **D-19:** **S7 and S8 are deferred to Phase 14** (roadmap default). S1 to S6, S9 and the POSX-02
  row are decided at the gate. S1, S9 and POSX-02 thresholds (0 each) are therefore accepted before
  their first measurement in Phase 12. ADR 0004 stays Proposed while S7 and S8 are deferred
  (DEC-01).
- **D-20:** The gate is run through a **gate sheet**. The last Phase 8 plan produces `08-GATE.md`
  in the phase directory: one block per row (S1 to S9, POSX-02) with measure, proposed threshold,
  baseline, the evidence in hand and a recommendation. The user marks each block; the plan then
  transcribes the marks into the scoping-linux.md section 4 Status cells (dated; accepted, changed
  with value, rejected, or deferred with phase) and updates the ADR 0004 status line.
- **D-21:** **DEC-03 goes on the same gate sheet** as its own block, with the reference-host probe
  (floor rows on Ubuntu HWE 7.0) and the VERIFIED distro-kernels table as evidence. The floor tiers,
  the reachability rule, the Landlock ABI 6 minimum, Ubuntu counted via HWE, the 3-of-5 count and the
  five distros are accepted or changed there; the section 3 "proposed" labels update from the marks.

### Claude's Discretion

- The S5 deny rule that the `task_kill`, `ptrace_access_check` and unix-socket BPF programs
  implement: what "agent cgroup" means for ADV-03, for example deny when the caller is in an agent
  cgroup and the target or peer is outside its subtree. Record the rule and its hooks in the ADV-03
  report.
- Stub restart between kill attempts, so one successful kill does not mask later routes.
- Whether an unattached sibling cgroup and the root cgroup are separate S2-relevant rows in the
  HARN-03 per-cgroup config.
- How the counter-increment bias (D-09) is reported, for example a counter-free spot check on one
  leg.
- The numeric test for "the reference host reproduces the openat miss" under D-18.
  *(interpretation)* Reuse the R1 eligibility rule of linux-baseline.md section 11 (stable row,
  spreads under 10%) so the reproduction claim is not read from a noisy row.
- Names and locations of the wrapper script, the Rust crate (a directory under `tools/`), the
  per-case JSON report and `08-GATE.md`.
- Whether the Rust suite and the stub, and the non-agent baseline run of ADV-01 and ADV-02, can be
  built before the dual-boot is up. *(interpretation)* Building needs no privilege; the S5 agent-cgroup
  run and the cgroup moves need delegated cgroup access that the WSL2 host may not give. Not
  checked.
- `summarize.py` has no R1 code (a grep for R1, revise and eligibility found nothing). The planner
  decides whether the HARN-02 "R1 arithmetic redone with the BPF-LSM share" is scripted or done by
  hand from the summary CSV.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and requirements
- `.planning/ROADMAP.md` - Phase 8 section (lines 202-234): goal, success criteria, "what would show
  this was wrong", research and decisions due; structure rationale at lines 53-115
- `.planning/REQUIREMENTS.md` - ENV-01/02 (lines 53-54), HARN-01..03 (59-61), ADV-01..05 (65-74),
  DEC-01..05 (78-94), POSX-02 (160); open-decision list near line 222
- `.planning/phases/08-reference-environment-early-falsifiers-and-threshold-gate/08-DISCUSSION-LOG.md` -
  audit trail of this discussion (not an input to planning)

### Architecture decisions and criteria
- `docs/architecture/adr/0004-linux-prototype-first.md` - decisions 1 to 5 and fork triggers (a) to
  (d); status line to update at the gate
- `docs/architecture/scoping-linux.md` - section 3 floor matrix and tiers, section 4 S1 to S9 table
  (Status cells edited at the gate), section 5 fork candidates, section 6 open questions
- `docs/architecture/research/linux-baseline.md` - section 8 (exact commands and the `bpflsm.bpf.c`
  and `bpflsm_cg.bpf.c` sketches), section 9 (ASSUMED items: `lsm_cgroup` attach, return-1-to-allow,
  transfer), section 10, section 11 (R1/R2 tables)

### Inputs for the floor and distro facts
- `.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md` -
  per-distro kernel versions and feature rows; Ubuntu HWE `-42` vs `-38` `lsm=` fix
- `.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md` -
  hook coverage, the attach-disabled hook set, goal 1 and 2 tables
- `.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md` -
  fork candidates and prior art

### Existing harness and baseline evidence
- `tools/linux-baseline/run.sh` - flags, `--tag` handling (lines ~60-63, ~169-171, ~199-200),
  `env.txt` writer (lines ~97-145)
- `tools/linux-baseline/micro.c`, `tools/linux-baseline/sandbox.c`, `tools/linux-baseline/sandbox.h` -
  the ten legs and the C0 to C3 configs
- `tools/linux-baseline/broker.c` - `broker_id` identity variant (SO_PEERCRED, SO_PEERPIDFD,
  PIDFD_GET_INFO)
- `tools/linux-baseline/cg.c`, `tools/linux-baseline/cg.h` - delegated-cgroup helpers and
  `cg_clone3_into`
- `tools/linux-baseline/summarize.py`, `tools/linux-baseline/check_report.py`
- `tools/linux-baseline/results/20261028T034700Z/` - the WSL2 baseline run (`summary.md`,
  `env.txt`, `micro.csv`, `broker.csv`, `deadline.csv`, `cgroup.csv`)
- `tools/unsafe_check.py` - scope of the unsafe gate (kernel and userland only)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `tools/linux-baseline/run.sh`: runs suites micro, broker, deadline, cgroup, with configs
  interleaved per round, each in a fresh process pinned with `taskset`. `--run-id`, `--configs`,
  `--rounds`, `--cpu`, `--smoke`, `--tag`. Output in `tools/linux-baseline/results/<RUN_ID>/`;
  `smoke-*` ids are recreated and gitignored.
- `micro.c`: all ten legs HARN-02 names already exist (`timer_pair`, `getppid`, `getppid_x1000`,
  `openat_tmpfs`, `openat_rootfs`, `unix_connect`, `tcp_connect`, `fork_exec_wait`, `posix_spawn`,
  `unix_dgram_pingpong`). No leg needs adding.
- `broker.c` `broker_id`: the kernel-stamped identity path the Rust stub (D-15) mirrors.
- `cg.c`: `cg_setup`, `cg_make_leaf`, `cg_write`, `cg_clone3_into`: a C reference for delegated
  cgroup handling that the Rust suite reimplements.

### Established Patterns
- Harness is "gcc and glibc only, no other library" (Makefile header). BPF objects need clang and
  `bpftool` outside it (D-07).
- Harness runs unprivileged. A selfcheck proves each restriction is active before timing; BPF-LSM
  has no in-harness selfcheck, hence D-09.
- Utilities are stdlib Python run with `python3 -I`.
- Evidence labels: VERIFIED, ASSUMED, NOT MEASURED, *(interpretation)*; a bound comes with a
  baseline, and the run directory it rests on is named.

### Integration Points
- `scoping-linux.md` section 4 Status cells and the ADR 0004 status line: edited by the last plan.
- `run.sh` `env.txt` writer: extended per D-05.
- `linux-baseline.md` section 8 sketches: starting point for the BPF programs.
- Probe of the WSL2 host on 2026-10-27 (VERIFIED, shell output): kernel `6.18.33.2-microsoft-standard-WSL2`;
  `CONFIG_BPF_LSM=y`, `CONFIG_CGROUP_BPF=y`, `CONFIG_SECURITY_LANDLOCK=y`; `CONFIG_LSM` has no
  `bpf`; no `lsm=` on the command line; no `/sys/kernel/security/lsm`; no `/dev/kvm`; no
  `bpftool`; clang present; no passwordless sudo. This is why WSL2 stopped being the reference.

</code_context>

<specifics>
## Specific Ideas

- What would show Phase 8 worked: the environment file meets D-04; the three falsifier reads exist
  with N, p50, p99, spreads and stability marks; every valid case and S5 route succeeds for the
  non-agent run; `08-GATE.md` is marked and the section 4 Status cells are dated.
- What would show it was the wrong thing to do (roadmap): no privileged environment by 2026-11-04;
  or the empty-map BPF-LSM rows sit within noise of C3 on every leg, `lsm_cgroup` attaches as
  assumed and ADV-03 counts 0, in which case the early reads bought little over doing them in
  Phase 11.
- Design falsifiers, not phase errors: empty-map BPF-LSM alone exceeding the S3 bound is a
  trigger (b) candidate; an ADV-03 count above 0 is a trigger (c) candidate, whose try-first step is
  the broker under its own uid.

</specifics>

<deferred>
## Deferred Ideas

- The broker's implementation language: Phase 10 decides it. D-13 and D-14 do not.
- A second, naive stub with a claimed-id payload as a sanity control: considered, not taken. The D-15
  (iii) control covers the claimed-id case.
- Measuring `lsm/unix_may_send` beside `lsm/socket_sendmsg`: not taken; revisit if the S5 read in
  Phase 10 shows the unix-socket hook set matters.
- Three S1 cases per class, or one per attachable hook family: ADV-04 and Phase 11 to 13 extend the
  skeleton.
- Reference-host verdicts on Debian 13, Fedora, the RHEL 10 family and Arch: Phase 12 (DIST-01 to
  DIST-03), not this phase.

</deferred>

---

*Phase: 8-Reference environment, early falsifiers and threshold gate*
*Context gathered: 2026-10-28*
