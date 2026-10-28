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
| **Framework** | None for the bash/BPF parts. Rust `cargo test` (exact toolchain pinned in `tools/aok-adv/rust-toolchain.toml`) for the suite's pure logic. `python3 -I` scripts for `r1.py` and the report check. Existing: `python3 -I tools/linux-baseline/summarize.py --check RUN_DIR` (check_report.py hard-codes the linux-baseline.md headings and is not reusable) |
| **Config file** | none — Wave 0 creates `tools/aok-adv/Cargo.toml` and `rust-toolchain.toml` |
| **Quick run command** | `cd tools/aok-adv && cargo test` ; `python3 -I tools/linux-baseline/r1.py --self-test` ; `bash -n` on the wrapper |
| **Full suite command** | On the reference host: the wrapper run for each variant, then `python3 -I tools/linux-baseline/summarize.py --check <run dir>` |
| **Estimated runtime** | quick ~30 seconds; full run depends on the harness leg count (NOT MEASURED) |

---

## Sampling Rate

- **After every task commit:** Run the quick run command.
- **After every plan wave:** Run the quick run command plus any reference-host check whose host exists by then.
- **Before `/gsd-verify-work`:** Reference-host run directories exist and `summarize.py --check` passes on each.
- **Max feedback latency:** 60 seconds for L checks; R checks wait for the host (2026-11-04).

---

## Per-Task Verification Map

Rows are keyed by requirement; the Task ID column names the plan tasks (08-NN-TN) that carry each one. Every R check is a `checkpoint:human-action` in its plan: the user runs it on the reference host and the agent runs only the exit-status gate afterwards.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 08-03-T1, 08-04-T3 | 08-03, 08-04 | 1, 2 | ENV-01 | T-08-01, T-08-07, T-08-10, T-08-SC | root wrapper runs only checked-in sources; `--dry-run` executes nothing privileged; no agent writes env.txt | dry-run (L) + probe (R, root) | `bash tools/bpflsm/bpflsm.sh --dry-run` prints the loadall, setpriv, probe.bpf.o and lsm_hook_fires steps and leaves no `/sys/fs/bpf/aok` (L) ; on `results/ref-probe/env.txt`: `grep -q '^bpf_in_lsm=yes$'`, `grep -q '^attach_global_file_open=yes$'`, `grep -qE '^lsm_hook_fires=[1-9][0-9]*$'`; `attach_lsm_cgroup` is recorded, not gated (R) | ❌ W0 | ⬜ pending |
| 08-01-T3, 08-04-T3 | 08-01, 08-04 | 1, 2 | ENV-02 | T-08-06 | floor.c emits only kernel and feature facts | key writer (L) + probe (R) | `test "$(grep '^floor_' env.txt \| wc -l)" -eq 12` and 12 `floormethod_` lines, each probe or derived: 2 derived on the WSL2 smoke run (L), 1 derived on ref-probe after the probe sets `floormethod_bpf_lsm_cgroup=probe` (R) | ❌ W0 | ⬜ pending |
| 08-01-T2, 08-05-T1 | 08-01, 08-05 | 1, 3 | HARN-01 | T-08-11, T-08-13, T-08-14 | runs start from the user's login shell, never the agent's scope; hostname scrubbed before commit | selftest (L) + run (R) | `python3 -I tools/linux-baseline/transfer.py --self-test` (L) ; `summarize.py --check results/ref-harn01` passes and its `requested_` env.txt lines equal those of `20261028T034700Z` (all four suites, every config, rounds 1-5), then `transfer.py results/20261028T034700Z results/ref-harn01`; the boot matches the OQ2 Answer (bpf absent from `active_lsms` for yes, plus `ref-harn01-bpfboot`) (R) | ❌ W0 | ⬜ pending |
| 08-01-T1, 08-03-T2, 08-05-T2, 08-06-T1 | 08-01, 08-03, 08-05, 08-06 | 1, 3, 4 | HARN-02 | T-08-02, T-08-11, T-08-12 | sysctls + governor restored on exit; r1.py --bpflsm never revises S3 | selftest (L) + run (R, root) | `python3 -I tools/linux-baseline/r1.py --self-test` (L, includes the --bpflsm fixture) ; per variant `bpflsm-validity.txt` lines `hook= calls= hits= n= valid=`, recomputed so valid=yes needs calls >= n and hits > 0 (global-hit, anc) or hits = 0 (global-miss); nocount lines read `calls=na hits=na valid=denial-proof` (R) ; 08-06-T1 runs `r1.py --bpflsm` over the variant dirs and its first line is `informational under D-18; the S3 bound is not revised` (L) | ❌ W0 | ⬜ pending |
| 08-03-T2, 08-05-T2 | 08-03, 08-05 | 1, 3 | HARN-03 | T-08-03, T-08-11 | deny keys on the probe cgroup, never the timed cgroup and never globally | run (R, root) | `results/ref-harn03`: EPERM in the probe cgroup, success in the sibling, `lsm_cgroup_inherits=` recorded, `summarize.py --check` passes, rows C3-bpflsm-cg, -cgsib, -cgroot present; with the OQ7 Answer yes the objects were rebuilt with `make -B -C tools/bpflsm CG_SENDMSG=1`; otherwise the anc fallback with `attach_lsm_cgroup=no` in ref-probe | ❌ W0 | ⬜ pending |
| 08-02-T3, 08-05-T3 | 08-02, 08-05 | 1, 3 | ADV-01 | T-08-08, T-08-11 | suite never signals outside its own processes; the API-key case sends only to 127.0.0.1 | suite (L for cgroup-free classes, R otherwise) | `cd tools/aok-adv && cargo test` (L) ; `results/ref-adv/nonagent.jsonl`: one object per class with `class`, `oracle`, `baseline_ok`, `valid` (R) | ❌ W0 | ⬜ pending |
| 08-02-T1, 08-02-T2, 08-05-T3 | 08-02, 08-05 | 1, 3 | ADV-02 | T-08-04, T-08-05, T-08-11 | stub restarts before every row; only suite-spawned pidfd targets are signalled; ptrace_scope is read, never written | suite (L list-routes; R execution) | `aok-adv list-routes` prints 23 rows, 17 sibling and 6 ancestor (L) ; `results/ref-adv/env.txt` `yama_ptrace_scope` equals the OQ1 Answer and the 12 ptrace rows' reach matches the prediction: 0 all reachable; 1 sibling void (baseline failed), ancestor reachable; 2 or 3 all void (R) | ❌ W0 | ⬜ pending |
| 08-02-T2, 08-03-T3, 08-05-T3 | 08-02, 08-03, 08-05 | 1, 3 | ADV-03 | T-08-09, T-08-11 | N/A (the count is the finding) | suite (R, root) | `results/ref-adv/deny-counters.txt` lines `route=<r> topology=<t> hook=<h> before=<n> after=<n>`; `aok-adv report --merge-counters ... --in nonagent.jsonl --in agent.jsonl --out report.json` gives agent_result and deny_counter_delta per route and topology (R) | ❌ W0 | ⬜ pending |
| 08-04-T2 | 08-04 | 2 | DEC-01 (OQ1, OQ2, OQ7 inputs) | T-08-10 | the user types each Answer; no agent writes one | grep (L, after the user answers) | exactly 3 `Answer:` lines, all non-blank: the first 0-3, the second and third yes or no | ❌ W0 | ⬜ pending |
| 08-06-T2, 08-06-T3 | 08-06 | 4 | DEC-01 | T-08-15 | no agent writes a mark; unmarked rows refused | grep (L, after the user marks) | `test "$(grep -cE '^Mark: (accepted\|changed .+\|rejected\|deferred to Phase [0-9]+)[[:space:]]*$' 08-GATE.md)" -ge 17`, exactly 17 `Mark:` lines (counted with `wc -l`), and a dated `Marked on:` line | ❌ W0 | ⬜ pending |
| 08-06-T1 | 08-06 | 4 | DEC-02 | T-08-12, T-08-16 | the evidence fill writes no Mark and changes no Answer | grep (L) | the DEC-02 block keeps `Pre-committed: option 1, keep the S3 bound (D-18)`; after the fill, 17 blank Mark lines, 3 Answer lines, no placeholder Evidence line, and 23 S5 rows | ❌ W0 | ⬜ pending |
| 08-04-T1, 08-06-T3 | 08-04, 08-06 | 2, 4 | DEC-03 | T-08-10, T-08-15 | transcription refuses an unmarked row | grep (L) | `test "$(grep -c 'proposed, awaiting user acceptance' docs/architecture/scoping-linux.md)" -eq 0` (baseline 11); 10 dated mark-form Status cells (S1-S9, POSX-02); `docs/architecture/adr/0004-linux-prototype-first.md` line 3 still starts `- Status: **Proposed**` and cites bpflsm-reference.md | ❌ W0 | ⬜ pending |

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
| Reference-host measurement runs | HARN-01, HARN-02, HARN-03, ADV-01, ADV-02, ADV-03 | Privileged runs on a physical host; an agent cannot run them and must not fabricate rows | The user runs 08-05 Tasks 1-3 from a login shell and commits the scrubbed run dirs; the agent runs only the exit-status gates afterwards |
| Gate sheet marking | DEC-01, DEC-02, DEC-03 | The thresholds are the user's to accept; no agent may mark them | User marks `08-GATE.md`; 08-06 Task 3 transcribes the marks into scoping-linux.md and the ADR 0004 line-3 parenthetical (the status stays Proposed) |
| Answers to OQ1 (`ptrace_scope`), OQ2 (no-bpf boot entry) and OQ7 (per-cgroup sendmsg row) | HARN-01, HARN-03, ADV-02, ADV-03 | Threshold-reading and host-setup choices (research Open Questions 1, 2 and 7) | The user types each Answer on `08-GATE.md` (08-04 Task 2) before 08-05 runs; for OQ1 the user sets `kernel.yama.ptrace_scope` to the Answer before 08-05 Task 3 and restores it after; the wrapper never writes it |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s for L checks
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
