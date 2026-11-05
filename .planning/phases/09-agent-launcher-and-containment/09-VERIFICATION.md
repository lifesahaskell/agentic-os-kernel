---
phase: 09-agent-launcher-and-containment
verified: 2026-11-05T00:00:00Z
status: passed
score: 5/5 must-haves verified
covered_files:
  - .planning/phases/09-agent-launcher-and-containment/09-01-PLAN.md
  - .planning/phases/09-agent-launcher-and-containment/09-01-SUMMARY.md
  - .planning/phases/09-agent-launcher-and-containment/09-02-PLAN.md
  - .planning/phases/09-agent-launcher-and-containment/09-02-SUMMARY.md
  - .planning/phases/09-agent-launcher-and-containment/09-03-PLAN.md
  - .planning/phases/09-agent-launcher-and-containment/09-03-SUMMARY.md
  - .planning/phases/09-agent-launcher-and-containment/09-04-PLAN.md
  - .planning/phases/09-agent-launcher-and-containment/09-04-SUMMARY.md
  - .planning/phases/09-agent-launcher-and-containment/09-05-PLAN.md
  - .planning/phases/09-agent-launcher-and-containment/09-05-SUMMARY.md
  - .planning/phases/09-agent-launcher-and-containment/09-06-PLAN.md
  - .planning/phases/09-agent-launcher-and-containment/09-06-SUMMARY.md
  - tools/aok-launch/aok-launch.conf
  - tools/aok-launch/src/bin/aok-shim.rs
  - tools/aok-launch/src/cgroup.rs
  - tools/aok-launch/src/identity.rs
  - tools/aok-launch/src/landlock.rs
  - tools/aok-launch/src/lib.rs
  - tools/aok-launch/src/main.rs
  - tools/aok-launch/src/seccomp.rs
  - tools/aok-launch/src/sys.rs
  - tools/aok-launch/tests/common/mod.rs
  - tools/aok-launch/tests/errno_contract.rs
  - tools/aok-launch/tests/fallback.rs
  - tools/aok-launch/tests/identity.rs
  - tools/aok-launch/tests/lifecycle.rs
  - tools/aok-launch/tests/registry_confinement.rs
  - tools/aok-launch/tests/shellout.rs
  - tools/aok-launch/tests/spawn.rs
  - tools/aok-launch/tests/subset.rs
  - tools/aok-launch/tests/xattr_forgery.rs
  - tools/linux-baseline/check-wrap-identity.sh
  - tools/linux-baseline/run.sh
covered_digest: "v3:sha256:7b353ee99f9946ef6a4efe0867ab545469b3effbd6a9913cf2061d14d5f7768c"
behavior_unverified: 0
overrides_applied: 0
---

# Phase 9: Agent launcher and containment Verification Report

**Phase Goal:** As the researcher, I want to spawn each agent into its own cgroup with a pidfd-resolvable identity, narrowed Landlock and seccomp layers, and subtree freeze and kill, so that every later component has an unprivileged principal to key on.
**Verified:** 2026-11-05
**Status:** passed
**Re-verification:** No, initial verification
**Host:** WSL2, kernel 6.18.33.2. Landlock ABI 7. Every result below is from this host.

Passed on this host, under the scope the phase set for itself. Three kinds of result are weaker than they read and are
kept apart below: simulated (D-13 shim), indicative (D-15/D-16/D-18 depth reads on WSL2), and not exercised
(CLONE_NNP, kernel 7.1). None of them is a real-old-kernel measurement or an S3 verdict.

## User Flow Coverage (MVP mode)

The goal parses as a User Story. The outcome clause is: every later component has an unprivileged principal to key on.

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| Researcher spawns an agent | Own cgroup, born there by `clone3` + CLONE_INTO_CGROUP | `lib.rs` `spawn()` -> `clone3_fork_into_cgroup`; `spawn.rs` (6 tests) | VERIFIED |
| A peer resolves who it is | `agent_id` from a pidfd, rooted in a launcher-written registry | `identity.rs` `resolve()`; `identity.rs` tests (14) | VERIFIED |
| Authority is narrowed, never widened | Landlock layered, seccomp stacked, NoNewPrivs 1 | `landlock.rs`, `seccomp.rs`; `subset.rs` (5) | VERIFIED |
| Shelling out does not escape | `sh -c` grandchild inherits cgroup, domain, filter | `shellout.rs` (3) | VERIFIED |
| Researcher freezes and kills the subtree | `frozen 1`, no task advances; kill reaps; exit via pidfd | `lifecycle.rs` (3) | VERIFIED |
| Denials are errnos | Documented errno, no KILL, filter reads arch and nr only | `seccomp.rs` unit tests; `errno_contract.rs` (4) | VERIFIED |

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A launched agent finds itself in its own cgroup; a sub-agent's cgroup is a descendant of its parent's; a peer holding a pidfd for any task in the subtree resolves the same `agent_id`; on missing `CLONE_INTO_CGROUP` / `PIDFD_GET_INFO` the fallback runs and reports the window or race (LNCH-01, LNCH-02) | VERIFIED (fallback legs simulated) | Native path: `lib.rs` `spawn()` creates `<root>/<name>/main`, then `clone3` with CLONE_INTO_CGROUP; child reads `/proc/self/cgroup`. Resolution: `identity.rs` `resolve()` uses PIDFD_GET_INFO and cross-checks cgroup id against the directory inode (`Mismatch`). Fallback: `lib.rs` falls back only on `Some(libc::ENOSYS)`, `identity.rs` `resolve_route` only on ENOTTY; both report `window_ns`. Fallback tests (`fallback.rs`, 2) run behind `aok-shim` and are labelled simulated. |
| 2 | A sub-agent granted a path its parent lacks still cannot open it; cannot remove an inherited Landlock rule or seccomp filter; `NoNewPrivs: 1` (LNCH-03) | VERIFIED (prctl path only) | `subset.rs` (5): subset_landlock, subset_seccomp, nnp, zero_grant_denies, cannot_remove_or_widen. `lib.rs` `child_main`: landlock::apply, seccomp::apply, `sys::set_no_new_privs`, exec. 16-layer cap gives E2BIG on the 17th. CLONE_NNP is not used and not exercised (see below). |
| 3 | A `sh -c` grandchild reports the agent's cgroup id and is refused the same out-of-grant path (LNCH-04) | VERIFIED | `shellout.rs` (3), `shellout_inherits`. Cgroup id and the refusal are asserted in the same test; see the weak-test note. |
| 4 | After a freeze `cgroup.events` reads `frozen 1` and no task runs; after `cgroup.kill` every task is reaped and the parent sees exit through a pidfd; retires the ASSUMED `cgroup.freeze` on a delegated leaf (LNCH-05) | VERIFIED | `cgroup.rs` `set_frozen` waits for `frozen` on the whole subtree; `kill_agent` writes `cgroup.kill`, waits populated 0, rmdirs deepest first; `check_not_inside` refuses self-targeting (EPERM). `lifecycle.rs` `freeze_thaw_kill`: agent dir and leaf both read `frozen 1`, the task's CPU ticks do not advance across 400 ms, thaw returns to `frozen 0` and the task advances, kill ends at `populated=0` with the pidfd exit seen. |
| 5 | Every denial returns a documented errno rather than killing the process; the seccomp filter inspects no pointer argument (LNCH-06) | VERIFIED | `seccomp.rs`: 383-entry x86-64 v6.18 table, 23 denied numbers, 71-instruction classic-BPF program. Unit test `program_reads_only_arch_and_nr_and_never_kills` interprets the program: no load beyond arch and nr, no KILL return. `x32_and_foreign_arch_are_eperm_for_every_number`, `every_number_gets_the_documented_action`. `errno_contract.rs` (4) against the real kernel, including i386 `int 0x80` -> EPERM. |

**Score:** 5/5 truths verified, 0 present-but-behavior-unverified.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `tools/aok-launch/src/lib.rs` (627) | spawn, fallback, lifecycle routing | VERIFIED | Substantive, wired from `main.rs`, exercised by every integration test |
| `tools/aok-launch/src/cgroup.rs` (456) | agent layout, freeze, kill, self-guard | VERIFIED | Wired from `lib.rs` |
| `tools/aok-launch/src/identity.rs` (1128) | registry, pidfd resolve, whois | VERIFIED | Wired; registry 0600 in 0700, flock, atomic rename |
| `tools/aok-launch/src/landlock.rs` (564) | grants, floor, preflight, apply | VERIFIED | `landlock_abi_floor = 6` read from the embedded `aok-launch.conf` |
| `tools/aok-launch/src/seccomp.rs` (489) | allowlist, program, in-crate interpreter | VERIFIED | The module doc is the allowlist record |
| `tools/aok-launch/src/bin/aok-shim.rs` | D-13 test shim | VERIFIED | Test-only; matches `args[1]` for the ioctl request, which is not a pointer |
| `tools/linux-baseline/run.sh` `--wrap` | additive hook | VERIFIED | `check-wrap-identity.sh df2d6b2` exit 0 |
| `09-ALLOWLIST.md`, `09-REPORT.md`, `09-VALIDATION.md` | records | VERIFIED | Numbers recomputed, see below |
| `tools/linux-baseline/results/09-lldepth/` | D-16 depth reads | VERIFIED as committed (indicative) | Matches report; see Data-Flow and Open Items |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `main.rs` | `lib.rs` `spawn` / `lifecycle` | dispatch run, nest, freeze, thaw, kill, whois | WIRED | |
| `spawn()` | `landlock::preflight` | before any cgroup or registry write | WIRED | No partial state on refusal |
| `child_main` | landlock, seccomp, NNP, exec | ordered in the clone3 child | WIRED | Errors returned over a CLOEXEC pipe |
| `spawn()` | registry | `identity::register` for a root | WIRED | Root keyed by cgroup id |
| `resolve()` | PIDFD_GET_INFO, then `/proc/PID/cgroup` | ENOTTY-only fallback | WIRED | |
| `run.sh --wrap` | `aok-launch nest` | D-16 depth reads | WIRED | Byte-identical when `--wrap` is absent |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `whois` output | `agent_id` | registry file plus kernel cgroup id from PIDFD_GET_INFO | Yes | FLOWING |
| Depth report added_p50 | openat_rootfs 480 / 521 / 600 ns at depth 1 / 2 / 4 | `summary.csv` rows | Yes, matches the report | FLOWING (indicative) |
| Depth report openat_tmpfs | n/a | C0 spread 10.21, ineligible | Not reported as a result | Honestly excluded |
| Depth report tcp_connect | n/a | refused connects, about -86.8 percent, errors 110000 per label | No | Not a result; open item below |

### Behavioral Spot-Checks and Test Run

Run once, from the repo root, in one delegated scope:
`systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1`

| Target | Passed | Failed |
|--------|--------|--------|
| lib (unit) | 54 | 0 |
| errno_contract | 4 | 0 |
| fallback | 2 | 0 |
| identity | 14 | 0 |
| lifecycle | 3 | 0 |
| registry_confinement | 1 | 0 |
| shellout | 3 | 0 |
| spawn | 6 | 0 |
| subset | 5 | 0 |
| xattr_forgery | 1 | 0 |
| **Total** | **93** | **0** |

Also run: `cargo fmt --check` exit 0, `cargo clippy` exit 0, `check-wrap-identity.sh df2d6b2` exit 0.

### Probe Execution

No `probe-*.sh` declared by the phase plans. Skipped.

### Requirements Coverage

Plan frontmatter IDs across 09-01..09-06: LNCH-01, LNCH-02, LNCH-03, LNCH-04, LNCH-05, LNCH-06. REQUIREMENTS.md maps exactly those six to Phase 9. No orphan.

| Requirement | Source Plans | Status | Evidence | How the test could pass while the requirement is false |
|-------------|--------------|--------|----------|---------|
| LNCH-01 | 09-01, 09-04, 09-06 | SATISFIED | `spawn_into_cgroup`, `sub_is_descendant`, `exit_contract`; fallback `spawn_fallback_window` (simulated) | "Born in the leaf" rests on the child's self-reported `spawn_path` plus the code path; no test samples the cgroup of a task before it runs. The fallback is exercised only behind the shim. |
| LNCH-02 | 09-02, 09-03, 09-04, 09-06 | SATISFIED | `identity_from_pidfd`, registry tests, `xattr_forgery`, `registry_unreachable_from_agent`, `fallback_equivalence` (simulated), `identity_fallback_race` (simulated) | `identity_fallback_race` asserts that the window is reported, not that a race occurs or is closed. That matches the requirement text ("reports the race it cannot close") but proves no race property. |
| LNCH-03 | 09-03, 09-05, 09-06 | SATISFIED (prctl path) | `subset_landlock`, `subset_seccomp`, `nnp`, `cannot_remove_or_widen`, `zero_grant_denies`; floor 6 | CLONE_NNP is not exercised. The ABI floor and old-kernel Landlock paths are simulated. The requirement text allows prctl "else", so the native path it tests is the one it must. |
| LNCH-04 | 09-03, 09-06 | SATISFIED | `shellout_inherits` | The refusal is shown by `cat` rather than by the same grandchild that reported the cgroup id. Both are children of the agent through `sh -c`, and the inheritance is kernel behavior, so the gap is small. |
| LNCH-05 | 09-01, 09-06 | SATISFIED | `freeze_thaw_kill`, `zero_member_kill` | One busy task in a two-level subtree (agent dir and `main` leaf). No test freezes or kills a subtree holding a nested sub-agent or several tasks, and the live self-cgroup guard (`check_not_inside`) has no integration test. The code writes the agent directory and the kernel applies it to the subtree, which is the mechanism the requirement names. |
| LNCH-06 | 09-03, 09-06 | SATISFIED | `seccomp.rs` unit tests, `errno_contract` (4) | Only 3 of the 23 denials run against the real kernel. The rest are covered by the in-crate BPF interpreter, which is the same author's model of BPF, not the kernel's. |

### Specific Checks Demanded

| Check | Result |
|-------|--------|
| ALLOWLIST numbers | Recomputed independently from the table: 383 entries, 23 denied, 71 instructions. All match. |
| Filter reads only arch and nr, never KILL | Verified in code and by the interpreting unit test. Denied -> EPERM, gaps and above-table -> ENOSYS, foreign arch and x32 -> EPERM. |
| CLONE_NNP deferred | No `CLONE_NNP` in `lib.rs` or `sys.rs`. No code depends on it. 09-REPORT.md, VALIDATION (Manual-Only table) and the requirement text treat it as not exercised. |
| `run.sh --wrap` additive | Diff read; `check-wrap-identity.sh df2d6b2` exit 0. |
| Simulated and indicative labelling | Report, ALLOWLIST and VALIDATION label D-13 results "simulated: shim-driven" and depth reads "indicative, WSL2, no S3 verdict". I found no line presenting either as a real-old-kernel measurement or an S3 verdict. |
| Commit hygiene | 0 commits with a `Co-Authored-By` or `Claude-Session` trailer. `git log -- 08-GATE.md` shows only the 08-04 template commit. No Mark, Answer or "Marked on" line carries a value. DEC-02 and DEC-03 are unmarked. |
| Disclosed open items true | `tcp_connect` under `--wrap` fails 15/15 wrapped rounds (summary.csv). `selfcheck.txt` records `landlock=none`. The seccomp cost confound is visible (`getppid` +40 ns at every depth). T-09-15 is real: `landlock::preflight` checks syntax, floor and path existence only. T-09-13 is moved to Phase 10. D-04 (seccomp action cache) is ASSUMED: no `/proc/PID/seccomp_cache` on this host, no test asserts engagement. |

### Anti-Patterns Found

Debt-marker scan (TBD, FIXME, XXX) over the covered implementation files: none without a follow-up reference. No stub, placeholder or empty-handler pattern reaches output. No blockers.

### Human Verification Required

None required for the verdict. The following are recorded limits, not pending human gates:

1. CLONE_NNP success path: re-run on a kernel 7.1+ host.
2. Seccomp action cache engagement (D-04): needs `CONFIG_SECCOMP_CACHE_DEBUG`.
3. musl 1.2.6 probe behavior under the filter: NOT MEASURED, no musl toolchain on this host.

### Gaps Summary

No gap fails a requirement or a success criterion. Items that stay open, none of which this phase's requirements demand:

- Wrapped `tcp_connect` depth rows are refused connects, and the wrapped selfcheck records FAIL. Only the openat rows are usable, and they are indicative. Source: `tools/linux-baseline/results/09-lldepth/summary.csv`, `selfcheck.txt`.
- Depth reads carry a seccomp cost confound; they are not an S3 verdict. DEC-02 and DEC-03 stay with the user.
- Real-old-kernel behavior (no CLONE_INTO_CGROUP, no PIDFD_GET_INFO, Landlock below the floor) is simulated only.
- T-09-15: `landlock::preflight` (`tools/aok-launch/src/landlock.rs`) does not check that a granted path is something the policy intends, only that it exists.
- Bookkeeping, not a code gap: `.planning/REQUIREMENTS.md` lines 253-258 still read "Pending" for LNCH-01..06. Updating them belongs to the orchestrator, and I did not edit them.

### Verified versus not verified

Verified by running: the 93-test suite (all green), fmt, clippy, `check-wrap-identity.sh`, and an independent recompute of the allowlist counts.
Verified by reading: spawn, identity, cgroup, landlock and seccomp code paths; the committed depth artifacts; the plan frontmatter; the 08-GATE history; commit trailers.
Not verified: behavior on a kernel older than this host; CLONE_NNP; the seccomp cache; musl; a sibling agent's `/proc/PID/environ`; kernels with `CONFIG_IA32_EMULATION` off (the `arch_mismatch_eperm` test would fail there rather than skip); an S3 verdict. I did not read each Mark block of 08-GATE.md in full, only confirmed by grep that none carries a value.

---

_Verified: 2026-11-05_
_Verifier: Claude (gsd-verifier)_
