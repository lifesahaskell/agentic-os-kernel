---
phase: 09-agent-launcher-and-containment
plan: 06
subsystem: agent-launcher
tags: [validation, report, seccomp-allowlist, goal-backward]
status: complete
requires:
  - phase: 09-01..09-05
    provides: the aok-launch crate, its tests, the depth-read run dir
provides:
  - 09-ALLOWLIST.md - seccomp policy record checked against seccomp.rs
  - 09-REPORT.md - LNCH-01..06 truth and evidence, limits, merged suite, ADV candidates, falsifiers
  - 09-VALIDATION.md - Status cells set from the real run and signed off
affects: [phase-10, phase-11, phase-12]
plan_head_before: 21d8ae5a1775535f810cf2d2f8c23868b87c2a28
plan_head_after: 8a4a5453fcd21a8fe0c95f320bd2ad549894f28d
actuals:
  tokens: 7700
  tasks: 2
  commits: 2
requirements-completed: [LNCH-01, LNCH-02, LNCH-03, LNCH-04, LNCH-05, LNCH-06]
key-decisions:
  - "Sign-off set to validated: 93 tests green in one delegated scope, fmt and clippy clean, every per-task row maps to a passing test or a committed artifact."
---

# Phase 9 Plan 6: Gate, allowlist record, phase report and sign-off Summary

The whole crate is fmt and clippy clean and passes 93 tests in one delegated scope; `09-ALLOWLIST.md` and `09-REPORT.md` are written and `09-VALIDATION.md` is signed off validated, with the simulated, indicative and ASSUMED limits recorded in the report.

`actuals.tokens` is chars/4 over the hand-written 09-ALLOWLIST.md, 09-REPORT.md and the added 09-VALIDATION.md lines (about 30.7k characters), excluding this SUMMARY. It is a measured count on the estimate's scale, not a harness token count. `commits: 2` is the two task commits (`git rev-list --count` from the persisted ledger); this SUMMARY's commit follows.

## Task Commits

| Task | Commit | Subject |
|---|---|---|
| 1 | 4ac3898 | docs(09-06): record the seccomp allowlist and the full-suite status cells |
| 2 | 8a4a545 | docs(09-06): write the phase report and sign off validation |

## Gate results (run from the repo root)

| Command | Result |
|---|---|
| `cargo +1.99.1 fmt --manifest-path tools/aok-launch/Cargo.toml --check` | exit 0 |
| `cargo +1.99.1 clippy --manifest-path tools/aok-launch/Cargo.toml --all-targets -- -D warnings` | exit 0 (finished from cache) |
| `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1` | exit 0, 93 passed, 0 failed, 0 ignored |
| `make -C tools/linux-baseline` then `bash tools/linux-baseline/check-wrap-identity.sh df2d6b2` | exit 0, 14 identical taskset invocations |
| Task 1 ALLOWLIST greps (`syscall_64.tbl`, `0x40000000`, `ENOSYS`, `statmount`, `glibc`, `435`) | exit 0 |
| Task 2 verify 1, 2 and 3 (Evidence rows, CLONE_NNP rule and phrases, Sign-off agreement), run literally | exit 0, 0, 0 |

Tests per binary: lib 54, main 0, aok-shim 0, errno_contract 4, fallback 2, identity 14, lifecycle 3, registry_confinement 1, shellout 3, spawn 6, subset 5, xattr_forgery 1, doc-tests 0.

CLONE_NNP: the report has one line naming it, and that line says not exercised (count 1 of 1).

## Plan versus code or suite mismatches

- The plan names `registry_root_from_inside_agent_is_eacces`; the suite has `_a` and `_b`, both passed.
- The plan names unit tests `floor_from_conf` and `floor_check`; the suite has five `landlock::tests::floor_*` tests, all passed.
- No test named in the plan is absent from the output.
- The ALLOWLIST record matches the code: 383 entries, max 469, one gap 337-423, 23 deny entries with matching names, 14 ranges over 360 numbers, 71 instructions. Recomputed by a script over the committed `TABLE` and `DENY`. No mismatch to report.
- Merged-suite prediction: none of the three predicted grant adjustments (`freeze_thaw_kill`, `xattr_forgery`, the `common/mod.rs` launch helper) was made, and 09-03-SUMMARY.md says none. The git diff since `6d34a4a` shows tests/common/mod.rs changed additively (61 insertions, 0 deletions, new helpers only) and no earlier test file changed. No prediction miss.

## Findings carried into the report

- The depth run's own `selfcheck.txt` shows `landlock=none` for the wrapped labels, because that field reports only what `micro` applies itself. Seccomp stacking (1, 2, 4 filters) and NoNewPrivs are read from it; the launcher's Landlock layers are not. The report says so. 09-05's SUMMARY did not.
- `tcp_connect` under `--wrap` (open item), seccomp confound in the depth deltas, openat_tmpfs ineligible (spread 10.21 percent), single host WSL2 indicative with no S3 verdict: all recorded as handed over.
- The re-attribution test (`moving_a_task_within_the_subtree_reattributes_it`) moves the task from the test process, not from a confined agent. The report states ADV candidate (a) accordingly.

## Deviations from Plan

None. The plan executed as written. No source or test file was edited. 08-GATE.md, STATE.md, ROADMAP.md, .planning/milestone.lock, .planning/state.json and .claude/worktrees/ untouched; no DEC mark; no `state.*` verb run; nothing pushed.

## Judgement call to review

Sign-off is set to validated. The 09-05-02 row (depth reads) maps to a committed artifact, not a cargo test, and its wrapped rounds record `result=FAIL` for `tcp_connect`. I read that row's contract as "openat read at 1, 2 and 4 layers, reported indicative, WSL2", which the artifact meets (openat rows errors=0), and recorded the `tcp_connect` finding as an open item. If the orchestrator reads that row as requiring a clean wrapped run, the three flags revert to false and the last line of 09-REPORT.md to `Sign-off: not validated`.

## Not verified

- Any result on a kernel without clone3, PIDFD_GET_INFO or Landlock ABI 6; CLONE_NNP; musl under the filter; seccomp cache engagement; the depth reads were not re-taken in 09-06 (committed artifact only; the identity gate was re-run).
- Clippy returned from cache, so it did not recompile; the sources had not changed since the 09-05 full run.

## Known Stubs

None.

## Threat Flags

None. T-09-12 mitigated: simulated results labelled "simulated on a kernel that has the feature", depth reads "indicative, WSL2", no S3 verdict.

## Self-Check: PASSED

- Files: 09-ALLOWLIST.md, 09-REPORT.md, 09-VALIDATION.md present.
- Commits 4ac3898 and 8a4a545 are ancestors of HEAD.
