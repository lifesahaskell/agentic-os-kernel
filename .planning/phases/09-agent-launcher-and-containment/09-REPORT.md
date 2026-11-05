# Phase 9 report: agent launcher and containment

Date: 2026-11-05. Host: WSL2, kernel 6.18.33.2, Landlock ABI 7 (floor 6), `CLONE_INTO_CGROUP` and
`PIDFD_GET_INFO` present, cgroup v2 with a delegated `systemd-run --user` scope. One host, one run.

Goal-backward check: for each LNCH requirement, what had to be true and which named test or committed
artifact is the evidence. A result forced by the seccomp shim (D-13) is labelled simulated. The depth reads
(D-16) are indicative and carry no S3 verdict.

## Gate run in 09-06

| Command | Result |
|---|---|
| `cargo +1.99.1 fmt --manifest-path tools/aok-launch/Cargo.toml --check` | exit 0 |
| `cargo +1.99.1 clippy --manifest-path tools/aok-launch/Cargo.toml --all-targets -- -D warnings` | exit 0 (finished from cache; no sources changed since the 09-05 run) |
| `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1` | exit 0, 93 passed, 0 failed, 0 ignored |
| `bash tools/linux-baseline/check-wrap-identity.sh df2d6b2` (after `make -C tools/linux-baseline`) | exit 0, 14 identical taskset invocations |

Tests per binary: lib 54, main 0, aok-shim 0, errno_contract 4, fallback 2, identity 14, lifecycle 3,
registry_confinement 1, shellout 3, spawn 6, subset 5, xattr_forgery 1, doc-tests 0. Summed test time
about 4.9 s (build cached).

Plan names versus suite names: the plan lists `registry_root_from_inside_agent_is_eacces`; the suite has
`registry_root_from_inside_agent_is_eacces_a` and `_b` (both passed, 09-03 recorded the same). The plan
lists `floor_from_conf` and `floor_check` unit tests; the suite has `landlock::tests::floor_from_conf_rejects_junk`,
`floor_from_the_real_conf_is_six`, `floor_check_passes_at_and_above`, `floor_check_refuses_below_and_names_the_scopes`
and `floor_check_zero_means_absent_and_names_every_right` (all passed). The plan's "name-guard cases" are
`name_guard` (tests/spawn.rs) and three `cgroup::tests::name_guard_*` unit tests. No test named in the plan is
absent from the output.

## Evidence

| Req | What had to be true | Evidence |
|---|---|---|
| LNCH-01 | A launched agent is born in its own `<root>/<name>/main` cgroup through clone3 `CLONE_INTO_CGROUP` (cgroup id equal to the leaf inode), or through the ENOSYS-only fork and move fallback; a sub-agent is a strict descendant built without writing the delegated root | `spawn_into_cgroup`, `sub_is_descendant`, `name_guard`, `clone3_eperm_is_not_a_fallback`; `fallback_reaches_cgroup` and `spawn_fallback_window` (simulated on a kernel that has clone3: the shim forces ENOSYS) |
| LNCH-02 | A peer resolves the same `agent_id` from a pidfd for any task in the subtree; the `SO_PEERCRED` fallback resolves the same key; a root launch from inside an agent is refused EACCES; a default-grant agent can neither open nor write the registry | `identity_from_pidfd`, `a_second_task_in_the_subtree_resolves_the_same_agent`, `registry_root_from_inside_agent_is_eacces_a` and `_b`, `registry_malformed_line_skipped`, `registry_unreachable_from_agent`, `xattr_forgery` (negative control, D-10); `fallback_equivalence`, `whois_listen_falls_back_to_so_peercred_on_enotty` and `identity_fallback_race` (simulated on a kernel that has PIDFD_GET_INFO: the shim forces ENOTTY) |
| LNCH-03 | A sub-agent runs under a strict subset of its parent's Landlock and seccomp layers with NoNewPrivs 1; an agent cannot remove or widen an inherited layer; an agent with only the default grants is denied everything else; a below-floor ABI is refused; the 16-layer cap holds | `subset_landlock`, `nnp`, `subset_seccomp`, `cannot_remove_or_widen`, `zero_grant_denies`, `depth_cap`; `floor_refusal` (simulated on a kernel that has Landlock ABI 7: the shim forces landlock_create_ruleset to ENOSYS) |
| LNCH-04 | An `sh -c` grandchild inherits the agent's confinement and resolves the agent's cgroup id; a launcher grant cannot widen authority (the D-12 falsifier) | `shellout_inherits`, `launch_cannot_widen` |
| LNCH-05 | Freeze, thaw and kill act subtree-wide on the agent directory; exit is observed through the pidfd with `populated 0`; `run` exits with the leader's status (64 usage, 125 refusal) | `freeze_thaw_kill`, `zero_member_kill`, `exit_contract` |
| LNCH-06 | Every denial returns a documented errno (EPERM for known-denied, foreign arch and x32; ENOSYS for gaps and above-table numbers) and never a kill; the filter reads no syscall argument. Policy record: [09-ALLOWLIST.md](09-ALLOWLIST.md) | `errno_contract`, `arch_mismatch_eperm`, `filter_no_ptr`, `subset_seccomp`; unit tests `every_number_gets_the_documented_action` and `x32_and_foreign_arch_are_eperm_for_every_number` |

### ROADMAP Phase 9 success criteria

1. Own cgroup, descendant sub-agent, pidfd-resolved `agent_id`, fallback reporting its window or race:
   `spawn_into_cgroup`, `sub_is_descendant`, `identity_from_pidfd`, and for the fallbacks
   `spawn_fallback_window` (window_us=325) and `identity_fallback_race` (window_ns=31951). Both fallback
   results are simulated on a kernel that has the feature, single samples, not gated. No real pre-5.7 or
   pre-6.13 kernel was run.
2. A sub-agent cannot open a path its parent lacks, cannot remove an inherited layer, shows NoNewPrivs 1:
   `subset_landlock`, `cannot_remove_or_widen`, `nnp`.
3. A `sh -c` grandchild reports the agent's cgroup id and is refused the same path: `shellout_inherits`.
4. `frozen 1` subtree-wide, no task advancing (CPU ticks from `/proc/PID/stat` unchanged), `cgroup.kill`
   reaps every task, the pidfd reports the exit: `freeze_thaw_kill`, `zero_member_kill`. This retires the
   ASSUMED item that `cgroup.freeze` works on a delegated leaf, on this host.
5. Documented errno, no kill, no pointer argument read: `errno_contract`, `arch_mismatch_eperm`,
   `filter_no_ptr`.

## Limits of the evidence

- CLONE_NNP is not exercised in Phase 9: CLONE_NNP arrives in kernel 7.1, this host is 6.18, and only the prctl(PR_SET_NO_NEW_PRIVS) path is tested natively, by nnp (D-13); re-run on a 7.1+ host.
- D-04, seccomp action-cache engagement, stays ASSUMED. The filter reads only `arch` and `nr`, but the cache is unobservable here (`CONFIG_SECCOMP_CACHE_DEBUG` is unset, so `/proc/PID/seccomp_cache` is absent) and no test asserts it.
- The D-16 depth reads are indicative, WSL2, with no S3 verdict. Single host, five rounds per label. Details under "Depth reads" below.
- T-09-15 residual: registry confinement is proven for the default and D-12 implicit grants only. An operator `--grant` covering `/run/user/UID/aok-launch` (for example `rw:/run/user/UID` or `rx:/`) would expose the registry, and preflight does not refuse it.
- T-09-13 moved to Phase 10: the D-12 implicit rw grant on the agent's own directory covers its interface files, so an agent can raise its own `memory.max` and `pids.max` and its sub-agents' up to the nearest ancestor's limit. The kernel's hierarchical cap still holds at the delegated root. Phase 9 sets no budgets.
- Simulated results (D-13), all on kernels that have the feature: `fallback_reaches_cgroup`, `spawn_fallback_window` (clone3), `fallback_equivalence`, `whois_listen_falls_back_to_so_peercred_on_enotty`, `identity_fallback_race` (PIDFD_GET_INFO), `floor_refusal` (Landlock ABI 7). They show the launcher's degraded branch runs for real and reaches the right result. They are not native measurements on an older kernel. The `window_us` and `window_ns` figures measure this machine's cost of the step, not a race on an old kernel.
- Only ABI 0 is exercised end to end for the floor. `floor_check(5, 6)` and other values are unit tests of the pure function. The floor of 6 is the DEC-03c proposal; the DEC-03 mark is not made here.
- musl behaviour under the filter is NOT MEASURED (glibc only, no musl toolchain on this host). Open item for the first musl-linked agent (B3).
- `/proc` is granted read-only wholesale. Reading a sibling agent's `/proc/PID/environ` was not tested.
- `arch_mismatch_eperm` assumes `CONFIG_IA32_EMULATION` is on; it would fail, not skip, without it.
- The `_b` EACCES test (`registry_root_from_inside_agent_is_eacces_b`) uses a python3 ctypes Landlock ruleset as a stand-in for a launcher layer; `registry_unreachable_from_agent` covers the launcher-applied layer.
- `fallback.rs` `identity_fallback_race` relies on a 1 s sleep as its margin; its listener children can leak on an early panic.
- Launcher limits raised in 09-01 and 09-02 and not fixed: `validate_name` accepts tab and newline (the registry writer refuses a path with a control character, so the registry cannot be corrupted); dead registry lines persist until the next root launch prunes them; tests share the real per-uid registry file; SIGTERM and SIGINT are not forwarded to the child; evacuating a non-delegated scope is guarded only by the mount-root EPERM and the mkdir EACCES; there is no live self-cgroup guard test (a unit test covers the refusal); concurrent freeze, thaw and kill on one agent are not interleaving-tested; simultaneous root launches racing on the registry lock are not tested; boot-id rollover is not tested across a reboot.
- 09-03 made two gate-fix commits by the orchestrator, `0c9c6b1` (`off()` uses `try_into().expect` so the literal `try_from` gate holds) and `d21c9e5` (the `floor_refusal` note gains the word simulated). Neither changed an assertion.

## Depth reads (09-05, D-16)

Run dir `tools/linux-baseline/results/09-lldepth/`, with `depth-report.md`. The byte-identity gate for the
`run.sh --wrap` hook (a Phase-8-shared file, edited additively) was re-run in 09-06 and exits 0.

- Indicative, WSL2, no S3 verdict. `openat_rootfs` added_p50 over unwrapped C0: 480 ns (21.38 percent) at depth 1,
  521 ns (23.21) at depth 2, 600 ns (26.73) at depth 4, r1.py p50 eligible. `openat_tmpfs` is ineligible at every
  depth (unwrapped C0 spread_p50 10.21 percent; the rule is strictly below 10). Every p99 leg reads unstable on this host.
- Each depth layer is the launcher's whole containment, Landlock plus seccomp, so the deltas include seccomp cost
  (`getppid` +40 ns at every depth). The wrap cannot separate Landlock cost from seccomp cost.
- The run's own `selfcheck.txt` reports `Seccomp_filters` 1, 2 and 4 and NoNewPrivs 1 for the three depths but `landlock=none`
  on the wrapped labels, because that field reports only what `micro` applies itself. It does not read the launcher's Landlock
  layers. That Landlock was in force is evidenced by the `tcp_connect` refusal below and by the crate tests, not by the selfcheck.
- Open item for a later plan: `tcp_connect` cannot run under the launcher. The ruleset handles ConnectTcp, `micro` binds an
  ephemeral loopback port, and the grant syntax is `tcp:PORT`, so no static grant covers it. All 15 wrapped rounds record
  `result=FAIL errors=22000` (all `tcp_connect`) and `micro` exits 2. The `tcp_connect` rows measure a refused connect
  (added_p50 about -86 percent) and are not a Landlock measurement. Every other bench has errors=0. `--wrap` cannot run the
  full micro suite until there is an ephemeral-port grant form or a tcp-bench skip.
- The reads were taken as 15 per-round `run.sh` invocations plus one hand-run `summarize.py`; `env.txt` carries each
  `wrap_micro_lldepth-N` key five times.
- D-15 asks whether Landlock alone is the largest share of the S3 miss. These reads cannot answer it, for the
  seccomp-confound reason above. That question goes to the gate, not to this phase.

## Merged suite

The 09-01 and 09-02 tests were written for unconfined agents and first ran under default-deny in 09-03. 09-03 predicted
three possible grant adjustments. 09-03-SUMMARY.md, heading "Grant adjustments to earlier tests", says: none.

| Predicted | Outcome (09-03-SUMMARY.md) | Checked in 09-06 |
|---|---|---|
| `freeze_thaw_kill` (tests/lifecycle.rs), if its does-not-advance witness wrote outside the default grants | not needed: it reads progress from `/proc/PID/stat` | `git diff --stat 6d34a4a HEAD -- tools/aok-launch/tests` lists no change to tests/lifecycle.rs; test green |
| `xattr_forgery` (tests/xattr_forgery.rs), if it started its probe from /tmp through `aok-launch run` | not needed: it does not launch through `aok-launch` | no change to tests/xattr_forgery.rs; test green |
| the launch helper in tests/common/mod.rs, a grants parameter for those callers | not needed as a grants parameter | tests/common/mod.rs changed by 61 insertions and 0 deletions after 09-01, all additive helpers for the new 09-03 tests (`Scratch`, `parse_probe`, `probe_line`); no earlier caller changed |
| 09-04 tests (`spawn_fallback_window`, `identity_fallback_race`), none expected | none made; written under default-deny, no `--grant` | tests/fallback.rs is new, no `--grant` needed |

Made: none. Not needed: all three. Prediction misses: none. Every earlier test is unchanged since 09-03 and green in the 93-test run.
`zero_grant_denies` and `registry_unreachable_from_agent` are green, which together show the default grant was not widened.
Diff basis: `6d34a4a` is the 09-03 `plan_head_before`; the stat shows only added test files plus the additive `common/mod.rs` change.

## ADV candidates (interpretation, for Phases 10 and 11)

These are candidates, not verified defects.

a. Re-attribution. Identity resolves through the cgroup, so a task moved between leaves inside its own subtree is
   re-attributed to the new leaf. `moving_a_task_within_the_subtree_reattributes_it` records this as the designed behaviour,
   but the move in that test is done by the test process, not by a confined agent. Whether an agent can move itself, and
   whether the move should be denied, is open. `cgroup_directory_rename_is_eperm` shows a rename cannot change an id.
b. `clone3` `CLONE_INTO_CGROUP` placement is DAC-checked, not Landlock-checked, so placement outside the agent's own subtree may be
   possible where file permissions allow. Not tested in Phase 9 (Phase 10 or 11).
c. A D-12 grant beneath the agent's own directory includes that directory's interface files, for example `memory.max`
   and `pids.max` (T-09-13). Phase 10 budgets must protect them.

## What would show this phase was wrong

| Req | Falsifier | Status |
|---|---|---|
| LNCH-01 | an agent not born in its own `main` leaf, or a sub-agent whose cgroup is not a descendant | not observed in `spawn_into_cgroup`, `sub_is_descendant`; fallback simulated only |
| LNCH-02 | a pidfd and the fallback disagreeing on `agent_id`, or a path and a cgroup id disagreeing (D-06), or a rename changing an id | not observed in `fallback_equivalence`, `identity_fallback_race`, `cgroup_directory_rename_is_eperm` (fallback simulated) |
| LNCH-03 | a sub-agent reaching a path or syscall its parent lacks | not observed in `subset_landlock`, `subset_seccomp`, `cannot_remove_or_widen` |
| LNCH-04 | a grandchild escaping the agent's confinement, or a launcher grant widening authority (the D-12 falsifier) | not observed in `shellout_inherits`, `launch_cannot_widen` |
| LNCH-05 | the launcher killing or freezing a cgroup it lives in | guarded and unit-tested; no live integration test, by design |
| LNCH-06 | a denial returning an undocumented errno or SIGSYS; an os-test run (Phase 12) differing between agent and non-agent because of the filter (D-01, D-03) | not observed in `errno_contract`, `arch_mismatch_eperm`; the Phase 12 half is not testable now |
| D-07 | the no-internal-process rule not biting with the `main` leaf layout | the leaf layout is used by every spawn test; no test isolates the rule |

Not decided here: no DEC-02 or DEC-03 mark, no edit to 08-GATE.md, no S3 verdict.

Sign-off: validated. The 93-test suite is green in one delegated scope with fmt and clippy clean, every per-task row maps to a passing test or a committed artifact, and the open items above are limits recorded, not blockers.
