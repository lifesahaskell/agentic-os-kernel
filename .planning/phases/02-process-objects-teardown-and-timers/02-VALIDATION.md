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

Every ktest command below is `cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test <filter>'` on a KVM host; the table names only the filter. Each plan's last task runs the full suite.

| Plan / Tasks | Wave | Requirement | Secure / correct behavior | Test Type | Automated Command | File Exists | Status |
|--------------|------|-------------|---------------------------|-----------|-------------------|-------------|--------|
| 02-01 T1 | 1 | KERN-07 | CPU halts: idle iterations over N ticks are about N | boot check | `sh scripts/check.sh`, line `aok: idle halt check passed` | ❌ W0 | ⬜ pending |
| 02-01 T2 | 1 | KERN-07 | `pick` falls back to idle; idle is never counted as "others"; Tick preempts idle | ktest unit | filter `sched_` | ❌ W0 | ⬜ pending |
| 02-01 T3 | 1 | KERN-07 | OSTD, image, cargo-osdk and nightly pins agree | static | `sh tools/pin_check.sh && sh tools/pin_check.sh --self-test` | ❌ W0 | ⬜ pending |
| 02-02 T1-T2 | 1 | KERN-09 | Timed futex and `recv` waits expire within the stated tolerance | ktest + ring-3 bin | filters `timeout_futex`, `timeout_recv` | ❌ W0 | ⬜ pending |
| 02-02 T3 | 1 | KERN-09 | A timer delivers a message to an endpoint | ktest | filter `timer_msg` | ❌ W0 | ⬜ pending |
| 02-03 T0-T1 | 1 | AGNT-05 | Per-client request ledger with a limit; EAGAIN past it for one tag only (checkpoint OD-05 first) | ktest + ring-3 bin | filter `request_ledger` | ❌ W0 | ⬜ pending |
| 02-03 T2 | 1 | AGNT-05 | ADR 0003 records the exemption; PROJECT.md does not claim CPU limits cover server-side work | doc check | `grep -c "server-side work" .planning/PROJECT.md` | manual | ⬜ pending |
| 02-04 T1-T2 | 2 | KERN-04 | Holder creates and interrupts a thread in another space; non-holder refused | ktest + ring-3 bin | filter `thread_create` | ❌ W0 | ⬜ pending |
| 02-04 T3 | 2 | KERN-04 | Rights include CREATE and SIGNAL; 64-thread limit; credential invariant; fuzz | ktest | filters `rights_`, `space_credential_matches_owner`, full suite | ❌ W0 | ⬜ pending |
| 02-05 T1-T3 | 3 | KERN-10 | Fault or exit in one of two forked siblings reaches the space endpoint naming `tid` and `tag` | ktest + bin | filter `note_`, full suite | ❌ W0 | ⬜ pending |
| 02-06 T0-T1 | 4 | KERN-08 | 10,000x fork+exit leaves pages and counters at baseline (checkpoint OD-04/07/08 first) | ktest (long) | filter `teardown_` | ❌ W0 | ⬜ pending |
| 02-06 T2 | 4 | KERN-08 | Dead process's futex waiters are woken; a stranded caller gets an error | ktest | filter `kill_`, full suite | ❌ W0 | ⬜ pending |
| 02-07 T1-T2 | 5 | KERN-08 | Child agent retires with its last process; staging and kernel CoW breaks charged | ktest (long) | filters `retire_`, `staging_` | ❌ W0 | ⬜ pending |
| 02-07 T3 | 5 | KERN-08 | Armed timers die with the process; counters and frame probe flat | ktest | filters `counts_`, `frame_probe_`, full suite | ❌ W0 | ⬜ pending |
| 02-08 T1-T3 | 6 | KERN-11 | All GPR and XMM survive redirect + sigreturn; user stack bytes unchanged; nesting limit | ktest + bin | filter `sigreturn_`, full suite | ❌ W0 | ⬜ pending |
| 02-09 T1 | 7 | AGNT-02 | `mmap` past the agent's budget returns `ENOMEM` from ring 3, mapping and charging nothing | ring-3 bin + ktest | filter `vmem_` (`vmem_enomem_past_budget_from_ring3`) | ❌ W0 | ⬜ pending |
| 02-09 T2-T3 | 7 | AGNT-02 | munmap/mprotect/brk return exact pages; 1,000x mmap loop flat; fuzz covers new errnos | ktest + bin | filter `vmem_`, full suite | ❌ W0 | ⬜ pending |
| 02-10 T1-T3 | 8 | KERN-04 (LIBC-04 calls, verified in Phase 6) | arch_prctl FS base, exit-and-unmap, `clear_tid` zeroed and woken at exit | ktest + ring-3 bin | filter `lifecycle_`, full suite | ❌ W0 | ⬜ pending |

AGNT-02's `SIGXCPU` leg has no row: OD-12 assigns it to SIG-03.

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Scheduler-protocol ktests: idle fallback, `update_current` truth table, `enqueue` preemption hint.
- [ ] Teardown counters module returning the primary counters, so the loop test can diff them.
- [ ] Ring-3 bins for fork+exit, `thread_create`, sigreturn, mmap overrun (`mmap`, 02-09) and thread lifecycle (`lifecycle`, 02-10), in the Rust-bin style. **Depends on 01-07**, which deletes the B2 `PROGRAM` array and adds `abi::TEST_DATA` and `libsys::test_page`.
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
