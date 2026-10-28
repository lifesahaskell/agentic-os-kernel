---
phase: "08"
slug: reference-environment-early-falsifiers-and-threshold-gate
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-28"
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: `08-RESEARCH.md` section "Validation Architecture". Host column: **R** needs the reference
> host (does not exist until 2026-11-04); **L** runs on the WSL2 session host.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None for the bash/BPF parts. Rust `cargo test` (exact toolchain pinned in `tools/aok-adv/rust-toolchain.toml`) for the suite's pure logic. `python3 -I` scripts for `r1.py` and the report check. Existing: `tools/linux-baseline/check_report.py` |
| **Config file** | none — Wave 0 creates `tools/aok-adv/Cargo.toml` and `rust-toolchain.toml` |
| **Quick run command** | `cd tools/aok-adv && cargo test` ; `python3 -I tools/linux-baseline/r1.py --selftest` ; `bash -n` on the wrapper |
| **Full suite command** | On the reference host: the wrapper run for each variant, then `python3 -I tools/linux-baseline/check_report.py <run dir>` |
| **Estimated runtime** | quick ~30 seconds; full run depends on the harness leg count (NOT MEASURED) |

---

## Sampling Rate

- **After every task commit:** Run the quick run command.
- **After every plan wave:** Run the quick run command plus any reference-host check whose host exists by then.
- **Before `/gsd-verify-work`:** Reference-host run directories exist and `check_report.py` passes.
- **Max feedback latency:** 60 seconds for L checks; R checks wait for the host (2026-11-04).

---

## Per-Task Verification Map

Task IDs are assigned by the planner. Until then this table is keyed by requirement.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | ENV-01 | — | root wrapper runs only checked-in sources | probe (R) | `grep -E '^(bpf_in_lsm\|lsm_hook_fires\|bpftool_version\|attach_global_file_open)=' env.txt` ; `lsm_hook_fires` must be non-zero | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | ENV-02 | — | N/A | probe (R); key writer testable L | `grep -c '^floor_' env.txt` equals the scoping-linux.md section 3 row count | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | HARN-01 | — | N/A | run (R) | `python3 -I tools/linux-baseline/check_report.py <run dir>` ; transfer table has a verdict per S3, S4, S6 leg | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | HARN-02 | — | sysctls restored on exit | run (R) + selftest (L) | `python3 -I tools/linux-baseline/r1.py --selftest` (L) ; counter delta >= call count per leg (R) | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | HARN-03 | — | deny program keys on a test cgroup, never globally | run (R) | wrapper log: attach exit code, errno in attached cgroup, success in sibling | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | ADV-01 | — | suite never signals outside its own processes | suite (L for cgroup-free classes, R otherwise) | report JSON: one object per class with `class`, `oracle`, `baseline_ok` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | ADV-02 | — | stub restarts between kill attempts | suite (R for cgroup routes) | report per route: non-agent success | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | ADV-03 | — | N/A (the count is the finding) | suite (R) | report per route: agent-run success count + deny counter delta | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DEC-01 | — | N/A | grep (L, after the user marks) | `grep` for the date in each of the S1-S9 and POSX-02 Status cells and the ADR 0004 status line | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DEC-02 | — | N/A | grep (L) | decision block names the option, the S3 row and the run directories | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DEC-03 | — | N/A | grep (L) | `grep -n proposed docs/architecture/scoping-linux.md` shows no section 3 label left | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tools/aok-adv/` crate with a pinned toolchain and a compile probe for `libc::PIDFD_GET_INFO` and `libc::SO_PEERPIDFD` — ADV-01..03
- [ ] `tools/linux-baseline/r1.py` with a self-test that reproduces the linux-baseline.md section 11 numbers from run `20261028T034700Z` (`openat_tmpfs` p50 45) — HARN-02
- [ ] BPF sources and wrapper (buildable only where `bpftool btf dump` works: the reference host) — HARN-02, HARN-03
- [ ] `run.sh write_env` extension and its check — ENV-01, ENV-02
- [ ] `08-GATE.md` template, and a `grep`-based check for each DEC row — DEC-01..03

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Dual-boot install and probe | ENV-01 | Human, physical machine; BitLocker recovery key and disk layout are not visible to the agent | Locate the recovery key first; follow `08-RESEARCH.md` "Dual-boot hazards"; target 2026-11-04 |
| Gate sheet marking | DEC-01, DEC-02, DEC-03 | The thresholds are the user's to accept; no agent may mark them | User marks `08-GATE.md`; the plan transcribes the marks into scoping-linux.md section 4 and the ADR 0004 status line |
| Decision on `ptrace_scope` for S5 | ADV-02, ADV-03 | A threshold-reading choice (research Open Question 1) | Answer on the gate sheet before the suite is written |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s for L checks
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
