---
phase: quick-261027-un5
plan: 01
subsystem: benchmarking
tags: [linux, seccomp, landlock, cgroup-v2, pidfd, baseline]
requires:
  - docs/architecture/scoping-linux.md section 4 (rows S3, S4, S6)
  - docs/architecture/adr/0004-linux-prototype-first.md
provides:
  - tools/linux-baseline/ harness (micro, broker, deadline, cgbench; run.sh; summarize.py; check_report.py)
  - one measured run, tools/linux-baseline/results/20261028T034700Z
  - docs/architecture/research/linux-baseline.md
affects: []
key-files:
  created:
    - tools/linux-baseline/ (Makefile, common.h, uapi_compat.h, sandbox.[ch], micro.c, broker.c, cg.[ch], deadline.c, cgbench.c, run.sh, summarize.py, check_report.py, .gitignore)
    - tools/linux-baseline/results/20261028T034700Z/
    - docs/architecture/research/linux-baseline.md
  modified: []
decisions:
  - "One measured run, no reruns; every table number in the report is copied from the run's summary.csv by script"
  - "Section 11 eligibility also requires the quantile spread under 10 percent, so S4 and S6 derive no revision"
metrics:
  duration: not recorded (single context, compacted once)
  completed: 2026-10-28
status: complete
commits: 4
plan_head_before: 05a5300150ac410c1ad8db4626cf4cc1c8376f53
plan_head_after: e40794e9d31428199dbbb198d0c9874783d630b4
actuals:
  tokens: 39100
  tasks: 3
  commits: 4
---

# Phase quick-261027-un5 Plan 01: Linux baseline benchmark harness and report Summary

A gcc-only harness that measures seccomp, Landlock, broker IPC, deadline kill and cgroup spawn
costs on this WSL2 host, one committed run of it (stable at p50 on the micro rows, noise-dominated on
11 of 21 deadline rows), and a report with a proposed-revision table for S3, S4 and S6 (L1, L2).

`actuals.tokens` is chars/4 over the 16 committed non-results files (156519 characters); the
generated run directory is excluded. `commits` comes from `git rev-list --count` against the plan
base because the per-plan ledger file was never created in the worktree git dir. The base is the
orchestrator-supplied 05a5300, also recorded in `.work/base-rev`.

## Commits (subject lines only, SSH-signed)

| Task | Commit | Subject |
|---|---|---|
| 1 | c43d46a | Add Linux baseline harness skeleton |
| 2 | 90b8c33 | Add remaining Linux baseline benchmark suites |
| 3a | 7660500 | Add Linux baseline results for WSL2 host |
| 3b | e40794e | Add Linux baseline report |

## Measured run

- Run id: `20261028T034700Z`, harness revision 90b8c332d6f7c1a3c735568e0a0a31e7de5e48a9,
  `harness_dirty=no`. One attempt; no failed or discarded run (L13). Smoke runs used `smoke-*`
  ids, are gitignored and feed nothing.
- Suite times (env.txt `elapsed_s`): micro 200 s, broker 18 s, deadline 85 s, cgroup 8 s. Each ran
  as a separate foreground command with nothing else running. Load average before micro was
  0.09 0.23 0.28; no process held more than 5.5 percent of a CPU beforehand.
- `summarize.py --check` printed CHECK-OK. selfcheck.txt has 106 lines, all `result=ok`; C3 passes
  all five negative checks; C1c has 65 instructions against C1b's 64.
- Stability (L12): C0 micro p50 spread under 10 percent on all ten benches (largest 9.64,
  `openat_tmpfs`). C0 `getppid` 130.000 ns and `openat_tmpfs` 1763.000 ns are inside the ASSUMED
  windows. C0 p99 spreads reach 22.14 percent on `getppid`. Deadline suite: 7 stable, 3 unstable,
  11 noise-dominated of 21 rows.
- NOT MEASURED, as required by L11: BPF-LSM per-hook overhead, BPF_LSM_CGROUP, S7 inference
  queue, S8 PSI trigger to freeze. Reasons: `bpf` is absent from the active LSM list, no
  privilege to load programs (`unprivileged_bpf_disabled=2`), no inference backend exists, and the
  freeze leg needs a pressure generator. Root commands are in report section 8.
- No cgroup leg was NOT MEASURED: the probe found `delegation=yes` (systemd-run service, uid 1000,
  controllers `cpu memory pids`). `CLONE_INTO_CGROUP` works; OOM kill 3 of 3; `pids.max=8` refused
  the eighth fork 3 of 3; PSI trigger registration ok.

## Planner additions A1-A8 (all kept; each can be dropped)

- A1 C1c (C1b plus `args[0]` load): kept. C1c adds 54.744 ns on `getppid_x1000` against 40.447 ns
  for C1b and 40.466 ns for C1a. Report section 4 explains the seccomp action cache and marks the
  link to the data *(interpretation)*.
- A2 `getppid_x1000`: kept. C0 120.628 ns.
- A3 `SO_LINGER {1,0}` and untimed drain: kept.
- A4 self-checks: kept; they gate every number.
- A5 `--suites` and `--rounds`: kept. Micro fit in one `--rounds 1-5` invocation (200 s).
- A6 `cgkill_*` deadline configs: kept; the probe was delegated so they ran.
- A7 PSI registration probe: kept; `psi_trigger_register=ok`.
- A8 `lsm_list_modules`: kept; it makes the missing `bpf` LSM a VERIFIED fact.

## Deviations from Plan

**1. [Rule 1 - Bug] Landlock signal scope blocks PR_SET_PDEATHSIG (threat T-261027-un5-03)**
- **Found during:** Task 2 smoke runs.
- **Issue:** with the signal scope active, the kernel does not deliver PDEATHSIG from a scoped
  parent, so an orphaned `unix_dgram_pingpong` peer survived and held the stdout pipe open; a
  foreground command hung.
- **Fix:** the peer holds a life-pipe read end and a watchdog thread calls `_exit(0)` on EOF.
  Verified: smoke-y exit 0, no
  lingering process, 14 of 14 self-checks ok.
- **Files modified:** tools/linux-baseline/micro.c. **Commit:** 90b8c33.

**2. [Rule 3 - Blocking] 6.8 headers lack `LSM_ID_IMA` and `LSM_ID_EVM`**
- **Fix:** numeric values 111 and 112 in the harness (the A8 listing). **Commit:** 90b8c33.

**3. [Rule 1 - Bug] Stale errno after the `PIDFD_GET_INFO` probe**
- **Fix:** `errno = 0` before the ioctl. **Commit:** 90b8c33.

## Plan versus host disagreements

- The plan's mitigation for helpers (PDEATHSIG on every helper) does not hold under the signal
  scope; see deviation 1.
- The summary.md stability verdict tests p50 spread only, and the planner's R1 and R2 wording says
  "spreads under 10 percent". I tested the spread of the quantile the bound constrains and also
  required the stability column to read stable. Under that reading S3 `openat_*` p99 and
  `posix_spawn` p99 yield no revision, and S4 and S6 yield none. Reading the plan on p50 spread
  alone would have derived S4 values from rows whose p99 spread is 12.63 to 30.00 percent. The user
  can overrule this in report section 11.
- Report section 1 reads "partially stable", not the plan's two-way stable or noise-dominated: the
  micro p50 rows are stable, but p99 columns and most deadline rows are not.
- The unexplained result: `pidfd_L0 deadline_to_wakeup` is 1880643.000 ns at p50 while `pidfd_L1`
  is 9517.000 ns. The report states the cause is not established.
- `env.txt` carries `uname -a`, which includes the hostname `oakwood`. I inspected env.txt and the
  kernel command line (WSL defaults only, no secrets) and committed them as produced; the hostname
  is the only identifying string.

## Threat flags

None beyond the plan's register. No harness file names sudo, pkexec or doas (grep clean). All
restrictions are self-applied under `PR_SET_NO_NEW_PRIVS`.

## Known Stubs

None.

## Cleanup

- Development scratch removed: `/dev/shm/aok-st-manual`, `/dev/shm/aok-bt-manual`,
  `/dev/shm/aok-est`, `.work/rootfs.manual`, `.work/est`.
- After the measured run: no harness process, no transient systemd scope or service (only
  `init.scope` and `dbus.service` remain), nothing under `/dev/shm` from the harness, no leftover
  delegated cgroup directories.
- Left in place, all gitignored: `tools/linux-baseline/bin/`, `.work/base-rev`,
  `.work/strace-micro.txt`, `results/smoke-t1`, `smoke-t2`, `smoke-x`, `smoke-y`.
- Nothing outside `tools/linux-baseline/` and `docs/architecture/research/linux-baseline.md` changed.
  `scoping-linux.md` and ADR 0004 are untouched (L3). No privilege escalation attempted (L4).

## Self-Check: PASSED

- Files present: report, check_report.py, run directory (11 files), harness sources.
- Commits c43d46a, 90b8c33, 7660500, e40794e are ancestors of HEAD; four commits, subject lines only.
- T3 verify steps all passed: one run directory, `harness_dirty=no`, `CHECK-OK`, `REPORT-OK`,
  changed paths limited to the allowed set, no commit bodies, clean status.
