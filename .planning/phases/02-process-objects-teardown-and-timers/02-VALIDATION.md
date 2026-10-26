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
> Validation Architecture section of 02-RESEARCH.md. The execution host has no `/dev/kvm`: ktests
> and the boot check run under QEMU TCG through `cargo osdk test` / `cargo osdk run`, and that is
> the runtime evidence. `sh scripts/check.sh --no-kvm` runs the static gates only. Every SUMMARY
> labels its results TCG evidence; the KVM re-run is deferred (02-CONTEXT.md Deferred Ideas).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | OSTD ktest via `cargo osdk test` (QEMU, `-smp 1`, isa-debug-exit), plus the boot check via `cargo osdk run` |
| **Config file** | `kernel/OSDK.toml`, `kernel/rust-toolchain.toml`. `kernel/OSDK.toml` has no timeout key. |
| **Quick run command** | `cd /home/rakdos/repos/agentic-os-kernel/kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test <filter>'` (QEMU TCG on this host) |
| **Full suite command** | `cd /home/rakdos/repos/agentic-os-kernel/kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'` under TCG, plus `cd /home/rakdos/repos/agentic-os-kernel && python3 -I tools/unsafe_check.py && sh scripts/check.sh --no-kvm` (must end `check: static gates passed`). The full `sh scripts/check.sh` (`check: all gates passed`) needs a KVM host and is the deferred re-run. |
| **Estimated runtime** | Unmeasured under TCG. The 10,000x loops (`teardown_loop_same_agent_10000x`, `retire_loop_child_agent_10000x`) each print `iterations=N jiffies=T`, and the 02-06 and 02-07 SUMMARYs record those wall times and the runner timeout that applied (Pitfall 22) |

---

## Sampling Rate

- **After every task commit:** `sh scripts/check.sh --no-kvm` and `cargo osdk test <filter>` under QEMU TCG on this host.
- **After every plan wave:** the full `cargo osdk test` under TCG, plus `sh scripts/check.sh --no-kvm`.
- **Before `/gsd-verify-work`:** the full suite green under TCG; the KVM run of `sh scripts/check.sh` stays deferred.
- **Max feedback latency:** about 60 s for a targeted ktest filter (assumed, not measured; TCG is slower than KVM).
- **Runner timeout:** the executor's 600 s foreground command cap, or an outer `timeout` around the command. A full suite that exceeds it moves the 10,000x loops to a `QUICK_ITERATIONS` default and runs each 10,000x loop once by name (02-07 Task 3).

---

## Per-Task Verification Map

Every ktest command below is `cd /home/rakdos/repos/agentic-os-kernel/kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test <filter>'` under QEMU TCG on this host; the table names only the filter. Each plan's last task runs the full suite.

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
| 02-11 T0 | 3 | KERN-08 | `osdk-frame-allocator` and `osdk-heap-allocator` 0.19.0 confirmed on their crate pages before `kernel/Cargo.toml` changes (blocking checkpoint, never auto-approved) | manual checkpoint | none; `sh tools/pin_check.sh` gates the pins once T1 lands | manual | ⬜ pending |
| 02-11 T1 | 3 | KERN-08 | A1 spike (D-06): both allocator counters read non-zero, one frame moves `probe::free()` by exactly 4,096 bytes, slab growth is neutral; the fallback is taken and recorded if A1 fails | ktest (spike) | filter `frame_probe_spike_` (`frame_probe_spike_counts_one_frame`, `frame_probe_spike_slab_growth_is_neutral`) | ❌ W0 | ⬜ pending |
| 02-11 T2 | 3 | KERN-08 | `probe::settled()` (park, settle, read) returns to the same value after an address space's whole life; the activation pin is released | ktest | filter `frame_probe_` (`frame_probe_settle_returns_an_unmapped_frame`, `frame_probe_activation_pin_released`), full suite | ❌ W0 | ⬜ pending |
| 02-06 T0-T1 | 4 | KERN-08 | 10,000x fork+exit leaves pages and counters at baseline (checkpoint OD-04/07/08 first) | ktest (long) | filter `teardown_` | ❌ W0 | ⬜ pending |
| 02-06 T2 | 4 | KERN-08 | Dead process's futex waiters are woken; a stranded caller gets an error | ktest | filter `kill_`, full suite | ❌ W0 | ⬜ pending |
| 02-06 T3 | 4 | KERN-08 | Fallible `cow_clone` (D-05, D-07): node frames charged by the root/3/1 rule, a partial clone unwinds (flush, uncharge, drop), bad ranges are `CloneError::Range` / `Unsupported`, not a panic | ktest | filter `cow_clone_` (`cow_clone_partial_failure_unwinds`, `cow_clone_rejects_bad_ranges`, `cow_clone_charge_matches_frames`, unchanged `cow_clone_isolates_writes_and_charges_the_writer`), full suite | ❌ W0 | ⬜ pending |
| 02-07 T1 | 5 | KERN-08 | Child agent retires with its last counted Space; CSpace drained and closed; derived grants survive; carve returns once, only at `children == 0`, cascading; roots never retire (D-01, D-02, D-08) | ktest (long) | filter `retire` (includes `retire_keeps_derived_grants`, `retire_returns_carve`, `root_never_retires`, `retire_waits_for_children`, `retire_loop_child_agent_10000x`), full suite | ❌ W0 | ⬜ pending |
| 02-07 T2 | 5 | KERN-08 | Staging bytes and kernel CoW breaks charged to the process (D-04); futex lookup returns ENOMEM or EFAULT through `From<AgentError>`, never panics (D-05, D-09) | ktest | filters `staging_`, `futex_cow_` | ❌ W0 | ⬜ pending |
| 02-07 T3 | 5 | KERN-08 | Armed timers die with the process; both 10,000x loops keep every counter flat and fail if `probe::settled()` drops by more than one measured footprint F (D-03, D-06) | ktest (long) | filters `frame_probe_`, `counts_`, full suite | ❌ W0 | ⬜ pending |
| 02-08 T1-T3 | 6 | KERN-11 | All GPR and XMM survive redirect + sigreturn; user stack bytes unchanged; nesting limit | ktest + bin | filter `sigreturn_`, full suite | ❌ W0 | ⬜ pending |
| 02-09 T1 | 7 | AGNT-02 | `mmap` past the agent's budget returns `ENOMEM` from ring 3, mapping and charging nothing | ring-3 bin + ktest | filter `vmem_` (`vmem_enomem_past_budget_from_ring3`) | ❌ W0 | ⬜ pending |
| 02-09 T2-T3 | 7 | AGNT-02 | munmap/mprotect/brk return exact pages; 1,000x mmap loop flat; fuzz covers new errnos | ktest + bin | filter `vmem_`, full suite | ❌ W0 | ⬜ pending |
| 02-10 T1-T3 | 8 | KERN-04 (LIBC-04 calls, verified in Phase 6) | arch_prctl FS base, exit-and-unmap, `clear_tid` zeroed and woken at exit | ktest + ring-3 bin | filter `lifecycle_`, full suite | ❌ W0 | ⬜ pending |

AGNT-02's `SIGXCPU` leg has no row: OD-12 assigns it to SIG-03.

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Scheduler-protocol ktests: idle fallback, `update_current` truth table, `enqueue` preemption hint.
- [ ] Free-frame probe `kernel/src/probe.rs` (`free`, `settle`, `park_activation`, `settled`) and the A1 spike (02-11 T1), before any gate depends on it. If A1 fails, the D-06 fallback is taken and recorded in 02-11-SUMMARY.md.
- [ ] Teardown counters: `space::counts()` and its `pub(crate)` readers `thread::{live_threads, deadline_count, fpu_owner_count}`, `futex::table_len`, `ipc::live_reply_waits`, `timer::armed_count`, `space::{live_spaces, dropped_spaces}`, `agent::live_agents`, so the loop tests can diff them (02-07 T1, T3). No static becomes `pub`.
- [ ] Probe-gate test helpers in the space.rs test module: `footprint()` (one measured process footprint F) and `assert_probe_flat` (02-07 T3), reused by 02-09's 1,000x loop.
- [ ] `space_with_at`, a vm.rs test helper that maps one page at each listed address, for the two-address `cow_clone` budget test (02-06 T3).
- [ ] Loop timing (Pitfall 22): each 10,000x loop prints `iterations=N jiffies=T`, and each SUMMARY states the runner timeout; `QUICK_ITERATIONS` is the fallback when the full suite exceeds it.
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
