---
phase: 08-reference-environment-early-falsifiers-and-threshold-gate
plan: 03
subsystem: infra
tags: [bpf-lsm, lsm_cgroup, bpftool, cgroup-v2, shell, falsifier]

requires:
  - phase: 08-reference-environment-early-falsifiers-and-threshold-gate
    provides: tools/linux-baseline/run.sh micro suite and --tag support (plan 01); aok-adv list-routes/run CLI contract (plan 02)
provides:
  - BPF-LSM global programs (hit, miss, nocount, ancestor-keyed) for HARN-02
  - lsm_cgroup per-cgroup programs for HARN-03
  - bpfadv.bpf.c S5 deny programs and a counter slot per hook for ADV-03
  - tools/bpflsm/bpflsm.sh root wrapper with --probe, --variant, --harn03, --adv and --dry-run
affects: [08-04, 08-05]

actuals:
  tokens: 11400
  tasks: 3
  commits: 3
commits: 3
plan_head_before: cf8766e5ccc6a82660c210827a18b825cf0abd87
plan_head_after: 878d572a9516fa55cca971f486e9143207bf7aa6

tech-stack:
  added: []
  patterns:
    - "Root wrapper moves a BASHPID subshell into the cgroup, then drops to the measuring uid with setpriv"
    - "Attach success is never proof a hook fires: a counter delta is the validity evidence"

key-files:
  created:
    - tools/bpflsm/.gitignore
    - tools/bpflsm/Makefile
    - tools/bpflsm/probe.bpf.c
    - tools/bpflsm/bpflsm.bpf.c
    - tools/bpflsm/bpflsm_cg.bpf.c
    - tools/bpflsm/bpfadv.bpf.c
    - tools/bpflsm/bpflsm.sh
  modified: []

key-decisions:
  - "Denial proof uses a bash builtin open from a root subshell moved into the cgroup, so a file_open deny cannot be confused with a denied exec or library open"
  - "S5 deny rule: -13 when agent_cg[current]=a and subtree[target] is not a; agent-a=1, agent-b=2, broker and nonagent left out of both maps"
  - "cgroup.kill and cgroup.procs deny evidence is a file_open counter (A11, assumed), not a task_kill counter"
  - "--adv never writes yama ptrace_scope; it records the value in env.txt and refuses to run the second mode under a different value"
  - "--adv does not touch the CPU governor or bpf_stats_enabled; it is not a timing run"

patterns-established:
  - "Strings run_pinned, systemd-run and lsm.s/ appear in no file, comments included"

requirements-completed: [ENV-01, HARN-02, HARN-03, ADV-03]

duration: not recorded
completed: 2026-10-28
status: complete
---

# Phase 8 Plan 03: BPF-LSM sources and root wrapper Summary

**BPF-LSM global, lsm_cgroup, probe and S5 adversary programs plus a root wrapper (`tools/bpflsm/bpflsm.sh`) with four modes and a dry-run, authored on a host that can run none of it; nothing here was compiled, loaded, attached or verified by the kernel.**

## Verified and not verified

This host is WSL2: no root, no `/dev/kvm`, no active BPF LSM, no bpftool, no clang BPF toolchain in use. Every BPF and root step is labelled R in the plan and was not attempted.

### Verified (command run, output seen)

| Check | Command | Result |
|---|---|---|
| Wrapper parses | `bash -n tools/bpflsm/bpflsm.sh` | exit 0, no output |
| Wrapper lint | `shellcheck tools/bpflsm/bpflsm.sh` | exit 0, no output (two directives disabled with a reason: SC2046 for the intentional word split of `le64` output, SC1003 for a literal backslash in a `tr` set) |
| Dry run, all modes | `bash tools/bpflsm/bpflsm.sh --dry-run` | prints the four mode plans, exit 0 |
| Dry run, adv | `bash tools/bpflsm/bpflsm.sh --dry-run --adv agent --run-id t --uid 1000` | exit 0; output names `deny-counters.txt`, the `aok-phase8/adv` cgroup root, `yama_ptrace_scope` and the `route= topology= hook= before= after=` line shape |
| adv source coverage | `grep -cE 'agent_cg|subtree|SEC\("lsm/(task_kill|ptrace_access_check|socket_sendmsg|socket_bind|file_open)"\)' tools/bpflsm/bpfadv.bpf.c` | 12 (threshold 7) |
| Forbidden strings | `grep -nE 'run_pinned|systemd-run|lsm\.s/'` over the wrapper, all `.c` files and the Makefile | no match |
| Wrapper greps | `ADV/results`, `deny-counters.txt`, `aok-phase8/adv`, `topology=`, `yama_ptrace_scope` in `bpflsm.sh` | 5, 3, 3, 2, 5 matches |
| No deletions | `git show --diff-filter=D` on the last task commit | none |
| Files touched | `git diff --name-only cf8766e..HEAD` | exactly the seven files in `tools/bpflsm/` |

### Not verified (not run: needs reference host)

- `make -C tools/bpflsm`: not run: needs reference host (bpftool for `vmlinux.h`, clang with the BPF target, a BTF kernel). Whether any of the six objects compiles is unknown.
- Whether the verifier accepts any program, including the BTF pointer chains in `bpfadv.bpf.c` (`task->cgroups->dfl_cgrp->kn->id`, `sock->sk->sk_cgrp_data.cgroup->kn->id`, assumption A8) and `bpf_get_current_ancestor_cgroup_id` from an LSM program: not run: needs reference host.
- `bpflsm.sh --probe`: not run: needs reference host. `lsm_hook_fires`, `attach_global_file_open`, `attach_lsm_cgroup`, `dmesg_ebpf_lsm` are unobserved.
- `bpflsm.sh --variant V` for all four variants: not run: needs reference host. The denial proof and the validity file are unobserved.
- `bpflsm.sh --harn03`: not run: needs reference host. `lsm_cgroup_inherits` is unobserved (assumption A4).
- `bpflsm.sh --adv baseline` and `--adv agent`: not run: needs reference host. The `aok-adv` binary was also not built or run by this plan.
- The `bpftool -j map dump` JSON shape that `map_sum` parses, the autoattach link pinning layout, `bpftool map update ... key hex` byte order, and cgroup id equal to inode number (A10): all assumed, first checked in 08-04.
- Kernel behaviour recorded in 08-RESEARCH.md (A1-A17) is assumed throughout and is stated as assumed in the file comments.

## Accomplishments

- Task 1 (`e915c42`): `.gitignore` and `bpflsm.sh` (about 710 lines) with argument parsing, `--dry-run` printers, helpers, the EXIT cleanup trap, uid/root/BPF-LSM gates, and the `--probe`, `--variant` and `--harn03` mode bodies; `--adv` was a stub.
- Task 2 (`63a4fb8`): `probe.bpf.c`, `bpflsm.bpf.c` (one source, three objects by macro), `bpflsm_cg.bpf.c`, `Makefile`.
- Task 3 (`878d572`): `bpfadv.bpf.c` and `--adv baseline|agent`, which runs one `aok-adv` row at a time, reads the five per-hook counters before and after each row, and writes `tools/aok-adv/results/ID/{baseline,agent}.jsonl`, `env.txt` and `deny-counters.txt`.

## Design notes the later plans depend on

- Global programs: 0 allows, -13 denies. `lsm_cgroup` programs: 1 allows, 0 denies (EPERM by default, assumed). The two conventions are opposite and kept in separate files.
- Validity (D-09): denial proof plus `hook_count` delta >= summed micro `n` plus `hit_count` (> 0 for global-hit and anc, == 0 for global-miss). `global-miss-nocount` has no counters and rests on the denial proof alone.
- `--adv` writes per-row output to a temp file and appends it itself, so it does not depend on whether `aok-adv run --out` appends.
- Hook to micro bench mapping used for `n`: file_open = openat_tmpfs, openat_rootfs; socket_connect = unix_connect, tcp_connect; bprm_check_security = fork_exec_wait, posix_spawn; socket_sendmsg = unix_dgram_pingpong.
- `lsm_cgroup` `socket_sendmsg` is compiled only with `make -B CG_SENDMSG=1` (OQ7 default no; 08-05 Task 2 rebuilds if 08-04 answers yes).

## Open dependencies for later plans (not changed here)

- Whether `summarize.py --check` accepts a run directory holding only tagged C3 rows (checked first in 08-04).
- How the 08-02 report parses route names containing spaces in `deny-counters.txt`. The wrapper takes the last word of a `list-routes` row as the topology and the middle as the route.
- `aok-adv` must be built unprivileged (`cargo build --release --manifest-path tools/aok-adv/Cargo.toml`) before `--adv`; the wrapper dies with that hint if the binary is missing.
- The `23 rows` figure in the `--adv` dry-run text comes from the 08-02 plan, not from a run.

## Deviations from Plan

None - plan executed as written. No file outside the plan's `files_modified` was touched. STATE.md and ROADMAP.md were left to the orchestrator, and the user's uncommitted planning edits were not touched.

## Known Stubs

None.

## Threat Flags

None. The wrapper creates cgroups only under `/sys/fs/cgroup/aok-phase8/`, pins only under `/sys/fs/bpf/aok`, refuses to start if either exists, and removes both on every exit path. It never writes `ptrace_scope`, never enables cgroup controllers, and runs `--dry-run` without root.

## Self-Check: PASSED

Files present: all seven under `tools/bpflsm/`. Commits `e915c42`, `63a4fb8`, `878d572` are ancestors of HEAD.
