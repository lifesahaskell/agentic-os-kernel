---
phase: 09-agent-launcher-and-containment
plan: 02
subsystem: agent-launcher
tags: [rust, cgroup-v2, pidfd, identity, registry, so-peercred, landlock, xattr]
status: complete
requires:
  - phase: 09-01
    provides: aok-launch spine (run, nest, whois dispatch), cgroup layout, aok-shim ioctl-enotty fake, identity.rs stub
provides:
  - identity.rs - resolve chain from a pidfd to {agent_id, on_behalf_of, parent, cgroup_id}
  - launcher-written, validated, pruned registry keyed on the root agent's cgroup id
  - resolve_role registry half (D-08, D-12) and register_root
  - ENOTTY-only pid fallback that reports its pid-reuse window
  - aok-launch whois --pid | --pidfd | --listen
  - xattr_forgery negative control (D-10)
affects: [09-03, 09-04]
plan_head_before: 2be05d127bba03e4ab492322b8e5c5be99a5a7d1
plan_head_after: 82538f1382c597fbf7f4086c99d191b073afa532
actuals:
  tokens: 17255
  tasks: 3
  commits: 5
tech-stack:
  added: []
  patterns:
    - identity is the kernel-stamped cgroup id cross-checked against the directory inode; the path-derived agent_id is never trusted without that check
    - registry lookup is by the directory's own inode, never by the path stored in the registry
    - decision logic (decide_role, identity_from_cgroup_text) takes injected loader and stat closures so it is unit-tested without a cgroup
    - the Landlock probe in tests is a python3 -I -B ctypes script (no 09-03 code involved)
key-files:
  created:
    - tools/aok-launch/tests/identity.rs
    - tools/aok-launch/tests/xattr_forgery.rs
  modified:
    - tools/aok-launch/src/identity.rs
    - tools/aok-launch/src/sys.rs
key-decisions:
  - "agent_id is the agent directory relative to the root agent's parent (r7, r7/s1); a task in a non-main directory is attributed to that directory itself, so its agent_id is the whole path."
  - "The fallback is taken on ENOTTY only. Any other PIDFD_GET_INFO errno is exit 125 with the errno named."
  - "A re-launch of a registered root with a different on_behalf_of is refused EEXIST while the agent is populated and replaces the entry once it is empty."
  - "A duplicate cgroup_id line in the registry is skipped: the first entry wins."
requirements-completed: [LNCH-02]
coverage:
  - id: I1
    description: A peer resolves agent_id, on_behalf_of, parent and cgroup_id from a pidfd; cgroup_id equals the main leaf inode
    verification: tests/identity.rs::identity_from_pidfd
    status: verified
  - id: I2
    description: A second task moved into the agent's main leaf resolves the same identity
    verification: tests/identity.rs::a_second_task_in_the_subtree_resolves_the_same_agent
    status: verified
  - id: I3
    description: Moving a task to a sibling leaf in the agent's own subtree re-attributes its agent_id (RESEARCH item 9, ADV candidate, recorded not fixed)
    verification: tests/identity.rs::moving_a_task_within_the_subtree_reattributes_it
    status: verified
  - id: I4
    description: cgroup2 directory rename is EPERM and agent_id is unchanged (RESEARCH item 8, confirms D-06)
    verification: tests/identity.rs::cgroup_directory_rename_is_eperm
    status: verified
  - id: I5
    description: Pidfd path and pid fallback resolve the same cgroup_id, agent_id, parent and on_behalf_of, for a root and for a sub agent; the fallback reports path=fallback and window_ns
    verification: tests/identity.rs::fallback_equivalence, fallback_equivalence_for_a_sub_agent (SIMULATED - ioctl returns ENOTTY on a kernel that has the feature, via aok-shim ioctl-enotty)
    status: verified-simulated
  - id: I6
    description: Only ENOTTY selects the fallback; every other PIDFD_GET_INFO errno is a named failure
    verification: identity::tests::only_enotty_selects_the_fallback (unit, errno table)
    status: verified
  - id: I7
    description: Root launch from inside a registered agent tree is refused EACCES and writes nothing
    verification: tests/identity.rs::registry_root_from_inside_agent_is_eacces_a
    status: verified
  - id: I8
    description: A Landlocked process reaching a Root launch gets EACCES reading the registry (it sits outside any grant); the same launch without Landlock succeeds
    verification: tests/identity.rs::registry_root_from_inside_agent_is_eacces_b (python3 ctypes Landlock probe, not 09-03 code)
    status: verified
  - id: I9
    description: A sub launch cannot set on_behalf_of (EACCES, exit 125, nothing created) and never reads the registry
    verification: tests/identity.rs::sub_launch_cannot_set_on_behalf_of, identity::tests::sub_role_never_touches_the_registry_and_refuses_on_behalf_of
    status: verified
  - id: I10
    description: Malformed, oversized, traversal and duplicate registry lines are skipped with a warning and never trusted; a spoofed duplicate does not change on_behalf_of
    verification: tests/identity.rs::registry_malformed_line_skipped, identity::tests::parse_* (unit)
    status: verified
  - id: I11
    description: Registry file is 0600 in a 0700 directory built from the real uid; dead roots are pruned on the next root launch
    verification: tests/identity.rs::registry_is_private_and_pruned
    status: verified
  - id: I12
    description: whois --listen names the connecting task via SO_PEERPIDFD, and via SO_PEERCRED with path=fallback under the simulated ENOTTY
    verification: tests/identity.rs::whois_listen_names_the_connecting_task, whois_listen_falls_back_to_so_peercred_on_enotty (second is SIMULATED)
    status: verified-simulated
  - id: I13
    description: user.* xattrs on a cgroup directory are forgeable by a Landlocked same-uid process (overwrite, create, remove all succeed; final read ENODATA) while mkdir of a sibling or child cgroup and a cgroup.procs write are EACCES; identity never reads an xattr
    verification: tests/xattr_forgery.rs::xattr_forgery (the forgery path ran and passed on this host; the not-writable branch did not run)
    status: verified
---

# Phase 9 Plan 2: Agent identity and whois Summary

Pidfd to cgroup-id identity resolution with a validated, launcher-written registry for `on_behalf_of`, an ENOTTY-only pid fallback that reports its pid-reuse window, `aok-launch whois`, and a negative control showing `user.*` xattrs are forgeable under Landlock.

## What was built

- `src/identity.rs` (complete): `resolve` goes pidfd, `PIDFD_GET_INFO` cgroup id, `/proc/PID/cgroup` path, stat of that path with `st_ino == cgroup_id` required (a disagreement is a named `Mismatch` error, the D-06 falsifier), drop a trailing `main`, walk up to the nearest ancestor whose own inode is in the registry. `agent_id` is the path from the root agent, `parent` its parent directory.
- Registry at `/run/user/UID/aok-launch/registry.<boot_id>` (UID is the real uid, never `XDG_RUNTIME_DIR`), 0700 directory and 0600 file, tab-separated `cgroup_id path on_behalf_of`. Every line is validated on read, bad lines are skipped with a warning. `register_root` takes an exclusive lock, prunes dead roots and invalid lines, writes tmp then rename.
- `resolve_role` (D-08, D-12): Sub never touches the registry and refuses `--on-behalf-of` with EACCES; Root is refused EACCES when the registry read is EACCES or when any ancestor of its own cgroup is a registered root.
- `whois --pid | --pidfd | --listen SOCK`. `--pid` opens the pidfd first. `--listen` uses `SO_PEERPIDFD`, falling back to `SO_PEERCRED` on ENOTTY. Output is `key=value` lines ending in `path=pidfd` or `path=fallback` plus `window_ns=`.
- `tests/identity.rs` (14 integration tests), 19 identity unit tests, `tests/xattr_forgery.rs` (1 test).

Tracer gate: `identity_from_pidfd` passed end to end on the first slice before the expansion work. Tracer verified end-to-end, expanding.

## Verification

Run once at the end: `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1`.

- lib: 37 passed (9 from 09-01 cgroup, 9 from 09-01 lib, 19 new in identity)
- tests/identity.rs: 14 passed
- tests/lifecycle.rs: 3 passed, tests/spawn.rs: 6 passed (all 09-01 tests still green)
- tests/xattr_forgery.rs: 1 passed
- `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean
- no non-comment `unsafe` in `src/identity.rs`, `tests/identity.rs`, `tests/xattr_forgery.rs`

### Verified
- Everything in the coverage list with status `verified`, on this host (WSL2, kernel 6.18.33.2, Landlock ABI 7, cgroup v2 delegated scope).
- The xattr forgery actually happened: the initial `user.aok.agent_id` set succeeded on cgroup2, so the "not writable here" branch did not run.

### Not verified
- Real pre-6.13 kernels. Every fallback test is `simulated`: the shim makes `ioctl(PIDFD_GET_INFO)` return ENOTTY on a kernel that has the feature. The window_ns value therefore measures this machine's `/proc` read, not the race on an old kernel.
- Any non-ENOTTY errno from `PIDFD_GET_INFO` end to end (the shim only has ENOTTY). Covered by a unit table of errno classification only.
- Landlock via the launcher: `landlock.rs` is still the 09-01 stub (09-03). The Landlock coverage here (I8, I13) uses a python3 ctypes ruleset applied to the process, not a launcher-applied layer.
- Multiple simultaneous root launches racing on the registry lock (tests are single-threaded).
- A reused pid between pidfd-open and the `/proc` read on the fallback is reported (window_ns), not exercised.
- Boot-id rollover (a registry file from a previous boot is never read; not tested across a real reboot).

## Deviations from Plan

### Auto-fixed Issues

None needed fixing. One structural note and two small additions:

**1. [Rule 3 - Blocking] sys.rs wrappers**
- **Found during:** Task 1
- **Issue:** identity.rs needs `SO_PEERPIDFD` and the real uid, and `unsafe` is confined to `sys.rs`.
- **Fix:** added `peer_pidfd` and `real_uid`, each with a `// SAFETY:` comment.
- **Files modified:** `tools/aok-launch/src/sys.rs` (outside `files_modified`, listed below)
- **Commit:** 1f41a58

**2. [Plan ordering] Task 1 carried the whole identity.rs**
- The tracer commit (6aaa3c4) contains the registry, `resolve_role`, fallback and `--listen` code as well as the pidfd chain, because the module is one file and the call sites in `lib.rs` already reach all of it. Task 2 (14d737f) is the test commit for that code. The unit tests for the registry and role logic landed with the implementation.

**3. [Rule 2 - Missing coverage] Two plan-stated assertions added after Task 3**
- The plan's Task 1 text asks for a second task moved into the same subtree resolving the same identity, and for agent_id unchanged after a refused rename. Both were missing from the first test commit and were added in 82538f1.

### Plan wording not followed literally
- The plan puts the Landlocked-registry case in "registry_root_from_inside_agent_is_eacces". That is split into `_a` (registered tree, no Landlock) and `_b` (Landlock, no registered tree) because the launcher cannot apply Landlock until 09-03; `_b` uses a python3 ctypes ruleset.
- Per-plan "agent_id ends in the agent name" is stricter here: it equals the name for a root and `<root>/<name>` for a sub.

## Out-of-scope changes
- `tools/aok-launch/src/sys.rs`: added `peer_pidfd` (SO_PEERPIDFD) and `real_uid` (getuid) safe wrappers. Expected by the dispatch.

## Raised, not fixed
- `cgroup::validate_name` does not reject a tab or newline in an agent name. The registry writer refuses a path containing a control character (EINVAL) so the registry cannot be corrupted by it, but the launcher accepts the name earlier. Candidate for a 09-01 follow-up.
- RESEARCH item 9 (ADV candidate): an agent that can write its own subtree's `cgroup.procs` can re-label itself by moving into a sibling leaf. Resolution follows the cgroup by design; whether to deny the move belongs to 09-03/09-04 containment.
- Dead registry entries persist until the next root launch prunes them (a stale line for a removed agent stays on disk meanwhile; it cannot be matched because lookup is by live inode, but it is not tidied on agent exit).
- The test registry shares the real per-user registry file (no throwaway sibling), because the registry path is built from the real uid by design. Tests add and remove only their own lines, but a crashed test run leaves launcher-written lines for its dead agents until pruned.

## Known Stubs
None. `landlock.rs` and `seccomp.rs` remain the 09-01 seams owned by 09-03; this plan did not touch them.

## Threat Flags
None beyond the plan's threat model. New surface: the registry file and directory under `/run/user/UID/aok-launch` (0600/0700, uid-checked, validated on read) and the `whois --listen` Unix socket, which the caller names and the process removes on exit.

## Self-Check: PASSED
- Created files found: `tests/identity.rs`, `tests/xattr_forgery.rs`, this SUMMARY.
- Commits found as ancestors of HEAD: 1f41a58, 6aaa3c4, 14d737f, cd7f41f, 82538f1.
- `plan_head_before` 2be05d1; `git rev-list --count` gives 5 commits.
