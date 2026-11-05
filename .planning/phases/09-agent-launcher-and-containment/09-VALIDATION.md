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
| **Quick run command** | `cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml --lib` from the repo root (builders and parsers, no cgroup needed) |
| **Full suite command** | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1` from the repo root |
| **Estimated runtime** | quick ~10 s, full ~60 s (not yet measured) |

Integration tests need a delegated cgroup subtree and run serialized so cgroup teardown does not
race. A test never kills a cgroup that contains the test process.

---

## Sampling Rate

- **After every task commit:** Run `cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml --lib`
- **After every plan wave:** Run the full suite command inside one delegated scope
- **Before `/gsd-verify-work`:** Full suite must be green; D-16 depth runs committed under `tools/linux-baseline/results/09-lldepth/` with `env.txt`
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 09-01-01 | 09-01 | 1 | LNCH-01 | T-09-01 | child is born in its own cgroup via clone3; a sub-agent is a descendant built without writing the delegated root; run exits 64 on usage, 125 on refusal, else the leader's status | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 spawn_into_cgroup sub_is_descendant exit_contract` | ✅ | ✅ green |
| 09-04-01 | 09-04 | 4 | LNCH-01 | T-09-01 | fork, move, exec fallback runs and reports its spawn-time window (simulated: clone3 → ENOSYS) | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 spawn_fallback_window` | ✅ | ✅ green (simulated: shim-driven) |
| 09-02-01 | 09-02 | 2 | LNCH-02 | T-09-01 | a peer resolves the same agent_id from a pidfd | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 identity_from_pidfd` | ✅ | ✅ green |
| 09-02-02 | 09-02 | 2 | LNCH-02 (D-08) | T-09-05 | a root launch from inside an agent is refused EACCES; a malformed registry line is skipped; the ENOTTY-only fallback resolves the same agent_id (simulated: ioctl → ENOTTY) | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 registry_root_from_inside_agent_is_eacces registry_malformed_line_skipped fallback_equivalence` | ✅ | ✅ green (fallback_equivalence simulated: shim-driven) |
| 09-03-01 | 09-03 | 3 | LNCH-02 (D-08) | T-09-15 | an agent with only the default grants cannot open, append to or create in the registry dir under /run/user/UID/aok-launch/, reach it through /proc/self/root, or hold an fd to it; a control run outside the agent can | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml --test registry_confinement -- --test-threads=1 registry_unreachable_from_agent` | ✅ | ✅ green |
| 09-04-02 | 09-04 | 4 | LNCH-02 | T-09-10 | SO_PEERCRED fallback reports the pid-reuse race (simulated: ioctl → ENOTTY) | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 identity_fallback_race` | ✅ | ✅ green (simulated: shim-driven) |
| 09-03-01, 09-03-02, 09-03-03 | 09-03 | 3 | LNCH-03 | T-09-06 | a sub-agent cannot open a path its parent lacks, cannot remove or widen an inherited layer, runs with NoNewPrivs=1; an agent with only the default grants is denied everything else | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 subset_landlock subset_seccomp nnp zero_grant_denies cannot_remove_or_widen` | ✅ | ✅ green |
| 09-03-03 | 09-03 | 3 | LNCH-04 | T-09-06 | an `sh -c` grandchild reports the agent's cgroup id and is refused an out-of-grant path | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 shellout_inherits` | ✅ | ✅ green |
| 09-01-02 | 09-01 | 1 | LNCH-05 | T-09-02 | freeze gives `frozen 1` subtree-wide; kill reaps every member; exit observed via pidfd | integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 freeze_thaw_kill` | ✅ | ✅ green |
| 09-03-02 | 09-03 | 3 | LNCH-06 | T-09-08 | every denial returns a documented errno, including EPERM for a foreign-arch (i386 int 0x80) call; the filter reads no pointer argument | unit + integration | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 errno_contract arch_mismatch_eperm filter_no_ptr` | ✅ | ✅ green |
| 09-03-03 | 09-03 | 3 | LNCH-01..06 (D-12) | T-09-06 | the whole crate (every 09-01 and 09-02 test plus 09-03's own) is green under default-deny in one delegated scope; an earlier test is adjusted only by a --grant on a path it creates or by moving its own scratch output, never by widening the default grant or weakening an assertion | integration (merged suite) | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1` | ✅ | ✅ green (93 tests, 0 failed; no grant adjustment was needed) |
| 09-02-03 | 09-02 | 2 | LNCH-02 (D-10) | T-09-03 | a Landlocked same-uid process can forge a `user.*` xattr on a cgroup dir, so xattrs are rejected as identity | integration (negative control) | `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1 xattr_forgery` | ✅ | ✅ green |
| 09-05-01 | 09-05 | 5 | LNCH-03 (D-16) | T-09-11 | with no --wrap, the edited run.sh gives the same taskset argv and exit status as run.sh at df2d6b2 | harness | `make -C tools/linux-baseline && bash tools/linux-baseline/check-wrap-identity.sh df2d6b2` | ✅ | ✅ green (re-run in 09-06, exit 0) |
| 09-05-02 | 09-05 | 5 | LNCH-03 (D-16) | T-09-14 | openat read at 1, 2 and 4 stacked layers, reported "indicative, WSL2" | harness | `systemd-run --user --scope -q -p Delegate=yes -- bash tools/linux-baseline/run.sh --run-id 09-lldepth --suites micro --configs C0 --tag lldepth-N --wrap "$PWD/tools/aok-launch/target/release/aok-launch nest --name lldepth-N --depth N --grant rx:$PWD/tools/linux-baseline/bin --grant rw:/dev/shm --grant rw:$PWD/tools/linux-baseline/.work --grant rw:$PWD/tools/linux-baseline/results/09-lldepth --"` (N = 1, 2, 4, after an unwrapped C0 into the same run id) | ✅ | ✅ green (committed artifact, indicative, WSL2, no S3 verdict; not re-taken in 09-06) |
| 09-06-01 | 09-06 | 6 | LNCH-06 (D-01, D-05) | T-09-08 | 09-ALLOWLIST.md records the table source, the x32 bit, the ENOSYS branch, the statmount/listmount discretion, the A1 glibc result and clone3 435, checked against seccomp.rs | doc | `F=.planning/phases/09-agent-launcher-and-containment/09-ALLOWLIST.md; test -f "$F" && grep -q 'syscall_64.tbl' "$F" && grep -q '0x40000000' "$F" && grep -q 'ENOSYS' "$F" && grep -q 'statmount' "$F" && grep -q 'glibc' "$F" && grep -q '435' "$F"` | ✅ | ✅ green (doc greps run, exit 0) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tools/aok-launch/Cargo.toml` and `rust-toolchain.toml` — copy the aok-adv pins; add `landlock` 0.4.7 and `seccompiler` 0.5.0 — **produced by 09-01-01** (Wave 1)
- [ ] `tools/aok-launch/tests/common/` — a delegated-scope and cgroup-cleanup fixture; every integration test creates its own leaf and tears it down — **produced by 09-01-01** (Wave 1)
- [ ] the external seccomp shim test wrapper (D-13) — **produced by 09-01-03** (Wave 1)
- [ ] an additive `--wrap` / `--tag` hook in `tools/linux-baseline/run.sh` for the D-16 depth reads (same file as Phase 8 plan 08-05; keep the edit additive), and `tools/linux-baseline/check-wrap-identity.sh`, the gate that proves it — **produced by 09-05-01** (Wave 5)

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
