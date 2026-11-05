---
phase: 09-agent-launcher-and-containment
plan: 05
subsystem: linux-baseline
tags: [bash, run-sh, wrap-hook, byte-identity, landlock, depth-reads, wsl2]
status: complete
requires:
  - phase: 09-01
    provides: the nest CLI (nest --name BASE --depth N [--grant SPEC]... -- CMD) and the role=root|sub status lines
  - phase: 09-03
    provides: the rx:/rw:/tcp: grant syntax, the default grant, the D-12 implicit grants, the per-layer seccomp filter
provides:
  - tools/linux-baseline/run.sh --wrap hook (additive, Phase-8-shared file)
  - tools/linux-baseline/check-wrap-identity.sh, the re-runnable byte-identity gate
  - tools/linux-baseline/results/09-lldepth/, the indicative WSL2 depth reads at Landlock depths 1, 2, 4 against an unwrapped C0
affects: [09-06]
plan_head_before: 875fa256e6aef6a911b358a13e10782afd2e8046
plan_head_after: fc87233c8696c18184af011b44fcde148eed3271
actuals:
  tokens: 2100
  tasks: 2
  commits: 2
tech-stack:
  added: []
  patterns:
    - an empty bash array between taskset and the binary expands to no words under set -u (bash 5.2), so an optional wrapper costs the default path nothing
key-files:
  created:
    - tools/linux-baseline/check-wrap-identity.sh
    - tools/linux-baseline/results/09-lldepth/depth-report.md
    - tools/linux-baseline/results/09-lldepth/env.txt
    - tools/linux-baseline/results/09-lldepth/run.log
    - tools/linux-baseline/results/09-lldepth/micro.csv
    - tools/linux-baseline/results/09-lldepth/selfcheck.txt
    - tools/linux-baseline/results/09-lldepth/summary.csv
    - tools/linux-baseline/results/09-lldepth/summary.md
  modified:
    - tools/linux-baseline/run.sh
key-decisions:
  - "Depth reads taken one run.sh invocation per round, because every wrapped micro exits 2 on tcp_connect and run.sh aborts after the first failing micro; summarize.py was run by hand once at the end"
  - "No grant widened beyond the plan's list; the tcp_connect refusal cannot be granted away because micro's TCP listener port is ephemeral"
duration: about 25 minutes, of which the measurements are under 3 minutes
completed: 2026-11-05
---

# Phase 09 Plan 05: Landlock depth reads Summary

An additive `--wrap` hook in `run.sh`, proven byte-identical on the default path against df2d6b2, and indicative WSL2 openat reads at launcher depths 1, 2 and 4 taken on the real `aok-launch nest`. No S3 verdict is given (D-16/D-18).

`actuals.tokens` is chars/4 over the hand-written files only (the run.sh additions, check-wrap-identity.sh, depth-report.md); the generated CSV and log files are excluded.

## Phase-8-shared file

`tools/linux-baseline/run.sh` is a Phase 8 shared file (08-05 runs it with defaults). It was edited additively: a `WRAP=""` default, a `--wrap)` parse, a `--wrap needs --tag` validation (exit 64), a `WRAP_ARGV` array filled with `read -r -a` only when WRAP is non-empty, `"${WRAP_ARGV[@]}"` between `taskset -c "$CPU"` and `"$BIN/micro"`, a `wrap_micro_$TAG` kv key written only when WRAP is non-empty, and one usage line with the `sed -n '2,9p'` range widened by one. 17 insertions and 2 deletions in the diff (the two deleted lines are the widened usage range and the taskset line, each replaced). The identity gate output:

```
check-wrap-identity: OK, 14 identical taskset invocations, exit status 0 both, base df2d6b2
```

The gate was also shown to fail: with `"${WRAP_ARGV[@]}"` temporarily changed to the quoted-scalar `"${WRAP_ARGV[*]}"` it printed a diff of all 14 taskset lines and did not exit 0; run.sh was restored from a copy and the gate re-run green before the commit.

## Tasks

| Task | Commit | Subject |
|---|---|---|
| 1 | 744d3f6 | feat(09-05): add the additive --wrap hook to run.sh and its byte-identity gate |
| 2 | fc87233 | test(09-05): take the Landlock depth reads at depths 1, 2 and 4 on the real launcher |

## Plan gates, run literally

| Gate | Exit |
|---|---|
| Task 1 verify 1: `bash -n` on both scripts, `grep -q WRAP_ARGV`, `grep -q 'WRAP=""'` | 0 |
| Task 1 verify 2: `make -C tools/linux-baseline && bash tools/linux-baseline/check-wrap-identity.sh df2d6b2 && test ! -e .wrapid-pre.sh && test ! -e results/smoke-wrapid` | 0 |
| Task 1 verify 3: `--wrap /bin/true` without `--tag`, inner rc 64, message `run.sh: --wrap needs --tag`, no smoke-wrapneg dir | 0 |
| Task 2 verify 1: four labels C0, C0-lldepth-1/2/4 in micro.csv and non-empty env.txt | 0 |
| Task 2 verify 2: three `wrap_micro_lldepth-N=` keys, `aok-launch: role=root`, `aok-launch: role=sub` | 0 |
| Task 2 verify 3: depth-report.md non-empty with `indicative, WSL2`, `no S3 verdict` (case-insensitive), `added_p50`, the three depth names | 0 |

Also run: `cargo +1.99.1 build --release --manifest-path tools/aok-launch/Cargo.toml` (0; CARGO_TARGET_DIR unset, binary at tools/aok-launch/target/release/aok-launch), and after the reads were committed the full aok-launch suite once, `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1`: 54 lib and 39 integration tests passed, 0 failed (the crate is untouched by this plan).

## Depth reads (as printed by summary.csv; indicative, WSL2)

Run dir `tools/linux-baseline/results/09-lldepth/`, five rounds per label, micro pinned to CPU 2, unwrapped C0 first, then lldepth-1, -2, -4 strictly one after another, each invocation in its own fresh delegated scope, harness_dirty=no.

| depth | bench | added_p50 ns | added_p50 % | r1.py p50 |
|---|---|---|---|---|
| lldepth-1 | openat_rootfs | 480 | 21.38 | eligible |
| lldepth-1 | openat_tmpfs | 491 | 27.99 | ineligible: none: unstable on this host |
| lldepth-2 | openat_rootfs | 521 | 23.21 | eligible |
| lldepth-2 | openat_tmpfs | 531 | 30.27 | ineligible: none: unstable on this host |
| lldepth-4 | openat_rootfs | 600 | 26.73 | eligible |
| lldepth-4 | openat_tmpfs | 611 | 34.83 | ineligible: none: unstable on this host |

openat_tmpfs is ineligible because the unwrapped C0 spread_p50 is 10.21 percent (r1.py needs strictly below 10). Nothing was tuned. All openat p99 legs read "none: unstable on this host" at every depth. `run.log` has 15 role=root and 20 role=sub lines (5 at depth 2, 15 at depth 4); `selfcheck.txt` shows Seccomp_filters 1, 2, 4 for the three depths. Each layer is the launcher's whole containment (Landlock plus seccomp allow-list), so the deltas include seccomp cost (getppid +40 ns at every depth). The numbers are not compared with any S3 bound here, and the Phase 8 Landlock fs falsifier is a separate finding this plan does not rule on.

## Deviations from Plan

### Flagged findings

**1. [Prediction contradicted] Wrapped runs fail on tcp_connect; the plan predicted a clean wrapped C0**
- **Plan said:** the wrapped C0 is "expected to pass" and the run completes; a FAIL would be a flagged finding (Task 2 action).
- **Observed:** the sandbox.c C0 check itself passed at every depth (micro reached its benches; it returns 2 before any bench when `sb_check` fails). But all 15 wrapped rounds record `result=FAIL errors=22000` in selfcheck.txt, because every `tcp_connect` is refused: the launcher's Landlock ruleset handles ConnectTcp, micro's loopback listener binds an ephemeral port, and the grant syntax is `tcp:PORT`. A probe under `aok-launch run` (bind port 0, connect to it) returned EACCES; the same probe outside the launcher connected.
- **Consequence:** micro exits 2, run.sh logs "failed" and exits 1 after the round's rows are written, and the rest of the run and `suite_end` (summarize.py) are skipped. The five unwrapped C0 rounds ran in one invocation and exited 0. All wrapped tcp_connect rows measure a refused connect (med_p50 about 2400 ns against 18235 ns unwrapped, added_p50 about -86 percent) and are not a Landlock measurement. Every other bench has errors=0 in every wrapped round.
- **Not done:** no grant widened, no edit to micro.c, sandbox.c, summarize.py, or src/landlock.rs. Needs a decision before any later plan reuses `--wrap` for the full micro suite: either a tcp grant mechanism that covers an ephemeral port range, or a micro option to skip the tcp bench under a wrap. Both are outside this plan.

**2. [Process, Rule 3] One run.sh invocation per round for the wrapped labels**
- **Plan said:** one wrapped run.sh invocation per depth.
- **Done:** `--rounds R-R` for R in 1..5 per depth (15 wrapped invocations, each in its own fresh `systemd-run --user --scope -q -p Delegate=yes`), each exiting 1 (tcp_connect, above), then `python3 -I tools/linux-baseline/summarize.py` once by hand on the run dir. Side effect: env.txt carries each `wrap_micro_lldepth-N` key five times (once per invocation), and `loadavg_before_micro_R-R` lines per invocation.
- **Why:** with one invocation per depth, run.sh would have aborted after round 1 and left 4 of 5 rounds unmeasured.

### Auto-fixed Issues

None.

## Threat Flags

None. The `rw:/dev/shm` grant in the wrap (T-09-14) is as accepted in the plan.

## Known Stubs

None.

## Not verified

- Whether a depth-4 run on a quiet host would give a different spread: the loadavg_before values in env.txt rise from 0.11 to 0.71 across the 16 invocations, which is largely the runs themselves; no other process was started during the reads.
- No claim is made about Landlock cost separate from seccomp cost; the wrap does not separate them.

## Self-Check: PASSED

- FOUND: tools/linux-baseline/check-wrap-identity.sh, tools/linux-baseline/results/09-lldepth/{depth-report.md,env.txt,run.log,micro.csv,selfcheck.txt,summary.csv,summary.md}
- FOUND commits: 744d3f6, fc87233 (`git merge-base --is-ancestor` against HEAD)
- `git status --short` apart from this SUMMARY: only the three paths untracked at dispatch (.claude/worktrees/, .planning/milestone.lock, .planning/state.json)
