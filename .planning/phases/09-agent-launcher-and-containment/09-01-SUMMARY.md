---
phase: 09-agent-launcher-and-containment
plan: 01
subsystem: agent-launcher
tags: [rust, cgroup-v2, clone3, pidfd, seccomp, freeze, kill, tracer]
requires:
  - phase: 08
    provides: locked launcher decisions (D-07 agent layout, D-12 root/sub role, D-13 kernel-fake shim)
provides:
  - tools/aok-launch crate: run, nest, freeze, thaw, kill, whois subcommands
  - clone3 CLONE_INTO_CGROUP spawn with ENOSYS-only fork fallback
  - delegated cgroup v2 subtree management (root evacuation, agent dir plus main leaf, sub-agents under parent agent)
  - aok-shim seccomp kernel-fake binary for fallback and layer tests
  - module seams for 09-02 (identity, landlock) and 09-03 (seccomp)
affects: [09-02, 09-03, 09-04]
actuals:
  tokens: 20300
  tasks: 3
  commits: 3
tech-stack:
  added: [libc =0.2.191, landlock =0.4.7, seccompiler =0.5.0, rust 1.99.1, edition 2024]
  patterns:
    - all raw syscalls confined to src/sys.rs with a SAFETY comment per unsafe block
    - pre-exec child failures reported over an O_CLOEXEC pipe (stage u8 plus errno i32 LE, EOF means exec succeeded)
    - tests use a per-test uniquely named agent with a Drop teardown that refuses to remove an ancestor of the test process
key-files:
  created:
    - tools/aok-launch/Cargo.toml
    - tools/aok-launch/Cargo.lock
    - tools/aok-launch/rust-toolchain.toml
    - tools/aok-launch/.gitignore
    - tools/aok-launch/src/lib.rs
    - tools/aok-launch/src/main.rs
    - tools/aok-launch/src/sys.rs
    - tools/aok-launch/src/cgroup.rs
    - tools/aok-launch/src/identity.rs
    - tools/aok-launch/src/landlock.rs
    - tools/aok-launch/src/seccomp.rs
    - tools/aok-launch/src/bin/aok-shim.rs
    - tools/aok-launch/tests/common/mod.rs
    - tools/aok-launch/tests/spawn.rs
    - tools/aok-launch/tests/lifecycle.rs
  modified: []
key-decisions:
  - "Launch role is derived from the launcher's own cgroup path (last component main means Sub, aok-mgr means Root(parent), otherwise Root(self)); a Sub launch never writes the delegated root."
  - "clone3 falls back to fork only on ENOSYS; any other clone3 errno propagates, so a seccomp EPERM is never masked."
  - "register_root runs after make_agent and before clone3, because 09-02 keys the identity entry on the agent dir's cgroup id."
  - "Freeze, thaw and kill act on the non-leaf agent dir so they reach sub-agents; the launcher refuses with EPERM when its own cgroup is inside the target subtree."
requirements-completed: [LNCH-01, LNCH-05]
coverage:
  - id: D1
    description: Agent spawned by clone3 CLONE_INTO_CGROUP lands in <agent>/main with no window outside the cgroup
    verification: tests/spawn.rs::spawn_into_cgroup
    status: verified
  - id: D2
    description: Sub-agent cgroup is a descendant of the parent agent cgroup
    verification: tests/spawn.rs::sub_is_descendant
    status: verified
  - id: D3
    description: Exit contract 64 usage, 125 refusal with errno named, otherwise the leader status
    verification: tests/spawn.rs::exit_contract
    status: verified
  - id: D4
    description: Agent name guard rejects names that collide with the layout
    verification: tests/spawn.rs::name_guard plus cgroup.rs unit tests
    status: verified
  - id: D5
    description: ENOSYS clone3 falls back to fork plus cgroup.procs write, reaching the cgroup
    verification: tests/spawn.rs::fallback_reaches_cgroup (under aok-shim clone3-enosys)
    status: verified
  - id: D6
    description: A non-ENOSYS clone3 error is not treated as a fallback trigger
    verification: tests/spawn.rs::clone3_eperm_is_not_a_fallback (under aok-shim clone3-eperm)
    status: verified
  - id: D7
    description: freeze stops the whole subtree, thaw resumes it, kill leaves populated=0 and removes the subtree
    verification: tests/lifecycle.rs::freeze_thaw_kill
    status: verified
  - id: D8
    description: kill of an agent with zero members succeeds
    verification: tests/lifecycle.rs::zero_member_kill
    status: verified
  - id: D9
    description: Launcher refuses to freeze or kill a cgroup that contains itself
    verification: cgroup.rs unit test launcher_never_targets_its_own_cgroup
    status: unit-only
  - id: D10
    description: Shim fakes ioctl-enotty and landlock-enosys behave as specified
    verification: manual python probe (landlock syscall gives 38, ioctl 0xc040ff0b gives 25)
    status: manual-only
duration: not captured (session start time was not recorded; wall-clock end 2026-11-05T09:55Z)
completed: 2026-11-05
status: complete
commits: 3
plan_head_before: 5c2d7911fcb688156dcc5fd8295494160f4760a5
plan_head_after: 3ded5e7cedef88a34ab901b592ef1cc2427fa301
---

# Phase 09 Plan 01: aok-launch spine Summary

**Rust launcher `tools/aok-launch` that spawns agents with clone3 CLONE_INTO_CGROUP into a delegated cgroup v2 subtree, with an ENOSYS-only fork fallback, freeze/thaw/kill over the whole agent subtree, an errno-faithful exit contract, and a seccomp shim that fakes an older kernel without any test branch in the launcher.**

## Performance

- **Duration:** not captured; start time was not recorded
- **Completed:** 2026-11-05
- **Tasks:** 3 of 3
- **Files:** 15 created, 0 modified (2460 insertions)

## Accomplishments

- Spawn tracer (Task 1): `aok-launch run` creates `<root>/<name>` with controllers enabled and a `main` process leaf, then clone3s the child straight into it. The id read back from the pidfd equals the leaf directory inode, so there is no window in which the child runs outside its cgroup.
- Root evacuation and role detection: the first launch moves every pid in the delegated root into `<root>/aok-mgr` before enabling `+memory +pids`, satisfying the no-internal-process rule. `nest` builds a chain of sub-agents under the parent agent directory (depth capped at 32).
- Lifecycle (Task 2): freeze and thaw poll `cgroup.events` (`frozen`); kill writes `cgroup.kill`, waits for `populated=0`, then removes the subtree deepest first with a bounded EBUSY-tolerant rmdir loop. A CPU-ticks witness proves the freeze: it stays valid under 09-03's default-deny layer because it needs no file write.
- Kernel-fake shim (Task 3): `aok-shim` installs seccompiler filters (one per errno) and execs the rest of its argv, so the unmodified launcher sees real ENOSYS or EPERM. No test-only branch ships in the launcher.
- Seams for later plans are fixed and compile: `identity::{resolve_role, register_root, whois}`, `landlock::{LayerSpec, LayerError, LayerErrorKind, preflight, apply}`, `seccomp::apply`.

## Task Commits

1. **Task 1: crate, cgroup spawn tracer** - `b7e8379` (feat)
2. **Task 2: freeze, thaw, kill and exit lifecycle tests** - `4463828` (test)
3. **Task 3: D-13 seccomp shim and clone3 fallback tests** - `3ded5e7` (feat)

Plan metadata commit: this SUMMARY, committed separately.

## Files Created/Modified

See `key-files` in the frontmatter. `src/sys.rs` is the only module with `unsafe` (20 blocks, each with a `// SAFETY:` comment). `src/identity.rs`, `src/landlock.rs` and `src/seccomp.rs` are compiling stubs by design.

## Decisions Made

See `key-decisions`. Additional: the launcher is single-threaded, so allocating in the clone3/fork child is acceptable; this is documented in the `sys.rs` module doc. `kill` prints `aok-launch: populated=0 agent=...` to stdout before the rmdir because the directory is gone afterwards and the tests need an observable.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Wider agent name guard**
- **Found during:** Task 1
- **Issue:** A sub-agent named `main` would collide with the process leaf; `aok-mgr`, `.`, `..` and `cgroup.*` names would collide with or escape the layout.
- **Fix:** `validate_name` also rejects `.`, `main`, `aok-mgr`, `cgroup.*` and any name containing `..` with EINVAL, and names over 128 bytes with ENAMETOOLONG.
- **Files modified:** `tools/aok-launch/src/cgroup.rs`
- **Commit:** `b7e8379`

**2. [Rule 2 - Missing critical functionality] Root evacuation refuses the hierarchy mount root**
- **Found during:** Task 1
- **Issue:** If the launcher's own cgroup resolved to the cgroup2 mount root, evacuation would move every process on the host.
- **Fix:** `evacuate_root` returns EPERM for the mount root.
- **Files modified:** `tools/aok-launch/src/cgroup.rs`
- **Commit:** `b7e8379`

**3. [Plan extension] Extra `clone3-eperm` shim fake and test**
- The plan lists three fakes. `clone3-eperm` is added, with the test `clone3_eperm_is_not_a_fallback`, to verify the truth that only ENOSYS triggers the fallback. It is test-only.
- **Commit:** `3ded5e7`

### Structure notes (not rule deviations)

- The code for Task 2 and Task 3 behaviour (`cgroup.rs` lifecycle, `sys.rs::fork_move`, the fallback and freeze/kill dispatch in `lib.rs`) was written whole in the Task 1 commit. The Task 2 and Task 3 commits hold the tests and the shim.
- `lib.rs` is the module root and the binary uses the library; `#![deny(unsafe_code)]` is set in `lib.rs`, `main.rs` and the shim.
- `register_root` is called after `make_agent` (see key-decisions).
- `Cargo.lock` was generated by cargo and committed.

## Out-of-scope changes

none

## Raised, not fixed

- The launcher does not forward SIGTERM or SIGINT to the leader. Needed before an agent is run under a supervisor.
- The LNCH-05 interleaving space (concurrent freeze, thaw and kill on one agent) is not exhaustively tested; each operation is tested alone.
- Evacuating a non-delegated session scope would move the whole session. Guarded only by the mount-root EPERM and the mkdir EACCES on an undelegated tree; a stronger "is this root delegated to me" check is not implemented.

## Known Stubs

| File | Reason | Resolved by |
|------|--------|-------------|
| `tools/aok-launch/src/identity.rs` | `register_root` and `resolve_role` are no-op seams; `whois` prints "unimplemented in 09-01" and returns 125 | 09-02 |
| `tools/aok-launch/src/landlock.rs` | `preflight` and `apply` are no-ops | 09-02 / 09-03 |
| `tools/aok-launch/src/seccomp.rs` | `apply` is a no-op | 09-03 |

These are intentional: the plan's goal (spawn, containment of lifecycle, exit contract) does not depend on them.

## Threat Flags

None. The new surface (cgroup writes under a delegated root, clone3, pidfd ioctl, shim seccomp filters) is the surface the plan's threat model covers.

## Issues Encountered

- Clippy `unnecessary_cast` on `libc::PIDFD_GET_INFO as u64`: removed the cast (`libc::Ioctl` is already u64 on this target).
- The acceptance greps for SAFETY count and for no bare `unsafe` in `lib.rs`, `cgroup.rs` and `main.rs` matched the word in doc comments; the comments were reworded and the checks then passed.
- The crates.io fetch of `landlock` and `seccompiler` succeeded, so no package-legitimacy checkpoint was needed.

## Verification

Verified (run, and passed):

- `cargo +1.99.1 test --lib --manifest-path tools/aok-launch/Cargo.toml`: 18 passed.
- Delegated integration suite under `systemd-run --user --scope -p Delegate=yes`, `--test-threads=1`: `spawn` 6 passed, `lifecycle` 3 passed.
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean.
- Acceptance greps: SAFETY comments >= unsafe blocks, no `unsafe` in `lib.rs`, `cgroup.rs`, `main.rs`, `CLONE_INTO_CGROUP: u64` present, and `0xc040ff0b` absent from non-comment lines of `lib.rs`.
- No leftover test cgroups after the run.
- Tracer gate: the automated verify was re-run end to end and passed before the expansion tasks.

Not verified:

- The refusal to freeze or kill the launcher's own cgroup is covered only by a unit test. There is no live integration test, by design: it would kill the test process's cgroup.
- The shim fakes `ioctl-enotty` and `landlock-enosys` were exercised only by a manual python probe, not by a crate test. 09-02, 09-03 and 09-04 consume them and will test them.
- No host other than this WSL2 machine (kernel 6.18.33.2, Landlock ABI 7). The fallback path is simulated by the shim, not observed on a real pre-5.7 kernel; it is labelled "simulated" accordingly.

## User Setup Required

None. Integration tests need a systemd user session that can delegate cgroups (`systemd-run --user --scope -p Delegate=yes`).

## Next Phase Readiness

09-02 can implement `identity` and the Landlock layer against the fixed seams. 09-03 fills `seccomp::apply`. The shim's `--fake` kinds are ready for 09-02 through 09-04.

## Self-Check: PASSED

All 15 files under `tools/aok-launch/` exist. Commits `b7e8379`, `4463828` and `3ded5e7` are ancestors of HEAD.
