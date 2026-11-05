---
phase: 09-agent-launcher-and-containment
plan: 04
subsystem: agent-launcher
tags: [rust, fallback, clone3, pidfd, so-peercred, seccomp-shim, simulated]
status: complete
requires:
  - phase: 09-01
    provides: aok-shim fakes (clone3-enosys, ioctl-enotty), the ENOSYS-only fork+move+exec spawn fallback
  - phase: 09-02
    provides: whois --listen, the SO_PEERCRED plus /proc fallback, path= and window_ns report
  - phase: 09-03
    provides: the default-deny Landlock and seccomp layers these tests run under
provides:
  - tests/fallback.rs - spawn_fallback_window and identity_fallback_race, both shim-driven and labelled simulated
affects: [09-05, 09-06]
plan_head_before: eb9bd5385f5a83aed064edcbfb7ee24ba3619c14
plan_head_after: 8f9d1e06f8f567624e166f294129eef90e1b4c5c
actuals:
  tokens: 2200
  tasks: 2
  commits: 2
tech-stack:
  added: []
  patterns:
    - the identity race test shims the whois LISTENER only; one live agent task connects to two pathname sockets, one served by an unshimmed listener and one by an ENOTTY-shimmed listener, so both routes resolve the very same task
key-files:
  created:
    - tools/aok-launch/tests/fallback.rs
  modified: []
key-decisions:
  - "Both windows are recorded and printed, never gated: window_us on the spawn side, window_ns on the identity side"
duration: short
completed: 2026-11-05
---

# Phase 09 Plan 04: Simulated fallback paths Summary

Two shim-driven tests show the clone3-less spawn path and the PIDFD_GET_INFO-less identity path still reach the correct cgroup and the same agent_id, and record each fallback's window. Test-only: no src file was touched.

All results below are SIMULATED (D-13). clone3 and PIDFD_GET_INFO are both present on this host. The shim's seccomp filter makes a real syscall return a real errno, so the launcher takes its degraded branch for real, but nothing here is a native-degraded-kernel measurement.

## What was built

- `spawn_fallback_window`: runs the launcher under `aok-shim --fake clone3-enosys`. Asserts the child's `/proc/self/cgroup` is `<name>/main` (so the move happened before exec), that stderr reports `spawn_path=fallback`, and that a `window_us=` field is present (a number or `unobserved`). An unshimmed launch of the same shape reports `spawn_path=clone3` with no window, which proves the shim switched the path.
- `identity_fallback_race`: starts two `whois --listen` servers on pathname sockets in a scratch dir, one plain and one under `--fake ioctl-enotty` (listener only; the agent is not shimmed). One live agent task connects to both. Asserts the plain listener reports `path=pidfd` with no window, the shimmed one reports `path=fallback` with `window_ns`, and `agent_id`, `cgroup_id`, `parent` and `on_behalf_of` are identical across routes, with `agent_id` equal to the agent name and `cgroup_id` equal to the main leaf inode. It asserts nothing about the window size or about the race never occurring.
- The agent holds the default grant only. No `--grant` was needed: connecting to a pathname socket is not restricted by the 09-03 layers, and the client is `python3 -I -B -c`. An earlier draft carried an `rx` grant on the scratch dir; it was removed because the test passes without it and a grant that does nothing makes the test less honest.

## Recorded figures (one run each, SIMULATED via the shim, indicative only)

| Path | Value | Unit |
|------|-------|------|
| spawn fallback fork-to-move | window_us=325 | microseconds |
| identity fallback peer-pid to /proc read | window_ns=31951 | nanoseconds |

Single samples on this WSL2 host; not a distribution. Neither is gated.

## Deviations from Plan

### Plan/code mismatch (not an auto-fix)

**1. Spawn report unit.** The plan says the launcher reports the spawn-side window as `window_ns`. The code (`src/lib.rs`, the spawn report) prints `aok-launch: spawn_path=fallback window_us=<n>` on stderr, in microseconds, or `window_us=unobserved`. The test was written against what the code emits and records the unit as microseconds. The production output was not renamed. The identity side does emit `window_ns` on whois stdout, as planned.

Otherwise the plan executed as written. No Rule 1-3 fixes, no src edits, no auth gates, no checkpoints.

## Findings

None against 09-01 or 09-02. Both fallbacks landed the child in the right cgroup and resolved the same agent_id and cgroup_id as the pidfd route.

## Known Stubs

None.

## Threat Flags

None. T-09-10 (pid-reuse in the SO_PEERCRED fallback) is accepted as planned: the test reports the window and does not assert the race closed.

## Verification (plan gates, run literally, from the repo root)

| Gate | Exit |
|------|------|
| `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml --test fallback spawn_fallback_window -- --test-threads=1` | 0 (1 passed) |
| `grep -q 'clone3-enosys' tools/aok-launch/tests/fallback.rs && grep -qi 'simulat' tools/aok-launch/tests/fallback.rs` | 0 |
| `systemd-run ... --test fallback identity_fallback_race -- --test-threads=1` | 0 (1 passed) |
| `grep -q 'ioctl-enotty' tools/aok-launch/tests/fallback.rs && grep -q 'SO_PEERCRED\|peer' tools/aok-launch/tests/fallback.rs` | 0 |
| `systemd-run ... --test fallback -- --test-threads=1` (phase gate) | 0 (2 passed) |
| full crate suite, one delegated scope, `--test-threads=1` | 0: 54 lib + 39 integration (errno_contract 4, fallback 2, identity 14, lifecycle 3, registry_confinement 1, shellout 3, spawn 6, subset 5, xattr_forgery 1), 0 ignored |
| `cargo +1.99.1 fmt --manifest-path tools/aok-launch/Cargo.toml --check` | 0 |
| `cargo +1.99.1 clippy --manifest-path tools/aok-launch/Cargo.toml --all-targets -- -D warnings` | 0 |
| `grep -n unsafe tools/aok-launch/tests/fallback.rs` | 1 (no match, as wanted) |

## Commits

- e170c30 test(09-04): add the simulated spawn fallback window test
- 8f9d1e0 test(09-04): add the simulated identity fallback race test

## Not verified

- Behaviour on a real kernel without clone3 or PIDFD_GET_INFO. Only the shim simulation was run.
- The window figures are single samples and carry no statistical weight.

## Self-Check: PASSED

- tools/aok-launch/tests/fallback.rs exists; both tests listed in the run output.
- Commits e170c30 and 8f9d1e0 are ancestors of HEAD.
