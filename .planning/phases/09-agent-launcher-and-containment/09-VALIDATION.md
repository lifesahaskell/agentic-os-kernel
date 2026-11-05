---
phase: "09"
slug: "agent-launcher-and-containment"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-05"
---

# Phase 09 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from the Validation Architecture section of `09-RESEARCH.md`. The planner replaces the
> placeholder task IDs below with the real `09-NN-MM` IDs.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust) in the new `tools/aok-launch` crate; integration tests under `tools/aok-launch/tests/` |
| **Config file** | none beyond `Cargo.toml` and `rust-toolchain.toml` (copied from `tools/aok-adv`) — Wave 0 creates both |
| **Quick run command** | `cargo test -p aok-launch --lib` (builders and parsers, no cgroup needed) |
| **Full suite command** | `systemd-run --user --scope -p Delegate=yes -- cargo test -p aok-launch -- --test-threads=1` |
| **Estimated runtime** | quick ~10 s, full ~60 s (not yet measured) |

Integration tests need a delegated cgroup subtree and run serialized so cgroup teardown does not
race. A test never kills a cgroup that contains the test process.

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p aok-launch --lib`
- **After every plan wave:** Run the full suite command inside one delegated scope
- **Before `/gsd-verify-work`:** Full suite must be green; D-16 depth runs committed under `tools/linux-baseline/results/<run-id>/` with `env.txt`
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 09-xx-xx | — | — | LNCH-01 | — | child is born in its own cgroup via clone3; a sub-agent is a descendant | integration | `cargo test -p aok-launch spawn_into_cgroup` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-01 | — | fork, move, exec fallback runs and reports its spawn-time window (simulated: clone3 → ENOSYS) | integration | `cargo test -p aok-launch spawn_fallback_window` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-02 | — | a peer resolves the same agent_id from a pidfd | integration | `cargo test -p aok-launch identity_from_pidfd` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-02 | — | SO_PEERCRED fallback reports the pid-reuse race (simulated: ioctl → ENOTTY) | integration | `cargo test -p aok-launch identity_fallback_race` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-03 | — | a sub-agent cannot open a path its parent lacks, cannot drop a layer, runs with NoNewPrivs=1 | integration | `cargo test -p aok-launch subset_landlock subset_seccomp nnp` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-04 | — | an `sh -c` grandchild reports the agent's cgroup id and is refused an out-of-grant path | integration | `cargo test -p aok-launch shellout_inherits` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-05 | — | freeze gives `frozen 1` subtree-wide; kill reaps every member; exit observed via pidfd | integration | `cargo test -p aok-launch freeze_thaw_kill` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-06 | — | every denial returns a documented errno; the filter reads no pointer argument | unit + integration | `cargo test -p aok-launch errno_contract filter_no_ptr` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-02 (D-10) | — | a Landlocked same-uid process can forge a `user.*` xattr on a cgroup dir, so xattrs are rejected as identity | integration (negative control) | `cargo test -p aok-launch xattr_forgery` | ❌ W0 | ⬜ pending |
| 09-xx-xx | — | — | LNCH-03 (D-16) | — | openat read at 1, 2 and 4 stacked layers, reported "indicative, WSL2" | harness | `bash tools/linux-baseline/run.sh --run-id <id> --suites micro --wrap <aok-launch> --tag lldepth-N` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tools/aok-launch/Cargo.toml` and `rust-toolchain.toml` — copy the aok-adv pins; add `landlock` 0.4.7 and `seccompiler` 0.5.0
- [ ] `tools/aok-launch/tests/common/` — a delegated-scope and cgroup-cleanup fixture; every integration test creates its own leaf and tears it down
- [ ] the external seccomp shim test wrapper (D-13)
- [ ] an additive `--wrap` / `--tag` hook in `tools/linux-baseline/run.sh` for the D-16 depth reads (same file as Phase 8 plan 08-05; keep the edit additive)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `CLONE_NNP` success path | LNCH-03 | Arrives in kernel 7.1; this host is 6.18 and no shim can fake a kernel feature | Record "not exercised in Phase 9; prctl(PR_SET_NO_NEW_PRIVS) path tested natively". Re-run on a 7.1+ host. |
| Seccomp action cache engaged for the wide allowlist | LNCH-06 (D-04) | `CONFIG_SECCOMP_CACHE_DEBUG` is unset, so `/proc/PID/seccomp_cache` is absent | Keep D-04 marked ASSUMED; the cache is read from kernel source only. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
