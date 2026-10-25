---
phase: "2"
slug: "process-objects-teardown-and-timers"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-25"
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Derived from the
> Validation Architecture section of 02-RESEARCH.md. Anything that runs ktests or the boot check
> needs a KVM host; on a host without `/dev/kvm` only the static gates run.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | OSTD ktest via `cargo osdk test` (QEMU, `-smp 1`, isa-debug-exit), plus the boot check via `cargo osdk run` |
| **Config file** | `kernel/OSDK.toml`, `kernel/rust-toolchain.toml` |
| **Quick run command** | `cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'` (KVM) |
| **Full suite command** | `sh scripts/check.sh` (must end `check: all gates passed`); `sh scripts/check.sh --no-kvm` runs the static gates only |
| **Estimated runtime** | ~30 s ktests under KVM today; the 10,000x teardown loop adds to this and is unmeasured |

---

## Sampling Rate

- **After every task commit:** `sh scripts/check.sh --no-kvm` on any host; `cargo osdk test <filter>` on a KVM host.
- **After every plan wave:** full `sh scripts/check.sh` on a KVM host.
- **Before `/gsd-verify-work`:** full suite green on a KVM host.
- **Max feedback latency:** about 60 s for a targeted ktest filter (assumed, not measured).

---

## Per-Task Verification Map

Task IDs are filled in by the planner. Requirement-level checks:

| Requirement | Secure / correct behavior | Test Type | Automated Command | File Exists | Status |
|-------------|---------------------------|-----------|-------------------|-------------|--------|
| KERN-04 | Holder creates and interrupts a thread in another space; non-holder refused | ktest + ring-3 bin | `cargo osdk test thread_create` | ❌ W0 | ⬜ pending |
| KERN-04 | Rights bits round-trip through derive and the wire | ktest unit | `cargo osdk test rights` | ❌ W0 | ⬜ pending |
| KERN-07 | `pick` falls back to idle; idle is never counted as "others"; Tick preempts idle | ktest unit | `cargo osdk test sched` | ❌ W0 | ⬜ pending |
| KERN-07 | CPU halts: idle iterations over N ticks are about N | boot check | `check.sh` grep `aok: idle halt check passed` | ❌ W0 | ⬜ pending |
| KERN-08 | 10,000x fork+exit leaves page, CSpace, futex-table, deadline and registry counters at baseline | ktest (long) | `cargo osdk test teardown_loop` | ❌ W0 | ⬜ pending |
| KERN-08 | Dead process's futex waiters are woken; a stranded caller gets an error | ktest | `cargo osdk test teardown_wakes` | ❌ W0 | ⬜ pending |
| KERN-08 | Physical-frame probe (secondary), tolerance of one process footprint | ktest | `cargo osdk test frame_probe` | ❌ W0 | ⬜ pending |
| KERN-09 | Timed futex and `recv` waits expire within the stated tolerance | ktest | `cargo osdk test timeout` | ❌ W0 | ⬜ pending |
| KERN-09 | A timer delivers a message to an endpoint | ktest | `cargo osdk test timer_msg` | ❌ W0 | ⬜ pending |
| KERN-10 | Fault or exit in one of two forked siblings names the right `tid` and `tag` | ktest + bin | `cargo osdk test exc_names_tid` | ❌ W0 | ⬜ pending |
| KERN-11 | All GPR and XMM survive redirect + sigreturn; user stack bytes unchanged | ktest + bin | `cargo osdk test sigreturn` | ❌ W0 | ⬜ pending |
| AGNT-02 | `mmap` past the agent's budget returns `ENOMEM` from ring 3 | ring-3 bin | `cargo osdk test mmap_enomem` | ❌ W0 | ⬜ pending |
| AGNT-05 | CPU-exemption ledger exists with a documented limit; PROJECT.md wording does not overclaim | doc check | `grep -n "server-side" .planning/PROJECT.md` | manual | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Scheduler-protocol ktests: idle fallback, `update_current` truth table, `enqueue` preemption hint.
- [ ] Teardown counters module returning the primary counters, so the loop test can diff them.
- [ ] Ring-3 bins for fork+exit, `thread_create`, sigreturn and mmap overrun, in the Rust-bin style. **Depends on 01-07**, which deletes the B2 `PROGRAM` array and adds `abi::TEST_DATA` and `libsys::test_page`.
- [ ] Boot-check lines for idle halt and timer, plus their entries in the `for want in ...` list in `scripts/check.sh`.
- [ ] Extend `abi::errno` and the fuzz `KNOWN` list for any new errno.

Limit: the idle-halt check and any test needing the injected `BudgetScheduler` run only in the boot check, because `install()` cannot be called from a ktest.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| CPU-exemption decision and PROJECT.md wording | AGNT-05 | A wording judgement, not a runtime property | Read the ledger entry and the PROJECT.md core-value sentence; confirm neither claims kernel CPU limits cover server-side work |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency under 60 s for targeted filters
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
