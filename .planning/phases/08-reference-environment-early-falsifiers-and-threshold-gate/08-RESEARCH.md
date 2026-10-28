# Phase 8: Reference environment, early falsifiers and threshold gate - Research

**Researched:** 2026-10-28
**Domain:** Linux BPF-LSM and cgroup-BPF measurement on a bare-metal Ubuntu 24.04 HWE 7.0 host; Rust same-uid adversarial suite; user threshold gate
**Confidence:** MEDIUM-HIGH. Kernel-source facts are read from kernel master as of 2026-10-28 and carried to Ubuntu 7.0.0-42 on the assumption that the code paths did not change (A1). Nothing here was run: the session host is WSL2 without root, so every runtime claim about BPF is a source reading, not an observation.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

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

### Deferred Ideas (OUT OF SCOPE)

- The broker's implementation language: Phase 10 decides it. D-13 and D-14 do not.
- A second, naive stub with a claimed-id payload as a sanity control: considered, not taken. The D-15
  (iii) control covers the claimed-id case.
- Measuring `lsm/unix_may_send` beside `lsm/socket_sendmsg`: not taken; revisit if the S5 read in
  Phase 10 shows the unix-socket hook set matters.
- Three S1 cases per class, or one per attachable hook family: ADV-04 and Phase 11 to 13 extend the
  skeleton.
- Reference-host verdicts on Debian 13, Fedora, the RHEL 10 family and Arch: Phase 12 (DIST-01 to
  DIST-03), not this phase.
</user_constraints>

<phase_requirements>
## Phase Requirements

Texts are from `.planning/REQUIREMENTS.md` lines 53-94, shortened; the Test clause is the part the plan must satisfy.

| ID | Description | Research Support |
|----|-------------|------------------|
| ENV-01 | Privileged test environment: root, `bpf` in the active LSM list, `bpftool`, clang BPF target, libbpf headers, delegated cgroup v2 subtree. Test: probe records `bpf` in `/sys/kernel/security/lsm`, uid 0, `bpftool` version, an `lsm/file_open` program loaded and unloaded. | Finding F1 (Ubuntu -42 carries `bpf`), F2 (tooling comes from the linux-tools package), F3 (`lsm=` fallback). Attach success is not proof the hook runs (F1). |
| ENV-02 | User names one reference kernel and host; env file gives kernel, cmdline, active LSM list, Landlock ABI, floor-row presence; decision record points to it. | F1, F2, F13 (extra `env.txt` keys). D-05 extends `run.sh write_env`. |
| HARN-01 | Re-run micro, broker, deadline, cgroup on the reference host; per-leg transfer table vs run `20261028T034700Z`. | F12 (`bpf` in LSM list is itself a confound), F14 (what must run in a user login session vs the root wrapper). |
| HARN-02 | BPF-LSM per-hook overhead, empty-map and populated-map variants, plus a unix-socket send hook; added % vs C0 and vs C3; R1 redone with the BPF share. | F4 (return convention), F5 (hook attachability), F9 (measurement validity), F11 (R1 arithmetic and the smallest script). |
| HARN-03 | `lsm_cgroup` attach via `bpftool`, return-1-to-allow, per-cgroup measurement; global fallback if attach fails. | F6 (lsm_cgroup semantics), F7 (bpftool route and its two defects in the section 8 sketch), F8 (where the process must sit). |
| ADV-01 | Adversarial suite before the mediation; non-agent run shows every valid case's side effect. | F10 (route table), F15 (Rust `sys` module), Pitfalls 4-7. |
| ADV-02 | S5 attempt set from the broker's uid; every attempt succeeds for a non-agent against a stub broker. | F10, Pitfall 4 (Yama `ptrace_scope=1` makes five ptrace routes invalid by default), F16 (`SO_PEERPIDFD` semantics). |
| ADV-03 | Early S5 read with `task_kill`, `ptrace_access_check` and unix-socket hooks loaded; count successes from an agent cgroup. | F10, F5, Pitfall 5 (`cgroup.kill` bypasses `task_kill`). |
| DEC-01 | User accepts, changes, rejects or defers each of S1 to S9 and POSX-02. | D-19/D-20; Validation Architecture row DEC-01. |
| DEC-02 | S3 openat bound decided, option named, run directories named. | D-18; F11 (what R1 yields with the BPF share, informational only under option 1). |
| DEC-03 | Floor tiers, reachability rule, five distros. | D-21; F1 (Ubuntu HWE row evidence), F13 (floor-row probe). |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

Sources: `/home/rakdos/repos/agentic-os-kernel/CLAUDE.md` (imports `AGENTS.md`), `.claude/CLAUDE.md`, and the user's global `~/.claude/CLAUDE.md`.

- Evidence labels: wiki claims cite PDF pages; design reasoning not stated by a source is marked *(interpretation)*. This phase's documents use VERIFIED, ASSUMED, NOT MEASURED, *(interpretation)* (CONTEXT.md code_context).
- Tooling: utilities are stdlib Python run with `python3 -I`; the baseline harness stays "gcc and glibc only".
- `#![deny(unsafe_code)]` and `docs/architecture/unsafe-ledger.md` govern the kernel crate. The Phase 8 Rust suite is outside them (D-13), but keep the single `sys` module discipline.
- Compatibility: agent features are additive; denials map to standard errno values (relevant to the HARN-03 errno check).
- Commits: no `Co-Authored-By:` or `Claude-Session:` trailers; short imperative subjects; one logical change per commit.
- Database-test rule (global) does not apply: no database in this phase.
- Terminals: one shell session per agent, no background shells. The wrapper and suite should not daemonize beyond the stub broker, which the suite itself spawns and reaps.
- Project instruction: direct repo edits go through a GSD command. The plans this research feeds are that route.
- `STATE.md`, `PROJECT.md`, `HANDOFF.json` are stale (archived B3 design); `ROADMAP.md` is authoritative.
- Global voice rule: when a source number was measured vs assumed, say which. Every table below follows that.

## Summary

The reference choice works, with one dated risk. Ubuntu `linux-hwe-7.0` 7.0.0-42.42~24.04.1 reached the Updates pocket on 2026-10-28 and its `CONFIG_LSM` contains `bpf`; -38 does not. The fix (LP #2166235) is still marked as awaiting verification and carries a note that it will be dropped if not verified in 5 working days. A newer build could therefore lose `bpf`. The gate in D-04 is right to key on the running kernel's `/sys/kernel/security/lsm`; add one thing: on a kernel without the patch, a BPF LSM program attaches "without error and never run", so attach success proves nothing. The ENV-01 probe must make the hook fire and read a counter. `lsm=landlock,lockdown,yama,integrity,apparmor,bpf` on the kernel command line is the fallback.

The research questions mostly resolve in favour of the CONTEXT.md design. `socket_sendmsg` is attachable and is not in the disabled set. A global program returns 0 to allow or a negative errno to deny. `lsm_cgroup` returns 1 to allow, 0 to deny (default errno -EPERM, changed with `bpf_set_retval`), at most 10 distinct hooks system-wide, never sleepable. `bpftool cgroup attach <cg> lsm_cgroup pinned <path>` parses even though the help text omits it, and the attachment outlives `bpftool`. The timed process needs no BPF privilege. A root wrapper can load, attach, move itself into the target cgroup and drop to the unprivileged uid before calling `run.sh`, because `micro` is launched with plain `taskset` and inherits the cgroup.

Five things in the existing design or sketches cannot work as written, or silently invalidate a result:

1. linux-baseline.md section 8 step 5 uses `bpftool prog load` on a three-program object; that loads **only the first program**. Use `prog loadall` and pin by function name.
2. Ubuntu's default Yama `ptrace_scope` restricts `PTRACE_ATTACH` to descendants. Five D-16 ptrace routes fail the same-uid non-agent baseline unless `ptrace_scope=0` or the attacker is the victim's ancestor, and then are marked invalid, and S5 reads nothing for them. Decide this before the first suite run.
3. Writing `cgroup.kill` does not pass `task_kill` (the kernel sends SIGKILL internally). The deny counter for that route will not be a `task_kill` counter.
4. `lsm_cgroup` on `socket_sendmsg` and `unix_may_send` is keyed on the **socket's** cgroup, not the sender's. For D-10 the hit-or-miss semantics of the per-cgroup variant differ from the global one.
5. Having `bpf` in the LSM list, with no program loaded, already changes the cost of every hook. The WSL2 baseline had no `bpf` active. HARN-01 therefore does not isolate the hypervisor (D-12) unless a no-`bpf` boot entry also exists.

**Primary recommendation:** Build the wrapper around pinned global-program links, one per-cgroup attach made from inside the target cgroup's ancestor, a per-CPU counter map pinned with `pinmaps`, and a deny probe that keys on a *probe cgroup id* (not a sentinel file). Add a small stdlib `r1.py` for the arithmetic. Resolve the ptrace_scope question and the `bpf`-in-list confound before the suite is written.

## Architectural Responsibility Map

The "tiers" here are privilege and ownership tiers on one host, not web tiers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Load/attach BPF programs, create cgroups, set sysctls | Root wrapper (setup only) | — | `bpf_prog_attach` needs privilege (`bpf_net_capable()` check in `bpf_prog_attach`, kernel/bpf/syscall.c, master; line not recorded); D-08 keeps root out of the timed path. |
| Mediation (deny/allow, counters) | Kernel BPF-LSM / lsm_cgroup | Yama, Landlock, seccomp (already in C3) | Programs run inside the kernel on every task; the wrapper only installs them. |
| Timed measurement | Unprivileged `micro` via `run.sh` | Wrapper (sets cgroup and uid before exec) | D-08. |
| Counter read, denial probe | Root wrapper | — | Pinned maps are root-only on bpffs. |
| Adversarial cases, stub broker | Unprivileged Rust suite (agent and non-agent runs) | Root wrapper (sysctl, hook loading) | Cases must run with the broker's uid (ADV-02). |
| Identity stamping | Kernel (`SO_PEERPIDFD`, `PIDFD_GET_INFO`) | Stub broker reads it | Mirrors `broker.c broker_id` (D-15). |
| R1 arithmetic, transfer table | Stdlib Python over `summary.csv` | — | `summarize.py` has no R1 code. |
| Gate sheet and Status cells | Docs (`08-GATE.md`, `scoping-linux.md`, ADR 0004) | User marks | D-20. |

## Standard Stack

### Core
| Library / Tool | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Ubuntu 24.04 HWE kernel `linux-hwe-7.0` | 7.0.0-42.42~24.04.1 (Updates, 2026-10-28) | Reference kernel | D-02. [VERIFIED: Launchpad API `getPublishedSources`, 2026-10-28] |
| `bpftool` | from `linux-hwe-7.0-tools-7.0.0-42`, via `linux-tools-generic-hwe-24.04` | Load, attach, pin, dump maps | No standalone `bpftool` package in noble. See F2. |
| clang/llvm | 1:18.0-59~exp2 (noble); local clang 18.1.3 lists target `bpf` | Compile `*.bpf.c` | [VERIFIED: local `clang --print-targets` shows `bpf`, `bpfel`, `bpfeb`] |
| `libbpf-dev` | 1:1.3.0-2build2 (noble Release) | `bpf_helpers.h`, `bpf_tracing.h` | [VERIFIED: Launchpad API 2026-10-28] |
| Rust (stable, pinned) | exactly one of the installed stable releases; `stable` alias resolves to 1.98.0 on this host today | Adversarial suite and stub | D-13. |
| `libc` crate | `=0.2.191` (crates.io, published 2026-10-16, `rust-version = "1.65"`, edition 2021) | Syscall and constant bindings | D-13. |
| Python | 3.12.3 here; stdlib only, `python3 -I` | `r1.py`, report checks | Repo convention. |

### Supporting
| Tool | Purpose | When to Use |
|------|---------|-------------|
| `setpriv` (util-linux) | Drop to the unprivileged uid inside the target cgroup | Wrapper, before `run.sh`. Do not pass `--no-new-privs`: baseline `env.txt:22-23` has `shell_seccomp=0`, `shell_nonewprivs=0` and D-08 wants the same credentials. |
| `systemd-run --user` | Delegated cgroup for deadline and cgroup suites | Only for HARN-01 runs launched from the user's own login session (F14). |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `libc =0.2.191` | `libc 1.0.0-alpha.5` (2026-10-02) | Alpha. Do not use. |
| `libc` for `CLONE_INTO_CGROUP` | hand-defined `u64` const | Mandatory. libc types it `c_int` (see F15). |
| Sketch's `prog load` | `prog loadall` | Mandatory for multi-program objects (F7). |

**Installation (reference host):**
```bash
# kernel and tools, from -updates (default sources carry it)
sudo apt install linux-generic-hwe-24.04 linux-tools-generic-hwe-24.04 clang llvm libbpf-dev
uname -r                        # must be 7.0.0-42-generic or later after reboot
cat /sys/kernel/security/lsm    # must contain bpf
```
Package names `linux-generic-hwe-24.04` and `linux-tools-generic-hwe-24.04` are VERIFIED to exist at -42 (Launchpad API). Whether a fresh install of the newest point-release ISO already boots 7.0 is ASSUMED; the install plan must run the `uname -r` check, not trust the ISO.

**Version verification done this session:** `libc` 0.2.191 source unpacked and read; Launchpad API for kernel and tools packages; crates.io API. No `npm`/`pip` is used.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| libc | crates.io | since 2015-01-15 | 34,075,271/wk | github.com/rust-lang/libc | OK (`gsd_run query package-legitimacy check --ecosystem crates libc`, run 2026-10-28, no reasons, no postinstall) | Approved, pin `=0.2.191` |

**Packages removed due to SLOP verdict:** none.
**Packages flagged SUS:** none.

The apt packages (`clang`, `llvm`, `libbpf-dev`, `linux-tools-*`, `linux-generic-hwe-24.04`) come from the Ubuntu archive; the package-legitimacy seam covers npm, PyPI and crates only. Their existence is VERIFIED against the Launchpad API, not the seam. No other crate is needed: `goblin`, `zerocopy` and the rest of `.claude/CLAUDE.md`'s user-side stack do not belong in this phase.

## Architecture Patterns

### System Architecture Diagram

```
                      reference host (dual boot, Ubuntu 7.0.0-42+)
                                    |
      [root wrapper: tools/.../bpflsm.sh]  -- one per variant ---------------------+
        1 probe env, write env.txt keys (F13)                                      |
        2 clang -target bpf -> *.bpf.o   (vmlinux.h from bpftool btf dump)         |
        3 bpftool prog loadall <o> /sys/fs/bpf/aok/<variant> autoattach pinmaps    |
              global lsm/*  -> pinned LINKS (stay attached while pins exist)       |
        4 [lsm_cgroup variant] bpftool cgroup attach <cg> lsm_cgroup pinned <p>    |
              -> attachment lives in the cgroup, no pin needed                     |
        5 populate policy map: key = cgroup id of the timed cgroup                 |
        6 deny probe  (probe cgroup id -> deny)  must fail EACCES, then allow      |
        7 read counters (before)                                                   |
        8 echo $$ > <timed cgroup>/cgroup.procs ; setpriv --reuid ... run.sh \     |
              --suites micro --configs C0,C3 --tag bpflsm-<variant>  --------------+--> taskset -c N micro  (unprivileged)
        9 read counters (after); delta >= call count else leg INVALID                    |   every hook fires in kernel:
       10 detach, rm pins                                                                |   security_* -> bpf_lsm_<hook> stub
                                                                                         |   -> trampoline -> program
      results/<run>/micro.csv, summary.csv, env.txt  <----------------------------------+   map lookup, counter++
                    |
        python3 -I r1.py summary.csv  --> added% vs C0 and vs C3, R1 value (informational under D-18)
                    |
        08-GATE.md (D-20) --> user marks --> scoping-linux.md section 4 Status cells + ADR 0004 line

 Adversarial suite (Rust, unprivileged, tools/aok-adv/):
   non-agent run (plain cgroup)  --routes-->  stub broker (spawned by the suite, restarted per route)
   agent run (agent cgroup, BPF task_kill / ptrace_access_check / unix hooks loaded by the wrapper) --routes--> same stub
   stub: accept -> SO_PEERPIDFD -> PIDFD_GET_INFO(cgroupid) -> reply identity
   report: one row per route: valid? (non-agent ok) , agent result, deny counter delta
```

### Recommended Project Structure
```
tools/
├── linux-baseline/            # unchanged except run.sh write_env additions (D-05)
│   └── r1.py                  # new, stdlib, reads summary.csv (F11)
├── bpflsm/                    # new (names are Claude's discretion)
│   ├── bpflsm.sh              # root wrapper
│   ├── bpflsm.bpf.c           # global programs: file_open, socket_connect, bprm_check_security, socket_sendmsg
│   ├── bpflsm_cg.bpf.c        # lsm_cgroup programs
│   └── bpfadv.bpf.c           # task_kill, ptrace_access_check, unix hooks for ADV-03
└── aok-adv/                   # new Rust crate (own rust-toolchain.toml)
    ├── Cargo.toml             # libc = "=0.2.191"
    ├── rust-toolchain.toml    # exact stable version
    └── src/{main.rs, sys.rs, cases/, stub.rs, report.rs}
```

### Pattern 1: global programs, pinned links, persistent after the loader exits
`bpftool prog loadall <obj> <dir> autoattach pinmaps <mapdir>`. For a `lsm/` section `bpf_program__attach` succeeds, `auto_attach_program` pins the link, and the attachment lasts while the pin exists [VERIFIED: bpftool prog.c, `auto_attach_program` on master; line numbers not recorded]. Maps are pinned **only** with `pinmaps`. Without it the counter map is unreachable by name.

### Pattern 2: lsm_cgroup attach is a program attach, not a link
```bash
bpftool prog loadall bpflsm_cg.bpf.o /sys/fs/bpf/aok/cg pinmaps /sys/fs/bpf/aok/cg-maps
# loadall without autoattach pins each program at <dir>/<function name> (libbpf bpf_object__pin_programs uses prog->name, libbpf.c; line not recorded)
bpftool cgroup attach /sys/fs/cgroup/<cg> lsm_cgroup pinned /sys/fs/bpf/aok/cg/<function_name> multi
```
`do_attach` ends in `bpf_prog_attach(prog_fd, cgroup_fd, attach_type, attach_flags)` [VERIFIED: bpftool cgroup.c]. No link, so the attachment survives `bpftool` exiting and ends on detach or cgroup removal. With `autoattach`, `SEC_DEF("lsm_cgroup+", LSM, BPF_LSM_CGROUP, SEC_ATTACH_BTF)` has no attach function, so `bpf_program__attach` fails and bpftool prints "does not support autoattach, falling back to pinning" [VERIFIED: libbpf.c, prog.c]. Either form works; the message is not an error.

### Pattern 3: put the timed process into the cgroup by inheritance
`run.sh` launches `micro` as `taskset -c "$CPU" "$BIN/micro" ...` (run.sh:208); only the deadline and cgroup suites go through `run_pinned`/`systemd-run --user` (run.sh:287-290). A wrapper that has done `echo $$ > <cg>/cgroup.procs` and then `setpriv ... run.sh` therefore runs `micro` inside `<cg>`. This is what makes the per-cgroup variant feasible without touching `run.sh`. The transient-unit route would put the process under `user@UID.service`, outside any cgroup the wrapper created.

### Pattern 4: counter that proves the hook fired
`BPF_MAP_TYPE_PERCPU_ARRAY`, one slot per hook, increment with a plain add (no atomic) in each program. A global program runs for every task on every CPU; with a shared array and `__sync_fetch_and_add` other CPUs would contend on the pinned CPU's line. *(interpretation)* The cost is NOT MEASURED. Read with `bpftool map dump pinned <path>` and sum the per-CPU values. The test "delta at least the leg's call count" stays valid when other tasks also increment, because noise only raises the delta; it is a necessary condition for "the hook fired for the timed process", not a precise count.

### Anti-Patterns to Avoid
- **Trusting "attach succeeded".** Ubuntu's own SRU cover letter says unpatched kernels attach a BPF LSM program "without error and never run" [CITED: lists.ubuntu.com kernel-team, Sept 2026 patches 171489, 171517].
- **Denying a whole cgroup to prove denial.** A deny keyed on the timed cgroup id breaks the micro legs. Key the deny probe on a separate probe cgroup id (see Code Examples).
- **Reading `bpf_stats_enabled` counters during timed legs.** The sysctl "adds a small performance cost to each program run" (kernel sysctl docs); the cost is NOT MEASURED. Use it only in an untimed pre-leg as an independent `run_cnt` cross-check.

## Findings by question

### F1. Ubuntu `bpf` in `CONFIG_LSM` (Q4)
- At tag `Ubuntu-hwe-7.0-7.0.0-42.42_24.04.1` the annotations file reads `CONFIG_LSM policy<{'amd64': '"landlock,lockdown,yama,integrity,apparmor,bpf"', ...}>` (line 8032), `CONFIG_BPF_LSM ... 'amd64': 'y'` (2446), `CONFIG_CGROUP_BPF ... 'y'` (2780), `CONFIG_DEBUG_INFO_BTF ... 'amd64': 'y'` (141) [VERIFIED: kernel.ubuntu.com annotations, fetched 2026-10-28]. At tag -38 (line 8033) the string has no `bpf` [VERIFIED].
- `linux-hwe-7.0` 7.0.0-42.42~24.04.1 is Published in Updates (2026-10-28T00:08Z). Security still serves -38; -39 is in Proposed [VERIFIED: Launchpad API]. Any plan that pins to `-security` misses the fix; default Ubuntu sources include `-updates`.
- LP #2166235 ("append bpf to CONFIG_LSM"): awaiting verification for linux-hwe-7.0 7.0.0-42; a bot note says the fix will be dropped if verification is not done in 5 working days [CITED: Launchpad bug page]. A later build could lose `bpf`. D-02 says "-42 or later"; read "later" as "later and still printing `bpf`".
- Ubuntu's stated reason for excluding it before: "bpf was intentionally excluded in LP: #1905975 pending removal of the per-hook indirect-call overhead upstream" [CITED: kernel-team list]. That is the exact quantity HARN-02 measures.
- `/sys/kernel/security/lsm` prints `capability` and `lockdown` first even though `CONFIG_LSM` omits `capability`. Patched expectation from the same list: `lockdown,capability,landlock,yama,integrity,apparmor,bpf`; unpatched sample: `lockdown,capability,landlock,yama,apparmor,ima,evm` [CITED: kernel-team list]. Test with `grep -qw bpf`, not string equality.
- Fallback: `lsm=` "Choose order of LSM initialization. This overrides CONFIG_LSM, and the 'security=' parameter." [VERIFIED: kernel-parameters.txt]. It replaces the list; restate the whole list: `GRUB_CMDLINE_LINUX_DEFAULT="... lsm=landlock,lockdown,yama,integrity,apparmor,bpf"`, then `sudo update-grub`, reboot. The list is copied from the -42 `CONFIG_LSM`; it is not tested here.
- dmesg on a booted BPF-LSM kernel prints `LSM support for eBPF active` [VERIFIED: security/bpf/hooks.c, master]. Record it as a second signal.
- D-04 amendment: add "a counter program on `file_open` shows a non-zero count after `cat /etc/hostname`". That is the only check that distinguishes patched from "attaches and never runs".

### F2. Tooling on Ubuntu 24.04 (Q3)
- No standalone `bpftool` binary package in noble (Launchpad `getPublishedBinaries` for the exact name returned nothing). The binary is `/usr/lib/linux-hwe-7.0-tools-7.0.0-42/bpftool` in `linux-hwe-7.0-tools-7.0.0-42`; `linux-tools-7.0.0-42-generic` symlinks `/usr/lib/linux-tools/7.0.0-42-generic -> ../linux-hwe-7.0-tools-7.0.0-42` [VERIFIED: `dpkg-deb -c` on the downloaded debs].
- `linux-tools-common` ships `/usr/sbin/bpftool`, a wrapper that execs `/usr/lib/linux-tools/$(uname -r)/bpftool` [VERIFIED]. It works only if the tools package for the **running** kernel is installed. Install `linux-tools-generic-hwe-24.04` (at -42), reboot into -42, then `bpftool version`.
- Root wrapper then drop uid: yes. Global programs hold their attachment through a pinned link on bpffs; `lsm_cgroup` through the cgroup attachment. After the wrapper has done both, an unprivileged process needs no BPF capability. `bpf_prog_attach` itself is privileged (`bpf_net_capable()` check; `BPF_PROG_ATTACH` case in `kernel/bpf/syscall.c`) [VERIFIED: master source]. Unprivileged BPF is irrelevant to the timed path: `unprivileged_bpf_disabled=2` is the Ubuntu default (`CONFIG_BPF_UNPRIV_DEFAULT_OFF=y` in the annotations) and should be left alone.
- bpffs: `bpftool` creates and mounts `/sys/fs/bpf` when needed (`create_and_mount_bpffs_dir`, `mount_bpffs_for_file` in prog.c). Pins vanish on reboot; the wrapper recreates them per run.
- Local check on this WSL2 host: `findmnt /sys/fs/bpf` printed nothing; `bpftool` absent; no root. Consistent with CONTEXT.md.

### F3. `lsm=` fallback syntax
See F1. Also: `bpf` is a non-exclusive LSM, so it coexists with the exclusive `apparmor` in the list. Treat that as ASSUMED until the first boot.

### F4. Global BPF-LSM return convention (Q2)
- `int` hooks: 0 allow, negative errno deny; the verifier rejects other ranges (`MAX_ERRNO` window). `bool` hooks return 0 or 1 (`bool_lsm_hooks` in bpf_lsm.c; ASSUMED from memory, not quoted). `void` hooks have no return range [VERIFIED: bpf_lsm.c, verifier.c, master].
- `file_open`, `socket_connect`, `bprm_check_security`, `socket_sendmsg`, `task_kill`, `ptrace_access_check`, `unix_may_send`, `unix_stream_connect`, `socket_bind`, `path_unlink` are all `int` hooks [VERIFIED: lsm_hook_defs.h lines 36, 54, 91, 216, 262, 335, 337, 342, 344, 348]. Quote of the signatures that matter:
  - `LSM_HOOK(int, 0, file_open, struct file *file)` (216)
  - `LSM_HOOK(int, 0, socket_sendmsg, struct socket *sock, struct msghdr *msg,` (348)
  - `LSM_HOOK(int, 0, unix_may_send, struct socket *sock, struct socket *other)` (337)
  - `LSM_HOOK(int, 0, unix_stream_connect, struct sock *sock, struct sock *other,` (335)
  - `LSM_HOOK(int, 0, ptrace_access_check, struct task_struct *child,` (36), `task_kill, struct task_struct *p, struct kernel_siginfo *info,` (262)
- A denial reaches the caller as that errno. The D-09 sentinel expects `EACCES`: return `-13` from a global program.

### F5. Which hooks can attach; "sleepable-only" (Q2)
- The disabled set `bpf_lsm_disabled_hooks` (bpf_lsm.c:39-59, master) is: `vm_enough_memory`, `inode_need_killpriv`, `inode_getsecurity`, `inode_listsecurity`, `inode_copy_up_xattr`, `getselfattr`, `getprocattr`, `setprocattr`, `ismaclabel`, `file_alloc_security`, plus `key_getsecurity` (CONFIG_KEYS), `audit_rule_match` (CONFIG_AUDIT), `xfrm_decode_session` (CONFIG_SECURITY_NETWORK_XFRM) [VERIFIED: quoted from the file]. None of `file_open`, `socket_connect`, `bprm_check_security`, `socket_sendmsg`, `task_kill`, `ptrace_access_check`, `unix_may_send`, `unix_stream_connect`, `socket_bind`, `path_unlink` appears. D-10's ASSUMED attachability is therefore supported by source; first-load on the host still confirms it.
- There is no sleepable-**only** hook. `sleepable_lsm_hooks` (bpf_lsm.c) lists hooks where a sleepable program is **allowed**. `bpf_lsm_file_open` and `bpf_lsm_socket_sendmsg` are in it. `task_kill`, `ptrace_access_check`, `unix_may_send`, `unix_stream_connect` are not, so programs there must not be sleepable [VERIFIED by grep of the file]. Write all Phase 8 programs non-sleepable (`SEC("lsm/...")`, not `lsm.s/...`).
- ADV-04's "hooks that cannot take a BPF-LSM attach" list is exactly the disabled set above.

### F6. `BPF_LSM_CGROUP` semantics (Q1)
- Return: the run loop does `func_ret &= 1` and `if (!func_ret && cgroup_bpf_hook_returns_errno(atype) && !IS_ERR_VALUE((long)run_ctx.retval)) run_ctx.retval = -EPERM;` (kernel/bpf/cgroup.c:134-138). So **1 allows, 0 denies, and a deny becomes -EPERM unless the program called `bpf_set_retval(errno)` first** [VERIFIED: master]. The selftest `progs/lsm_cgroup.c` returns `0; /* EPERM */` on deny and `1` on allow, and `prog_tests/lsm_cgroup.c` asserts `ASSERT_EQ(skel->data->socket_retval, -EPERM, ...)` [VERIFIED]. Both ASSUMED items in linux-baseline.md section 9 are therefore settled on the source side; HARN-03 still observes them on the host.
- `bpf_set_retval`/`bpf_get_retval` are available to lsm_cgroup programs through `cgroup_common_func_proto` (bpf_lsm.c:252-256) [CITED: source; exact helper list not read].
- If the target errno must be `EACCES` for the D-09 style probe, the lsm_cgroup program needs `bpf_set_retval(-EACCES); return 0;`. A bare `return 0` yields EPERM, so HARN-03's errno check must expect EPERM by default.
- Shim keyed by first argument type (`bpf_lsm_find_cgroup_shim`, bpf_lsm.c:94-116): zero args or a hook in `bpf_lsm_current_hooks` uses `__cgroup_bpf_run_lsm_current` (`task_dfl_cgroup(current)`); first arg `struct socket *` uses `__cgroup_bpf_run_lsm_socket`; `struct sock *` uses `__cgroup_bpf_run_lsm_sock`; anything else, `current`. `sk_alloc_security`/`sk_free_security` are forced to current [VERIFIED]. Consequences:
  - `file_open`, `socket_connect`'s `struct socket *`... note `socket_connect(struct socket *sock, ...)` and `socket_sendmsg(struct socket *sock, ...)` and `unix_may_send(struct socket *sock, ...)` key on the **socket's** cgroup (`sk_cgrp_data`, the cgroup in which the socket was created), `unix_stream_connect(struct sock *sock, ...)` on the connecting sock's. `file_open(struct file *)`, `bprm_check_security(struct linux_binprm *)`, `task_kill(struct task_struct *, ...)`, `ptrace_access_check(struct task_struct *, ...)` key on `current` [first-arg types VERIFIED from lsm_hook_defs.h; the shim mapping follows from the code above; *(interpretation)* for the per-hook conclusion].
  - A socket made in cgroup A and passed over `SCM_RIGHTS` to a process in B is still evaluated against A. For the per-cgroup `socket_sendmsg` variant this is the opposite of the global variant keyed on `current`. HARN-02 compares them on a socket each side creates itself, so timing is unaffected; ADV-03 (route D-15 (ii)) is affected and must pick the global form.
- Limits: `#define CGROUP_LSM_NUM 10` (kernel BPF cgroup headers; defining file not recorded); an eleventh distinct hook returns `-E2BIG` [VERIFIED]. Phase 8 plans use at most 8 distinct hooks across HARN-03 and ADV-03 (`file_open`, `socket_connect`, `bprm_check_security`, `socket_sendmsg`, `task_kill`, `ptrace_access_check`, `unix_may_send`, `unix_stream_connect`). Two spare. Adding the `uring_*` family would overflow.
- `lsm_cgroup` programs cannot be sleepable: `if (prog->type == BPF_PROG_TYPE_LSM) return prog->expected_attach_type != BPF_LSM_CGROUP;` (verifier.c:22937) [VERIFIED].
- Cost shape *(interpretation, from code)*: once any `lsm_cgroup` program is attached to hook H, the trampoline on H's stub runs `__cgroup_bpf_run_lsm_*` for **every task** hitting H, which then dereferences the cgroup's effective array. Tasks outside the attached cgroup pay the lookup and find an empty array. So the "sibling cgroup" row is not free, and "unattached root cgroup" is a separate S2-relevant row (Claude's Discretion item). NOT MEASURED.
- Inheritance: `cgroup_bpf_inherit` (kernel/bpf/cgroup.c:680) computes the child's effective array from the parent. Attaching to an ancestor therefore reaches descendants for ordinary cgroup-BPF types [VERIFIED: function exists]. The `lsm_cgroup` selftest checks only sibling isolation, not descendants. Plan an explicit check: attach at a parent, open a file from a child leaf. Until then descendant coverage for `lsm_cgroup` is ASSUMED (A4).

### F7. `bpftool` route and defects in the section 8 sketch (Q1)
- `bpftool cgroup attach <cgroup> lsm_cgroup pinned <path>` parses: `parse_attach_type` accepts any name from `libbpf_bpf_attach_type_str`, and libbpf has `[BPF_LSM_CGROUP] = "lsm_cgroup"`. The `bpftool cgroup help` text omits it. Flags accepted: `multi`, `override`, `preorder` [VERIFIED: bpftool cgroup.c, libbpf.c].
- **Sketch defect 1.** linux-baseline.md section 8 step 5 runs `bpftool prog load bpflsm_cg.bpf.o /sys/fs/bpf/aok-bpflsm-cg`. `do_load` calls `load_with_options(argc, argv, true)` (first_prog_only), which loads and pins `bpf_object__next_program(obj, NULL)` and nothing else (prog.c). Section 8 describes three programs in that object. Use `prog loadall <obj> <dir>` and attach `<dir>/<function name>` per hook.
- **Sketch defect 2.** The section 8 step 4 `loadall ... autoattach` pins no map, so the counter and policy maps cannot be read or populated by name. Add `pinmaps <dir>`.
- The sketch's `check()` discards the map value. A hit variant must act on it, otherwise the compiler may elide the difference between hit and miss paths; the verifier does not require use, but `-O2` may drop the lookup-result branch. Keep the lookup (it is a helper call and is not removed) and branch on the value.
- Detach: `bpftool cgroup detach <cg> lsm_cgroup pinned <path>`; `rm` the pins.

### F8. Where the timed process must sit (Q1, Q3)
Per Pattern 3. Cgroup layout that satisfies the Discretion item: `/sys/fs/cgroup/aok-phase8/{timed,sibling,probe}` created by root, directories `chown`ed to the test uid only if the suite itself must write them (the `micro` timed path does not). Attach `lsm_cgroup` to `timed`. The sibling test moves the wrapper into `sibling` and runs the same `openat`. v2 no-internal-process rule is not hit unless controllers are enabled in `subtree_control`; do not enable any here.

### F9. Measurement validity (Q9)
| Item | Finding | Tag |
|------|---------|-----|
| `kernel.bpf_stats_enabled` | Default 0. 1 collects `run_cnt`/`run_time_ns` and "adds a small performance cost to each program run". | Cost NOT MEASURED. Keep 0 for timed legs; use 1 only in an untimed pre-leg. |
| Counter increment | Per-CPU array plain add recommended. | Cost NOT MEASURED; D-09 already treats it as upward bias. A counter-free spot check on one leg (Discretion) bounds it. |
| `bpf` in LSM list, no program | `security/bpf/hooks.c` registers every hook in `lsm_hook_defs.h` and `bpf_lsm.c` defines a weak nop `bpf_lsm_<hook>` stub for each, so every hook has an active callee. | VERIFIED on master source; cost NOT MEASURED. WSL2 baseline had no `bpf` active (`env.txt:25`). |
| Global program hits every task | Yes; all CPUs, all tasks. Counter delta is a lower bound for "timed process fired it". Background load adds noise to both counter and timing. | *(interpretation)* |
| Governor, SMT, boost | No source read for effect sizes. Record `scaling_governor`, `scaling_driver`, `boost`, `/sys/devices/system/cpu/smt/active`, and keep the SMT sibling idle. The harness already records `cpu_pinned_core_siblings` (`env.txt:9`: `3`). | ASSUMED relevant; effect NOT MEASURED. Setting the governor to `performance` needs root and belongs in the wrapper, restored afterwards. |
| Trampoline overhead | BPF-LSM programs attach through a BPF trampoline on the stub. No published figure was found. | NOT MEASURED. Do not quote a number. |
| Reproducibility | Five rounds measure round-to-round spread inside one boot. | Same limit as linux-baseline.md section 9 last bullet. |

### F10. S1 classes and S5 attempts: route, mediating hook, non-agent baseline (Q7)
All hook mappings below are ASSUMED from kernel structure and the gap-analysis input unless a VERIFIED mark is present. Only hook **signatures** were read this session. The non-agent baseline column is what D-16 requires to succeed; it uses the same uid and no agent cgroup.

S5 attempts (the stub broker runs as the same uid):

| Route | Syscall / action | Mediating hook | Non-agent baseline | Note |
|-------|------------------|----------------|--------------------|------|
| kill | `kill`, `tkill`/`tgkill`, `rt_sigqueueinfo`, `rt_tgsigqueueinfo` | `task_kill` | Succeeds, same uid | ASSUMED |
| kill | `pidfd_send_signal` | `task_kill` | Succeeds | ASSUMED |
| kill | write `1` to victim's `cgroup.kill` | **no `task_kill`**: `__cgroup_kill` calls `send_sig(SIGKILL, task, 0)` (kernel/cgroup/cgroup.c:4316) | Succeeds only if the writer has write access to that cgroup's `cgroup.kill` | VERIFIED call site; that `send_sig` skips `check_kill_permission` is ASSUMED. Mediation is on the file open/write (`file_open`, `file_permission`) or by cgroup file permissions. Deny counter for this route is a file hook counter. |
| ptrace | `PTRACE_ATTACH`, `PTRACE_SEIZE` | `ptrace_access_check` | **Fails under Ubuntu default Yama `ptrace_scope=1` unless attacker is an ancestor** | Pitfall 4 |
| ptrace | `process_vm_readv/writev`, open `/proc/<pid>/mem`, `pidfd_getfd` | `ptrace_access_check` via `ptrace_may_access` | Same Yama gate | ASSUMED. `pidfd_getfd` needs a ptrace-style access check [CITED: man7 pidfd_getfd(2), appeared Linux 5.6]. |
| bind | `unlink(path)` then `bind` | `path_unlink` / `inode_unlink`, `socket_bind` | Succeeds when the directory is writable by the uid and not sticky-protected | ASSUMED; needs the broker dead or the path removable |
| bind | abstract name | `socket_bind` | Succeeds only while the name is free (EADDRINUSE otherwise) | Needs a stub restart gap; this is the Discretion "stub restart" item |
| impersonation (i) | write own pid to victim's `cgroup.procs` | no dedicated LSM hook; file hooks on the kernfs file | Succeeds only inside one delegated subtree the uid may write | `cgroup_procs_write_permission` / `cgroup_may_write` exist in cgroup.c:5443-5481 [VERIFIED]; the check passes within a delegated subtree for the same uid (ASSUMED) |
| impersonation (ii) | connected fd via `SCM_RIGHTS` or inheritance | `file_receive`, `unix_may_send`, `socket_sendmsg` | Succeeds: the peer identity is the connector, not the sender (F16) | Global form only (F6) |
| impersonation (iii) | claim another id in payload | none (control) | Must be ignored by the stub | |
| candidate (iv) | `clone3` with `CLONE_INTO_CGROUP` into the victim's cgroup | no clone-specific hook | Succeeds iff the uid may write the destination `cgroup.procs` (`cgroup_may_write(dst_cgrp)` at cgroup.c:6947, VERIFIED call) | **Not in D-16's list. Add it.** It yields a fresh process stamped with the victim's cgroup id and is the same capability as (i) by another syscall. |

S1 classes, one valid case each (D-17). Each needs a side-effect oracle; the hooks are the candidates ADV-04 will classify, so `closable_by` stays empty now.

| Class | Concrete route | Candidate hook / right | Non-agent baseline oracle |
|-------|----------------|------------------------|---------------------------|
| files | `openat` write to a path outside a granted tree | `file_open`; Landlock fs | file exists with expected bytes |
| network | `connect` to an unlisted TCP port | `socket_connect`; Landlock net | accepted connection at a local listener |
| tools | `execve` of a binary outside the allowlist | `bprm_check_security`; Landlock execute | marker file written by the binary |
| other agents | signal or `process_vm_readv` on a peer agent | `task_kill`, `ptrace_access_check` | peer observes SIGUSR1 / value read |
| model | `connect` to the inference endpoint bypassing the broker | `socket_connect`, `unix_stream_connect` | stub model endpoint logs a request |
| io_uring | `IORING_OP_OPENAT` / `IORING_OP_CONNECT` doing the files/network action | `uring_*`, plus the per-op hooks it reaches | same oracle as files or network, via ring |
| directly used API key | read a key file or env var and use it | `file_open` on the key file | stub endpoint logs the key |
| shell-out escape | spawn `/bin/sh -c` which performs a denied action as a child | `bprm_check_security`, inherited domains | marker file written by the child |

`io_uring` uses `SYS_io_uring_setup=425` and `SYS_io_uring_enter` from libc 0.2.191 (present for x86_64); the ring structs are not in libc and must be hand-defined in `sys`. The `uring_*` hooks exist: `uring_override_creds`, `uring_sqpoll`, `uring_cmd`, `uring_allowed` (lsm_hook_defs.h:471-474) [VERIFIED]. Whether they are attachable was not checked beyond their absence from the disabled set; they are not in the sleepable set.

ADV-03 deny rule (Discretion). Read the target's cgroup without helper trouble: `bpf_get_current_cgroup_id()` gives the caller. For `task_kill`/`ptrace_access_check` the target is a BTF `struct task_struct *`; the usual route is a direct BTF dereference of `task->cgroups->dfl_cgrp->kn->id`. ASSUMED to verify and pass under the Ubuntu lockdown level if Secure Boot turns lockdown on (A8). Avoid `bpf_probe_read_kernel` in these programs. The simplest rule with one map: deny when the caller's cgroup id is in the `agent` set and the target's cgroup id is not in the same agent's subtree set. Record rule and hooks in the ADV-03 report as D-discretion asks.

### F11. R1/R2 arithmetic and the smallest change (Q10)
- R1 as written (linux-baseline.md:555-557): "Revised bound = max(current bound, C3 added percent + 2 x the larger of the two spreads), rounded up to a whole percent, for p50 and p99 separately." Eligibility (551-554): stable row, baseline and config spreads both under 10 percent on the quantile the bound constrains; otherwise "none: unstable on this host". Worked example (566-567): `25.01 + 2 x 9.64 = 44.29`, rounded up to 45 [VERIFIED: file lines read this session].
- R2 (558-561): the smallest 1-2-5 value at or above 4 x measured p99 (added, for S4). It is not touched by BPF-LSM in this phase.
- `summarize.py`: `baseline_of("micro", config)` returns `None if config == "C0" else "C0"` (lines 176-178), so every tagged label (`C3-bpflsm-x`) is compared against untagged **C0** from the same run directory. `sort_key` uses `config.split("-")[0]` (line 256). It computes the added columns and `within_noise` and the stable mark; it has no R1 and no eligibility test.
- Consequences: every BPF-LSM run directory must also contain untagged C0 rows (and C3 for the "vs C3" column). `run.sh` exits 3 on a repeated (round, label); put C0 and C3 in the first invocation and the tagged label in a second invocation of the same `--run-id`. `write_env` runs only when `env.txt` is empty, so the second invocation does not rewrite it.
- Smallest change: a separate stdlib `r1.py` that reads `summary.csv` (column names to be read from the header of `summary.csv` when the script is written; not recorded in this research), and for each (tagged config, bench, quantile) applies the eligibility rule and `ceil(max(bound, added + 2*max(spread_cfg, spread_base)))`. The BPF-LSM share is the tagged-vs-C0 added percent in place of C3's; the incremental share is tagged minus C3. Under D-18 this value is informational (the bound is kept at 10%/25%); report it as "what R1 would have been", not as a revision.
- A numeric reproduction test for D-18 (Discretion): the reference host reproduces the openat miss if `openat_tmpfs` C3-vs-C0 p50 added is above 10 and the row passes the eligibility rule. *(interpretation)*

### F12. `bpf` in the LSM list is a confound for HARN-01
`env.txt:25` is `active_lsms=capability,landlock,yama,safesetid,selinux,ima`. The reference kernel necessarily adds `bpf`. HARN-01's table compares WSL2 (no `bpf`) with a host that has per-hook stubs, so a difference cannot be attributed to the hypervisor alone. Add a row: one HARN-01 micro run on the same boot with `bpf` active and **no program loaded**, and, if cheap, a second GRUB entry booting `lsm=` without `bpf` for C0/C3. Cost of the extra GRUB entry: one reboot. Either is Claude's-discretion; the planner should at least take the first.

### F13. `env.txt` additions (D-05, ENV-02, DEC-03)
Existing keys include `uname_r`, `cmdline`, `config_lsm`, `config_bpf_lsm`, `active_lsms`, `landlock_abi`, `so_peerpidfd`, `pidfd_get_info`, `unprivileged_bpf_disabled` (baseline `env.txt:4,15-18,25-28` [VERIFIED]). Add: `bpftool_version`, `btf_vmlinux` (present/absent of `/sys/kernel/btf/vmlinux`), `bpf_in_lsm`, `lsm_hook_fires` (counter after `cat /etc/hostname`), `attach_global_file_open` (ok/errno), `attach_lsm_cgroup` (separate, not in the ready gate), `dmesg_ebpf_lsm`, `yama_ptrace_scope`, `lockdown` (`/sys/kernel/security/lockdown`), `secure_boot`, `scaling_governor`, `scaling_driver`, `smt_active`, `kernel_pkg` (`dpkg -l linux-image-$(uname -r)` version), `apparmor_userns_restrict` (sysctl `kernel.apparmor_restrict_unprivileged_userns`), and one `floor_<row>=present|absent` per scoping-linux.md section 3 row. The floor rows themselves were not re-read in this research; the planner must take the list from that file. `/sys/kernel/security/lockdown` presence and `kernel.apparmor_restrict_unprivileged_userns` name are ASSUMED (A9).

### F14. What runs where (HARN-01 vs HARN-02/03)
The broker, deadline and cgroup suites use `systemd-run --user --wait --pipe --collect --same-dir -p Delegate=yes` (`run_pinned`, run.sh:287-290), which needs the user's systemd manager and D-Bus session. Run HARN-01 for all four suites from the user's own login shell, not through the root wrapper. Only the micro suite goes through the wrapper (it uses plain `taskset`). If a micro run is launched from a root wrapper, `micro --env` still works.

### F15. Rust `libc` coverage, `sys` module, toolchain (Q6)
Read from the unpacked crate `libc-0.2.191` this session.
- Present for x86_64 linux-gnu: `SYS_pidfd_open=434`, `SYS_clone3=435`, `SYS_pidfd_getfd=438`, `SYS_io_uring_setup=425`, `SYS_process_vm_readv=310`, `SYS_landlock_create_ruleset: c_long = 444`, `SYS_landlock_add_rule = 445`, `SYS_landlock_restrict_self = 446` (`src/unix/linux_like/linux/gnu/b64/x86_64/not_x32.rs:363-365`). `SYS_pidfd_send_signal`, `SYS_io_uring_enter`, `SYS_process_vm_writev`, `SYS_bpf`, `SYS_kill`, `SYS_tgkill` were found by name across the tree; their x86_64 lines were not printed. `SYS_lsm_list_modules` is **not** in `not_x32.rs`.
- `SO_PEERPIDFD = 77` (`src/new/linux_uapi/asm/socket.rs:169`). This agrees with `uapi_compat.h:98`: `#define AOK_SO_PEERPIDFD 77`.
- `PIDFD_GET_INFO`, `PIDFD_INFO_CGROUPID (1<<2)` and `struct pidfd_info` (`#[non_exhaustive]`, fields through `exit_code`; **no** `coredump_mask`/`__spare1`) in `src/new/linux_uapi/linux/pidfd.rs`, with `PIDFD_GET_INFO = _IOWR::<pidfd_info>(0xFF, 11)`. The repo's own layout is `uapi_compat.h:76-93` (`struct aok_pidfd_info` ending `__s32 exit_code; __u32 coredump_mask; __u32 __spare1;`) with `#define AOK_PIDFD_GET_INFO _IOWR(AOK_PIDFS_IOCTL_MAGIC, 11, struct aok_pidfd_info)` (line 96) and `AOK_PIDFD_INFO_CGROUPID (1ULL << 2)` (line 73). Because the ioctl number encodes the buffer size, a smaller Rust struct produces a different ioctl number. The kernel accepts any size at or above `PIDFD_INFO_SIZE_VER0` (`if (usize < PIDFD_INFO_SIZE_VER0) return -EINVAL;` fs/pidfs.c, and `extensible_ioctl_valid(cmd, PIDFD_GET_INFO, PIDFD_INFO_SIZE_VER0)`), so the libc struct would work with its own derived number. **Hand-define the full 64-byte-class struct and the ioctl number in `sys` to match the C harness**, so both sides use one layout (the C struct on x86_64: 2 x u64 + 11 x u32 + s32 + u32 + u32 = 72 bytes; sizeof is *(interpretation)* from that field list). The kernel has `BUILD_BUG_ON(sizeof(struct pidfd_info) != PIDFD_INFO_SIZE_VER3)` on master; later fields were added after the repo copy was made from v6.18, which the extensible-size rule tolerates.
- `CLONE_NEWCGROUP = 0x02000000`, `CLONE_PIDFD = 0x1000`, `CLONE_INTO_CGROUP = 0x200000000` (`linux/gnu/mod.rs:780`) typed `c_int`. A `c_int` cannot hold a 33-bit value; using the libc constant truncates silently or fails to compile. **Hand-define `CLONE_INTO_CGROUP: u64 = 0x2_0000_0000`.**
- `struct clone_args` exists for x86_64 gnu (`src/unix/linux_like/linux/gnu/b64/x86_64/mod.rs:236`) and musl; it has a `cgroup: c_ulonglong` field after `set_tid_size` [VERIFIED].
- Not in libc: Landlock structs and constants (grep for `LANDLOCK_` in `src/` found nothing). Hand-define the three structs and constants as in `uapi_compat.h:15-46`. io_uring SQE/CQE and params structs are likewise absent from libc.
- Present (found by name): `SCM_RIGHTS` with the `CMSG_*` helpers, `PTRACE_ATTACH`, `PTRACE_SEIZE`, `PR_SET_NO_NEW_PRIVS`, `SO_PEERCRED`, `SOCK_SEQPACKET`.
- libc's `pidfd` module path: `lib.rs:235` is `pub use new::*;` and `new/mod.rs` does `pub(crate) use linux_uapi::*;` for Linux, so the new-style items are re-exported at the crate root (`libc::PIDFD_GET_INFO`). The exact final public names were not compiled; compile a three-line probe in Wave 0 (A6).
- Toolchain: the host default is nightly 1.100.0; `rustup run stable rustc --version` printed `rustc 1.98.0 (88d9e12ae 2026-08-18)`. CONTEXT.md D-13 says stable 1.99.1 is installed; earlier in this session installed toolchains were listed as 1.94.0, 1.97.1, 1.99.1 and `nightly-2026-07-21`. The `stable` alias and the 1.99.1 note disagree. Pin an **exact version** in `rust-toolchain.toml` (`channel = "1.99.1"`), not `stable`, and run `rustup toolchain list` on the reference host first. libc's MSRV is 1.65, so any of these works. Edition: pin 2024 (needs Rust 1.85 or later); under 2024 `unsafe_op_in_unsafe_fn` is a default warning, which suits one `sys` module. Edition choice is ASSUMED (A7); edition 2021 also builds libc 0.2.191.
- Safety pattern: `#![deny(unsafe_code)]` at the crate root and `#[allow(unsafe_code)] mod sys;`, every `unsafe` block inside `sys` with a `// SAFETY:` comment. Same discipline as the kernel crate without joining its ledger.

### F16. `SO_PEERPIDFD`, `PIDFD_GET_INFO`, the pid-reuse race (Q8)
- Minimum kernels: `SO_PEERPIDFD` 6.5 and `PIDFD_GET_INFO` with `PIDFD_INFO_CGROUPID` 6.13 are carried from the repo's inputs and my memory (A3). Observed `ok` for both on 6.18.33.2 (`env.txt:27-28`: `so_peerpidfd=ok`, `pidfd_get_info=ok`). Ubuntu 7.0 is newer than both. `pidfd_open` appeared in Linux 5.3 and `pidfd_getfd` in 5.6 [CITED: man7.org pidfd_open(2), pidfd_getfd(2)].
- `SO_PEERPIDFD` hands back a pidfd for the **peer recorded on the socket**, taken under `sk_peer_lock` with `get_pid(sk->sk_peer_pid[type])`, for `PIDTYPE_TGID` [VERIFIED: net/core/sock.c SO_PEERPIDFD getsockopt path; line numbers not recorded]. The unix man page: "The returned credentials are those that were in effect at the time of the call to connect(2), listen(2), or socketpair(2)." [CITED: man7.org unix(7), on SO_PEERCRED; the page has no SO_PEERPIDFD text]. Consequences:
  - Passing the connected fd to another process (D-15 (ii)) leaves the identity as the original connector. Only the kernel-recorded process counts, not the sender.
  - Pid reuse: the pidfd references `struct pid`, not a number, so a recycled number cannot be returned. For AF_UNIX the code sets `PIDFD_STALE`, which returns a pidfd even when the peer has exited [VERIFIED: sock.c same function; line numbers not recorded]. `SO_PEERPIDFD_THREAD` exists on master; avoid both in portable code.
- `PIDFD_GET_INFO` reports `cgroupid` as the task's **current** default-hierarchy cgroup for a live task (`task_dfl_cgroup(task)`), and as the cgroup recorded at exit when `PIDFD_INFO_EXIT` data is present [VERIFIED: fs/pidfs.c]. So the identity is read at request time, not connect time. A peer that migrates between connect and request is stamped with its new cgroup: route (i) is therefore an identity route by construction, and a broker must read the cgroup id **per request**.
- Remaining race: read-then-act. Between reading the cgroup id and acting on the request the peer can migrate. No pidfd feature closes that; note it as a limit of the stub.
- PIDFD_GET_INFO restricts lookups to the caller's pid namespace hierarchy (`-EREMOTE`); irrelevant on one host with no pid namespaces [VERIFIED].

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Loading and pinning BPF objects | A C or Rust loader around `bpf(2)` | `bpftool prog loadall ... autoattach pinmaps` | The harness must stay gcc/glibc only (D-07); bpftool does link pinning and the lsm_cgroup fallback. |
| Reading a pinned counter | Custom reader | `bpftool map dump pinned <path>` plus a small parse | JSON output (`-j`) available. |
| Peer cgroup identity | `/proc/<pid>/cgroup` parse, or `SO_PEERCRED` pid | `SO_PEERPIDFD` + `PIDFD_GET_INFO` | Numeric pid is racy; this is also what `broker.c` does. |
| Statistics (median, spread, stability) | New aggregation | `summarize.py` | Already produces the stable mark and `within_noise`. `r1.py` only adds R1. |
| Environment parsing for `env.txt` | New format | `kv` helper in `run.sh` | D-05. |
| Error codes in the Rust suite | Magic numbers | `std::io::Error::raw_os_error()` and `libc::E*` | Every report row records an errno. |

**Key insight:** every piece that can lie in this phase (attach success, "route works", spreads) already has an oracle: a counter, a non-agent baseline, the stable mark. Do not add a new way to decide.

## Common Pitfalls

### Pitfall 1: attach succeeds, hook never runs
**What goes wrong:** the probe passes on a kernel without `bpf` active; all "BPF-LSM" rows measure nothing.
**Why:** Ubuntu's own cover letter describes this exact behaviour on unpatched kernels (F1).
**Avoid:** D-04 plus a counter that must be non-zero after a known open; D-09's leg-invalid rule.
**Warning signs:** counter delta 0; `cat /sys/kernel/security/lsm` lacks `bpf`; no "LSM support for eBPF active" in dmesg.

### Pitfall 2: only the first program of a multi-program object loads
`bpftool prog load` loads one program (F7). The lsm_cgroup test then silently covers only `file_open`. Use `loadall`.

### Pitfall 3: the timed process is outside the attached cgroup
Launching through `systemd-run --user` puts `micro` under `user@UID.service`. For micro this does not happen (Pattern 3), but anyone who "improves" the wrapper to use `run_pinned` breaks the per-cgroup rows. Also, a wrapper that attaches then forgets to move itself measures the sibling case and calls it the hit case.

### Pitfall 4: Yama makes ptrace routes invalid
Ubuntu's default `kernel.yama.ptrace_scope` is ASSUMED to be 1 (A5): "a process must have a predefined relationship with the inferior it wants to call PTRACE_ATTACH on. By default, that means only descendants." [CITED: docs.kernel.org Yama]. `process_vm_*`, `/proc/pid/mem` reads for ptrace modes and `pidfd_getfd` use the same attach check. With scope 1 and sibling agents, the non-agent baseline fails and D-16 marks five routes invalid. Choose and record: (a) set `ptrace_scope=0` for the S5 run via the wrapper, restore after, and record it in `env.txt`; (b) additionally run the default scope and report which routes Yama already closes. The honest reading of S5 for ptrace is "BPF-LSM adds protection beyond Yama scope 1 only for non-descendants that Yama would already refuse". Ask the user which they want at the gate; (a) is what ADV-02 needs to show "the test reaches its target".

### Pitfall 5: `cgroup.kill` is not a `task_kill` route
Counting a `task_kill` deny counter for that route is wrong; the success path bypasses the hook (F10). Record the file-hook counter or a per-route counter instead.

### Pitfall 6: the suite kills the user's session
`cgroup.kill`, `kill(-1, ...)`, `kill(0, ...)` or a wrong pidfd can hit the login session or the desktop. Assert before every destructive write that the path is under the suite-created cgroup and the pid is one the suite spawned and pidfd-opened itself. Run the S5 suite from a throwaway cgroup, not the session scope.

### Pitfall 7: pathname bind routes pollute
`unlink`+`bind` on a fixed path collides across runs. Use a per-run directory under the suite's temp dir and clean it. D-14 says "socket path only", so the path is a CLI argument.

### Pitfall 8: lsm_cgroup hit/miss semantics differ on socket hooks
F6. A per-cgroup `socket_sendmsg` row is not comparable to the global row for sockets created elsewhere.

### Pitfall 9: BPF-LSM global programs fire for the harness's own processes
`file_open` fires for `taskset`, `run.sh`, the shell and bpftool. The count is a lower bound for the leg and counts others; the counter test is a necessary condition only (Pattern 4).

### Pitfall 10: libc constant typed too small
`CLONE_INTO_CGROUP` (F15). Compile-time failure at best, silently wrong flags at worst.

### Pitfall 11: BitLocker and boot-order surprises (Q5)
See Environment Availability, "Dual-boot hazards".

## Code Examples

### Global program with a counter and a cgroup-keyed policy (shape only, not compiled)
```c
// Source: adapted from linux-baseline.md section 8 (bpflsm.bpf.c); additions are counter + acting on the value.
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
char LICENSE[] SEC("license") = "GPL";

struct { __uint(type, BPF_MAP_TYPE_HASH); __uint(max_entries, 1024);
         __type(key, __u64); __type(value, __u32); } cg_policy SEC(".maps");   // 0 allow, 1 deny
struct { __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY); __uint(max_entries, 8);
         __type(key, __u32); __type(value, __u64); } hook_count SEC(".maps");

static __always_inline int check(__u32 slot)
{
	__u64 id = bpf_get_current_cgroup_id();
	__u64 *c = bpf_map_lookup_elem(&hook_count, &slot);
	if (c) *c += 1;                                  // plain add on a per-CPU slot
	__u32 *v = bpf_map_lookup_elem(&cg_policy, &id);
	return (v && *v) ? -13 /* -EACCES */ : 0;
}
SEC("lsm/file_open")
int BPF_PROG(h_file_open, struct file *file) { return check(0); }
```
Policy for the denial probe: key = probe cgroup id, value 1. Timed cgroup id: value 0 for the "hit" variant, absent for the "miss" variant. The probe then runs in the probe cgroup and must see `EACCES`; the timed cgroup never denies. `bpf_get_current_cgroup_id()` equals the inode number of the cgroup directory on 64-bit kernfs (ASSUMED, A10); read it from the program instead of computing it: have the wrapper `stat -c %i`, then confirm with a counter-by-id test.

### lsm_cgroup deny with a chosen errno
```c
// Source: kernel/bpf/cgroup.c:134-138 (return convention), selftest progs/lsm_cgroup.c (shape)
SEC("lsm_cgroup/file_open")
int BPF_PROG(c_file_open, struct file *file)
{
	bpf_set_retval(-13);   // EACCES; without this a 0 return becomes -EPERM
	return 0;              // 0 = deny, 1 = allow
}
```

### Wrapper skeleton
```bash
# Source: composition of Patterns 1-3. Not run; root required.
set -eu
bpftool prog loadall bpflsm.bpf.o /sys/fs/bpf/aok/g autoattach pinmaps /sys/fs/bpf/aok/gm
echo $$ > /sys/fs/cgroup/aok-phase8/timed/cgroup.procs
setpriv --reuid="$TEST_UID" --regid="$TEST_GID" --init-groups \
  tools/linux-baseline/run.sh --run-id "$RID" --suites micro --configs C0,C3 --rounds 1-5
setpriv --reuid="$TEST_UID" --regid="$TEST_GID" --init-groups \
  tools/linux-baseline/run.sh --run-id "$RID" --suites micro --configs C3 --rounds 1-5 --tag bpflsm-file_open-miss
```
The first invocation lays down C0 and C3; the second must not repeat C3 untagged (`run.sh` exits 3 on a repeated (round, label)). Flag spelling follows `run.sh` lines 4-5 and 36 as read; verify against `run.sh --help` when writing the plan. Note: untagged C3 in this run has the BPF programs loaded and is not the clean C3. Either run the clean C0/C3 first with programs absent (same `--run-id`, earlier), or put clean rows in their own run directory and note that `baseline_of` then lacks a C0. Decide in the plan.

### Rust `sys` module shape
```rust
// Source: pattern; libc =0.2.191
#![deny(unsafe_code)]
#[allow(unsafe_code)]
mod sys {
    pub const CLONE_INTO_CGROUP: u64 = 0x2_0000_0000;      // libc types its own as c_int
    pub const AOK_PIDFD_INFO_CGROUPID: u64 = 1 << 2;
    #[repr(C)] pub struct PidfdInfo { pub mask: u64, pub cgroupid: u64, pub pid: u32, pub tgid: u32,
        pub ppid: u32, pub ruid: u32, pub rgid: u32, pub euid: u32, pub egid: u32, pub suid: u32,
        pub sgid: u32, pub fsuid: u32, pub fsgid: u32, pub exit_code: i32, pub coredump_mask: u32, pub spare1: u32 }
    pub fn peer_cgroup(sock_fd: i32) -> std::io::Result<u64> {
        // SO_PEERPIDFD = 77; then ioctl(pidfd, PIDFD_GET_INFO, &mut PidfdInfo{mask: CGROUPID, ..})
        unimplemented!()
    }
}
```
Field order matches `uapi_compat.h:76-93`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Per-task LSM only | `BPF_LSM_CGROUP` per-cgroup LSM programs | Linux 6.0 | Floor fallback for S2 (ADR 0004 decision 2). Version is memory (A2). |
| `SO_PEERCRED` pid | `SO_PEERPIDFD` | 6.5 | Race-free peer identity. |
| `/proc/<pid>/cgroup` | `PIDFD_GET_INFO` cgroup id | 6.13 | One ioctl, no path parse. |
| Ubuntu without `bpf` in `CONFIG_LSM` | `bpf` appended on HWE 7.0.0-42 | 2026-10-28 (Updates) | Makes Ubuntu a reachable BPF-LSM distro for DEC-03; not yet verified by Ubuntu. |

**Deprecated/outdated:** the `kernelCommandLine`/`.wslconfig` route in linux-baseline.md section 8 (dead, D-01); `bpftool prog load` for multi-program objects (F7).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Kernel master source read on 2026-10-28 matches Ubuntu 7.0.0-42 for bpf_lsm.c, kernel/bpf/cgroup.c, fs/pidfs.c, net/core/sock.c behaviours cited | All Findings | A hook is disabled or a return convention differs; caught by the first load. |
| A2 | `BPF_LSM_CGROUP` arrived in Linux 6.0 | State of the Art | None for the plan; 7.0 is far past. |
| A3 | `SO_PEERPIDFD` 6.5 and `PIDFD_GET_INFO` 6.13 minimums | F16 | Floor-row wording in DEC-03. Observed present on 6.18 and Ubuntu 7.0 is newer. |
| A4 | `lsm_cgroup` attached at a parent applies to descendant leaves | F6 | Per-cgroup row design; plan must attach at the leaf or test inheritance. |
| A5 | Ubuntu default `kernel.yama.ptrace_scope` is 1 | Pitfall 4 | If 0, no ptrace route is Yama-invalid; if 1, five routes are. Probe it and record. |
| A6 | `libc::PIDFD_GET_INFO` and friends compile at crate root in 0.2.191 for x86_64-linux-gnu | F15 | Hand-define in `sys` instead; cheap. |
| A7 | Edition 2024 for the suite | F15 | Edition 2021 also works. |
| A8 | BTF direct dereference of `task->cgroups->dfl_cgrp->kn->id` works in `task_kill` and `ptrace_access_check` programs under the host's lockdown level | F10 | ADV-03 deny rule needs another way to read the target's cgroup. |
| A9 | `/sys/kernel/security/lockdown` and `kernel.apparmor_restrict_unprivileged_userns` names | F13 | `env.txt` key blank; trivial. |
| A10 | `bpf_get_current_cgroup_id()` equals the cgroup directory inode on 64-bit | Code Examples | Policy map populated with the wrong key; the counter-by-id check catches it. |
| A11 | `send_sig(SIGKILL, task, 0)` skips `check_kill_permission`, so `cgroup.kill` bypasses `task_kill` | F10, Pitfall 5 | If it does call the hook, the route is mediated and the deny counter is a `task_kill` counter. |
| A12 | All hook-to-route mappings in F10 (ptrace modes, `pidfd_getfd`, `/proc/pid/mem`, `unlink`, `bind`, `file_receive`) | F10 | A route is unmediated by the hook the plan expects; the agent-run result will show it. |
| A13 | `socket_sendmsg` fires for `send()` as well as `sendmsg()` (the peer in `unix_dgram_pingpong` uses `send`, micro.c:435; the client `sendmsg`, micro.c:245) | F9 | Counter delta smaller than the call count; leg wrongly invalid. |
| A14 | `bpf` coexists with `apparmor` in `lsm=` | F3 | Boot without BPF-LSM; the dmesg line and `/sys/kernel/security/lsm` show it. |
| A15 | Fresh install of the newest 24.04 point ISO does not boot 7.0 until `linux-generic-hwe-24.04` is installed | Installation | Extra reboot. |
| A16 | `bpftool` creates `/sys/fs/bpf` mounts and dirs on demand | F2 | Wrapper must `mount -t bpf` first. |
| A17 | Dual-boot general facts (Fast Startup, hibernation, shrink, Secure Boot signed shim, BitLocker auto-enable on Windows 11 24H2) | Environment Availability | See that section. |

## Open Questions

1. **Does the reference run use `ptrace_scope=0`?**
   - Known: Yama scope 1 invalidates five ptrace routes for sibling agents (A5).
   - Unclear: whether the user wants S5 read against Ubuntu's default or against the classic setting.
   - Recommendation: run S5 with `ptrace_scope=0` for ADV-02/03, record it in `env.txt`, and report the default-scope result as a second column. Put the question on the gate sheet.
2. **Does HARN-01 get a no-`bpf` boot entry?**
   - Known: the WSL2 baseline had no `bpf` active; the reference kernel must (F12).
   - Unclear: whether one extra GRUB entry is acceptable.
   - Recommendation: take it; otherwise label the HARN-01 table "includes `bpf` stub cost".
3. **Is the dual-boot on the 7900X3D machine?** D-12 says ASSUMED. The probe records `cpu_model`; the transfer table's headline sentence depends on it.
4. **Ubuntu verification of LP #2166235.** If `bpf` is dropped from a later build, `lsm=` is the fallback. Pin the exact `linux-image` version in `env.txt` and in the decision record.
5. **Does `lsm_cgroup` inherit into descendants?** (A4). Cheap to test on the host; decide the attach point after.
6. **Clean C3 vs C3-with-programs in one run directory.** See the wrapper skeleton note.
7. **`socket_sendmsg` and the lsm_cgroup per-cgroup variant.** Is the per-cgroup row for this hook wanted at all given F6? If not, say so on the report (D-11 lists the per-cgroup config without naming hooks).

## Environment Availability

Probed on the session host (WSL2, no root) on 2026-10-28. The reference host does not exist until 2026-11-04.

| Dependency | Required By | Available (this host) | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Root on the reference host | ENV-01 | n/a | — | None. Without it the phase cannot finish (roadmap "wrong thing" entry). |
| `bpf` in `/sys/kernel/security/lsm` | ENV-01 | not here | — | `lsm=...,bpf` on the kernel command line (F1) |
| `bpftool` | ENV-01, HARN-02/03 | no | — | Install `linux-tools-generic-hwe-24.04` on the reference host |
| clang with BPF target | ENV-01 | yes | Ubuntu clang 18.1.3, targets include `bpf`, `bpfel`, `bpfeb` | — |
| `libbpf-dev` headers | ENV-01 | not checked here | 1:1.3.0-2build2 in noble | `apt install libbpf-dev` |
| `/sys/kernel/btf/vmlinux` | D-04 | not checked here | `CONFIG_DEBUG_INFO_BTF=y` in the -42 annotations | — |
| `/sys/fs/bpf` mounted | pins | no (`findmnt` printed nothing) | — | `mount -t bpf bpf /sys/fs/bpf` |
| gcc | harness | yes | 13.3.0 | — |
| Python | `r1.py`, checks | yes | 3.12.3 | — |
| Rust | suite | yes | default nightly 1.100.0, `stable` = 1.98.0, 1.99.1 installed per CONTEXT | Pin an exact version per crate |
| cgroup v2 delegated subtree | ENV-01, HARN-03, ADV | baseline run found one on WSL2 | — | Root-created cgroups on the reference host |
| `/dev/kvm` | not needed in this phase | absent on WSL2 | — | — |

**Missing with no fallback:** root and the reference host itself.
**Missing with fallback:** `bpf` in the LSM list (command line), `bpftool` (package), `/sys/fs/bpf` (mount).

### Dual-boot hazards before 2026-11-04 (Q5)
General facts, not claims about the user's machine. Sources: Microsoft Learn "BitLocker recovery overview" (updated 2026-10-08) [CITED], plus common dual-boot guides [ASSUMED, A17].
- BitLocker recovery is triggered by, verbatim: "Changes to the NTFS partition table on the disk", "Changes to the boot manager", "Modifying the Platform Configuration Registers (PCRs) used by the TPM validation profile", "Upgrading critical early startup components, such as a BIOS or UEFI firmware upgrade" [CITED]. Shrinking the Windows volume and installing a second boot manager can hit the first two. For planned changes Microsoft advises "temporarily suspending BitLocker protection"; a suspension resumes at the next reboot unless a reboot count is set with PowerShell or `manage-bde` [CITED]. Locate the recovery key (Microsoft account, Entra ID or AD) before touching the disk.
- Disable Fast Startup and hibernation (`powercfg /h off`) before shrinking, shrink in Windows Disk Management, run `chkdsk` [ASSUMED].
- Windows 11 24H2 may enable device encryption automatically, including on Home; Ubuntu's guided "install alongside" cannot resize an encrypted partition [ASSUMED].
- Secure Boot works with Ubuntu's signed shim [ASSUMED]; it also may turn on kernel lockdown, which matters for A8.
- A Windows update can move Windows Boot Manager to the top of the boot order; fix in firmware boot menu or with `update-grub` [ASSUMED].
- Machine unknowns the plan must probe on 2026-11-04, not assume: Secure Boot state; BitLocker state and where the recovery key is; disk layout, ESP size and free space after shrink; firmware boot order; whether the machine is managed or domain joined; CPU model (decides D-12).
- Time budget: the install is one week out. A BitLocker recovery lockout is the likeliest single cause of missing the date; a recovery-key check is a zero-cost first step.

## Validation Architecture

`workflow.nyquist_validation` is true in `.planning/config.json`.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | No framework for the bash/BPF parts. Rust `cargo test` (pinned toolchain) for the suite's pure logic; `python3 -I` scripts for `r1.py` and the report check. Existing: `tools/linux-baseline/check_report.py` (exists, not read this session) |
| Config file | none yet; `tools/aok-adv/rust-toolchain.toml` and `Cargo.toml` are Wave 0 |
| Quick run command | `cd tools/aok-adv && cargo test` ; `python3 -I tools/linux-baseline/r1.py --selftest` |
| Full suite command | on the reference host: wrapper run for each variant, then `python3 -I tools/linux-baseline/check_report.py <run dir>` |

### Phase Requirements to Test Map
"Host" column: **R** needs the reference host (does not exist until 2026-11-04); **L** can run on the WSL2 session host.

| Req ID | Observable | Command / file | Failure looks like | Host |
|--------|------------|----------------|--------------------|------|
| ENV-01 | `bpf` in LSM list; uid 0; bpftool version; hook fires; program loaded and unloaded | probe script writes `env.txt` keys `bpf_in_lsm`, `lsm_hook_fires`, `bpftool_version`, `attach_global_file_open` | `bpf_in_lsm=no`; `lsm_hook_fires` count 0; bpftool error text | R |
| ENV-02 | Env file names kernel, cmdline, LSMs, Landlock ABI, one `floor_*` key per section 3 row | `grep -c '^floor_' env.txt` equals the row count in scoping-linux.md section 3; keys of F13 present | missing key; a floor row absent from the file | R (the key-writing code is testable L) |
| HARN-01 | New run dir; side-by-side table with stability marks | `results/<new run>/summary.csv`; table in the report; script comparing C3-vs-C0 added % for both runs | any S3/S4/S6 leg missing a transfer verdict; unstable rows read without the mark | R |
| HARN-02 | Rows per (hook variant x leg) with N, p50, p99, spreads, stable; counter delta >= call count; R1 numbers | `results/<run>/summary.csv`, wrapper log with counter before/after, `r1.py` output | counter delta below call count (leg invalid); missing rows; R1 table lacks BPF share | R |
| HARN-02 (script logic) | `r1.py` reproduces the linux-baseline.md section 11 numbers from run `20261028T034700Z` | `python3 -I r1.py tools/linux-baseline/results/20261028T034700Z/summary.csv` must give `openat_tmpfs` p50 45 (the worked example, linux-baseline.md section 11) and match every other R1 figure printed in that section | any of those differs | **L** |
| HARN-03 | `lsm_cgroup` attach result; `openat` errno in attached cgroup; success in sibling; return-1 observed | wrapper log: attach exit code, errno, sibling result; per-cgroup rows | attach error text recorded (valid finding, then fallback measured); EPERM vs expected errno mismatch; sibling also denied | R |
| ADV-01 | Per class: valid flag from the non-agent run | suite report JSON, one object per case with `class`, `oracle`, `baseline_ok` | `baseline_ok=false` marks invalid; fewer than one valid case in a class | R (also L for classes needing no cgroup) |
| ADV-02 | Per route: non-agent success | suite report per route | route invalid; stub fails to start; stub restart not performed between kills | R (cgroup routes), L (kill, bind, pidfd routes may run on WSL2 if the stub runs) |
| ADV-03 | Count of successful agent-run routes, per route, with deny counter delta | suite report plus counters | count above 0 (a trigger (c) candidate, not a phase error); a success with a zero counter for its supposed hook | R |
| DEC-01 | 10 Status cells dated in scoping-linux.md section 4; ADR 0004 status line | `grep` for the date in each of S1-S9 and POSX-02 rows | a cell still reads "proposed"; S7/S8 without a deferred-to phase | L (after user marks) |
| DEC-02 | Decision names option, resulting S3 row, run dirs | gate block text | no run directory named; mismatch with D-18 | L |
| DEC-03 | "proposed" labels updated in section 3 from marks | `grep -n proposed docs/architecture/scoping-linux.md` | labels remain | L |

### Sampling Rate
- **Per task commit:** `cargo test` in the crate; `python3 -I r1.py --selftest`; `bash -n` on the wrapper.
- **Per wave merge:** the same, plus `python3 -I tools/wikigraph.py wiki` is not relevant here; use the repo's own checks only.
- **Phase gate:** reference-host run directories exist and `check_report.py` passes before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `tools/aok-adv/` crate with pinned toolchain and a compile probe for `libc::PIDFD_GET_INFO`, `libc::SO_PEERPIDFD`.
- [ ] `tools/linux-baseline/r1.py` with a self-test that reproduces the section 11 numbers (item L above).
- [ ] BPF sources and wrapper (buildable only where `bpftool btf dump` works, i.e. the reference host).
- [ ] `run.sh write_env` extension and its check.
- [ ] `08-GATE.md` generator or hand template.
- [ ] Decision on how a DEC row is checked by script (`grep`) rather than by eye.

## Security Domain

`security_enforcement` is enabled; `security_asvs_level` is 1.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | The suite authenticates nothing; the stub identifies peers by kernel-stamped cgroup id, which is the subject under test. |
| V3 Session Management | no | — |
| V4 Access Control | yes (subject of the phase) | Kernel mediation under test; the wrapper is the only root component and runs no unprivileged-writable code as root. |
| V5 Input Validation | yes | Stub broker parses attacker-controlled payloads: length-prefix and bounds checks; reject unknown ids; (iii) control must be ignored. Wrapper parses `bpftool` output defensively. |
| V6 Cryptography | no | No crypto. The directly-used API key case uses a dummy key and a local stub endpoint; never a real key. |
| V14 Configuration | yes | bpffs pins and maps root-only (default mode); wrapper restores sysctls (`ptrace_scope`, governor) on exit, including on failure, via a trap. |

### Known Threat Patterns
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Suite signals or kills outside its own processes | Tampering/DoS | Spawn and pidfd-open every target; assert cgroup path prefix before `cgroup.kill`; never `kill(0/-1)` (Pitfall 6) |
| Root wrapper executes repo files writable by the unprivileged uid | Elevation | Wrapper itself and BPF objects are built by root-run steps from checked-in sources; the unprivileged `run.sh` runs only after privilege drop. Low risk on a single-user research box; note it. |
| Sysctl left weakened (`ptrace_scope=0`) | Elevation | Restore in an exit trap; record original value in `env.txt`. |
| Real API key in the "directly used key" case | Information disclosure | Dummy key, local stub endpoint only. |
| Lost BitLocker key during dual-boot | Availability | Locate the recovery key before partitioning (Environment Availability). |
| A BPF program that denies too broadly bricks the host | DoS | Programs key on a test cgroup id, never deny globally; keep a root shell open; unload by `rm` of pins. The `file_open` deny probe in particular must not apply to root's session cgroup. |

## Sources

### Primary (HIGH confidence)
- Linux kernel master as of 2026-10-28 (fetched from the kernel git mirror): `kernel/bpf/bpf_lsm.c`, `kernel/bpf/cgroup.c`, `kernel/bpf/trampoline.c`, `kernel/bpf/verifier.c`, `kernel/bpf/syscall.c`, `include/linux/lsm_hook_defs.h`, `security/bpf/hooks.c`, `security/lockdown/lockdown.c`, `fs/pidfs.c`, `net/core/sock.c`, `kernel/cgroup/cgroup.c`, `tools/bpf/bpftool/{cgroup.c,prog.c}`, `tools/lib/bpf/libbpf.c`, `Documentation/admin-guide/kernel-parameters.txt`, selftests `progs/lsm_cgroup.c` and `prog_tests/lsm_cgroup.c`.
- Ubuntu kernel annotations at tags `Ubuntu-hwe-7.0-7.0.0-42.42_24.04.1` and `...-38.38_24.04.4` (CONFIG_LSM, CONFIG_BPF_LSM, CONFIG_CGROUP_BPF, CONFIG_DEBUG_INFO_BTF).
- Launchpad REST API 2026-10-28: published sources and binaries for `linux-hwe-7.0`, `linux-tools-common`, `libbpf`, `llvm-toolchain`; `dpkg-deb -c` on `linux-tools-7.0.0-42-generic` and `linux-hwe-7.0-tools-7.0.0-42`.
- `libc-0.2.191.crate` unpacked and read; crates.io API (published 2026-10-16, `rust-version` 1.65).
- Repo, read this session: `08-CONTEXT.md`, `.planning/REQUIREMENTS.md` 50-95, `.planning/ROADMAP.md` 202-234, `docs/architecture/research/linux-baseline.md` 146-245, 363-530, 547-590, `tools/linux-baseline/{uapi_compat.h,run.sh (95-150, selected greps),summarize.py (selected greps)}`, `results/20261028T034700Z/env.txt`.
- Local probes: `clang --print-targets`, `clang --version`, `gcc --version`, `python3 --version`, `rustup run stable rustc --version`, `findmnt /sys/fs/bpf`.

### Secondary (MEDIUM confidence)
- docs.kernel.org Yama and cgroup-v2 pages; man7.org `pidfd_open(2)`, `pidfd_getfd(2)`, `unix(7)`, `socket(7)` (no SO_PEERPIDFD text).
- Microsoft Learn, BitLocker recovery overview (updated 2026-10-08).
- Ubuntu kernel-team mailing list, Sept 2026 patches 171489 and 171517 (SRU cover letters, test plan, rationale); Launchpad bug #2166235 page.

### Tertiary (LOW confidence)
- Dual-boot guides for Fast Startup, shrink, auto-BitLocker and boot order (A17).
- Not read: `tools/linux-baseline/{micro.c,broker.c,cg.c,check_report.py}` in this segment (read before the earlier compaction; their facts are not restated here), `scoping-linux.md` section 3 floor rows.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH for versions and package locations (Launchpad, crates.io); MEDIUM for edition and exact toolchain pin.
- Architecture: MEDIUM-HIGH. The wrapper, pinning and cgroup-inheritance patterns follow source reading; none was executed.
- Hook-to-route mapping (F10): LOW-MEDIUM. Signatures are verified; which syscall reaches which hook is from kernel structure and the gap-analysis input.
- Pitfalls: HIGH for items 1, 2, 4 (source or doc backed); MEDIUM for 5 (A11).
- Measurement costs (counter, `bpf_stats`, trampoline, governor, SMT): NOT MEASURED.

**Research date:** 2026-10-28
**Valid until:** 2026-11-04 for the Ubuntu package state (it moves with each SRU respin and with the LP #2166235 verification outcome); 30 days for kernel-source facts.
