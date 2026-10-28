---
phase: 08-reference-environment-early-falsifiers-and-threshold-gate
plan: 01
subsystem: linux-baseline tooling
tags: [r1, transfer, floor-probe, env-02, harn-01, harn-02]
requires: []
provides:
  - tools/linux-baseline/r1.py (R1 threshold calculator, self-test, bpflsm mode)
  - tools/linux-baseline/transfer.py (per-leg HARN-01 transfer verdict, self-test)
  - tools/linux-baseline/floor.c (12 floor_ keys plus floormethod_ keys plus host keys)
affects: [08-02, 08-03]
tech-stack:
  added: []
  patterns: [importlib sibling load under python3 -I, Decimal arithmetic with explicit rounding]
key-files:
  created:
    - tools/linux-baseline/r1.py
    - tools/linux-baseline/transfer.py
    - tools/linux-baseline/floor.c
  modified:
    - tools/linux-baseline/Makefile
    - tools/linux-baseline/run.sh
    - scripts/check.sh
decisions:
  - "S4 and S6 transfer delta is the percent change of the leg value (interpretation): their bounds are absolute, so a percent-point difference of added percent would carry no meaning"
  - "floor_bpf_lsm_cgroup and floor_io_uring_task_restrictions are method derived from the kernel release string and carry ASSUMED in the value"
  - "Clone-flag bit values for CLONE_PIDFD_AUTOKILL and CLONE_NNP are ASSUMED; the two rows carry '(flag value ASSUMED)'"
requirements-completed: [ENV-02, HARN-01, HARN-02]
status: complete
metrics:
  tasks: 3
  files: 6
actuals:
  tokens: 9700
  tasks: 3
  commits: 3
plan_head_before: cf8766e5ccc6a82660c210827a18b825cf0abd87
plan_head_after: df2d6b2cf81edee0c510e962586d1110f0127105
commits: 3
---

# Phase 8 Plan 01: R1 calculator, transfer verdict, floor probe Summary

Three host-independent measurement tools for phase 8: `r1.py` computes the R1 revised bound with Decimal and ceil, `transfer.py` gives a per-leg HARN-01 verdict between two run directories, and `floor.c` writes the 12 ENV-02 floor rows (each with a probe or derived method key) into `env.txt`. All three were built and checked on the WSL2 session host; none needed the reference host.

## Commits

| Task | Commit | Subject |
|------|--------|---------|
| 1 | 097ee7b | feat(08-01): add r1.py R1 calculator with self-test and check.sh gate |
| 2 | 8cd9ed9 | feat(08-01): add transfer.py per-leg HARN-01 verdict with self-test gate |
| 3 | df2d6b2 | feat(08-01): add floor.c ENV-02 floor-row probe and run.sh env wiring |

## What was built

- `r1.py`: revised bound = max(current bound, C3 added% + 2 x larger spread), rounded up to a whole percent, for bounds p50=10 and p99=25. A row is eligible only when `summarize.stability` marks it stable and the baseline and config spreads on the constrained quantile are strictly below 10; a spread of exactly 10.00 reads `none: unstable on this host`. The `--against C3` added percent comes from the `med_*_ns` Decimal strings, quantized to 0.01 with ROUND_HALF_EVEN. Output is stdout only and informational (D-18): the S3 bound is not revised and nothing is written under docs/. CLI: `--self-test`, `RUN_DIR [--config CFG] [--against C0|C3]`, `--bpflsm BASE VARIANT...` (fewer than 2 directories exits 64, read errors exit 65).
- `transfer.py`: verdicts `transfers`, `does not transfer`, `unreadable (unstable)`. Tolerance is 2 x the largest spread among the compared rows; a delta equal to the tolerance transfers. S3 compares C3 against C0 added percent at p50 and p99 over seven benches; S4 compares broker_id against direct and egress_broker against egress_direct at p99; S6 compares pidfd_L4 and cgkill_L4 deadline_to_reaped at p99. It reuses the eligibility rule from `r1.py` rather than restating it.
- `floor.c`: prints 12 `floor_<row>` lines and exactly one `floormethod_<row>` per row, then nine host keys (`btf_vmlinux`, `yama_ptrace_scope`, `lockdown`, `secure_boot`, `scaling_governor`, `scaling_driver`, `smt_active`, `kernel_pkg`, `apparmor_userns_restrict`), each falling back to `unreadable`. Ten rows are probe (syscall or /proc or /sys read), two are derived from the kernel release string (`bpf_lsm_cgroup`, `io_uring_task_restrictions`). It reads no kernel build-config file.
- `Makefile` builds `bin/floor` with the shared CFLAGS; `run.sh` lists `floor` in the binary-existence loop and appends its output after `micro --env` in `write_env`, so the keys land only under the existing empty-`env.txt` guard. `scripts/check.sh` runs both Python self-tests before `cd kernel`.

## Verified (command run, output seen)

- `python3 -I tools/linux-baseline/r1.py --self-test`: CHECK-OK. On the committed run 20261028T034700Z: openat_tmpfs p50 `R1 45 (44.29)`, openat_rootfs p50 `R1 19 (18.90)`, unix_dgram_pingpong p50 `R1 14 (13.53)`; the other four p50 legs keep 10 (4.16, 3.97, 3.43, 3.30); four p99 legs keep 25 (9.87, 8.81, 11.31, 16.44); three p99 legs read `none: unstable on this host`. The self-test also checks the med-string added percent equals the CSV column for every leg.
- `python3 -I tools/linux-baseline/transfer.py --self-test`: CHECK-OK. Committed run against itself: S3 p50 7 transfer; S3 p99 4 transfer and 3 unreadable (openat_tmpfs, openat_rootfs, posix_spawn); S4 and S6 all unreadable. Boundary: delta 2.00 against tolerance 2.00 transfers, 12.01 against 2.00 does not; spread 10.00 is unreadable.
- `sh scripts/check.sh --no-kvm` ran twice (after Task 1 and after Task 2) and ended with "check: static gates passed; LOCAL-ONLY (needs /dev/kvm): cargo osdk test, cargo osdk run boot checks". It was not re-run after Task 3, which touched no file the script reads beyond the already-built harness.
- `make -C tools/linux-baseline` builds `bin/floor` under `-Wall -Wextra -Werror`; `bash -n tools/linux-baseline/run.sh` passes.
- `bash tools/linux-baseline/run.sh --run-id smoke-floor --smoke --suites micro` exited 0 on WSL2. In its `env.txt`: `grep -c '^floor_'` = 12; `grep -cE '^floormethod_[a-z0-9_]+=(probe|derived)$'` = 12; `grep -cE '^floormethod_[a-z0-9_]+=derived$'` = 2. `grep -c -e /boot/config -e config.gz floor.c` = 0.
- Observed floor values on this host (WSL2, kernel 6.18.33.2-microsoft-standard-WSL2): clone_into_cgroup present, clone_pidfd_autokill absent, clone_nnp absent, pidfd_get_info_cgroupid present, landlock_abi 7, config_bpf_lsm present (bpf_lsm_ symbols in kallsyms), bpf_default_lsm_order unreadable, memcg present, psi_per_cgroup present, dmem absent. These are probe results on a WSL2 kernel, not reference-host results.

## Not verified

- Not run: needs reference host. Every R-labelled step of the phase, including any `floor.c` result on the reference kernel and the root `bpflsm.sh --probe` that rewrites `floor_bpf_lsm_cgroup` to method probe (08-03). No R result was attempted or fabricated.
- Kernel behaviour behind the probes is ASSUMED (08-RESEARCH.md A1-A17), not measured: that clone3 with an invalid cgroup fd returns EBADF when a flag is known and EINVAL when it is not, and that the cgroup fd check is reached after flag validation. The clone3 probe distinguished present from absent only for `CLONE_INTO_CGROUP` on this host; the `absent` results for autokill and NNP could also mean the assumed bit values are wrong. Both rows say so in their value.
- The clone-flag bit values (`1ULL<<34` for CLONE_PIDFD_AUTOKILL, `1ULL<<35` for CLONE_NNP) were not read from a v7.1 tree. Read `include/uapi/linux/sched.h` and `kernel/fork.c` at v7.1 on the reference host before trusting those two rows.
- `floor_bpf_lsm_cgroup` reads `present ASSUMED` on WSL2 purely from kernel >= 6.0; the BPF LSM is not active here, so the value does not describe attachability. `floor_io_uring_task_restrictions` reads `absent ASSUMED` from kernel < 7.0.
- `floor_bpf_default_lsm_order` is `unreadable` here (securityfs not readable), `lockdown`, `secure_boot`, `scaling_*` and `apparmor_userns_restrict` are `unreadable` on WSL2. The yes/no branch of `bpf_default_lsm_order` and the `lsm_override` flag were not exercised.
- `psi_per_cgroup` was read from this process's own cgroup directory only; the non-root EOPNOTSUPP branch was not exercised.
- The `r1.py --bpflsm` mode is covered by a synthetic-fixture self-test only; it has not seen a real `C3-bpflsm-<variant>` run.

## Deviations from Plan

None to the plan's behaviour. Discretion and process notes:

- [Interpretation] S4 and S6 delta is percent change of the leg value (see decisions). HARN-01 text defines the tolerance but not the S4/S6 delta.
- [Process] The per-plan commit ledger file `gsd-plan-head-before-08-01` under the worktree git dir could not be written (the write was refused by the tool guard). `plan_head_before` and `commits` above are measured from the known base SHA with `git rev-list --count cf8766e5ccc6a82660c210827a18b825cf0abd87..HEAD` (3), taken after the third task commit and before this SUMMARY commit. The SUMMARY commit makes it 4.
- [Process] r1.py gained a `Result` class and the `--bpflsm` mode beyond the minimal calculator, because 08-03 tags `C3-bpflsm-<variant>` and needs per-variant R1 lines.

## Files touched outside the plan's files_modified

None. `scripts/check.sh` is in the plan's list (the plan places only the tools under tools/linux-baseline). STATE.md, ROADMAP.md and the user's uncommitted ROADMAP.md/REQUIREMENTS.md edits were not touched. `tools/linux-baseline/bin/` and `results/smoke-floor/` are gitignored build and run output.

## Known Stubs

None.

## Threat Flags

None. `floor.c` emits kernel and feature facts only, never argv, environment or user names (T-08-06). The one new file read outside the plan's expectations is `/proc/version_signature` (falls back to `uname` version), which holds a kernel package string.

## Self-Check: PASSED

- FOUND: tools/linux-baseline/r1.py, tools/linux-baseline/transfer.py, tools/linux-baseline/floor.c
- FOUND commits: 097ee7b, 8cd9ed9, df2d6b2 (ancestors of HEAD)
