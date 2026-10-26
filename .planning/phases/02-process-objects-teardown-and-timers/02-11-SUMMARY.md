---
phase: 02-process-objects-teardown-and-timers
plan: 11
subsystem: kernel-test-probe
tags: [ktest, free-frame-probe, osdk-allocators, rcu-settle, activation-pin, D-03, D-06, KERN-08]
requires:
  - phase: 02-01
    provides: tools/pin_check.sh and the exact OSTD pin this plan extends
  - phase: 02-04
    provides: last plan to edit kernel/src/lib.rs before this one
provides:
  - "cfg(ktest) probe::free(): osdk_frame_allocator free bytes + osdk_heap_allocator slab bytes, proven live by an exact one-frame delta (A1 holds)"
  - "probe::settled(): the park, settle, read protocol the D-03 leak gates in 02-07 and 02-09 call before each read they compare"
  - "osdk-frame-allocator and osdk-heap-allocator =0.19.0 as direct aok dependencies, pinned in Cargo.toml and Cargo.lock by pin_check.sh"
affects: [02-06, 02-07, 02-09]
tech-stack:
  added: [osdk-frame-allocator 0.19.0, osdk-heap-allocator 0.19.0]
  patterns:
    - "leak gates compare two probe::settled() reads, never two raw free() reads"
    - "ktest filters are a path suffix match and take ONE name per run"
key-files:
  created: [kernel/src/probe.rs]
  modified: [kernel/Cargo.toml, kernel/Cargo.lock, kernel/src/lib.rs, tools/pin_check.sh]
key-decisions:
  - "A1 holds, so the D-06 fallback (probe-by-exhaustion) was not taken and the D-03 gate keeps its independence from the teardown code"
  - "SETTLE_ROUNDS stays 8: the measured delta is 0 from the first round, so 8 is margin, not a measured threshold"
requirements-completed: [KERN-08]
status: complete
actuals:
  tokens: 2500
  tasks: 3
  commits: 3
plan_head_before: 4298e59d31beab8154a133ac420083fc9a10c82c
plan_head_after: d35cb8ca196941f078c06ab4c34c11c06aeb2ac2
coverage:
  - id: D1
    description: "probe::free() reads the OSDK base crate's own allocator counters (A1): both non-zero, one frame moves the probe by exactly 4096 bytes, dropping it restores the value"
    requirement: KERN-08
    verification:
      - kind: integration
        ref: "cargo osdk test frame_probe_spike_counts_one_frame (QEMU TCG)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Slab growth is neutral to the probe (slab grew 20480 bytes, probe unchanged)"
    requirement: KERN-08
    verification:
      - kind: integration
        ref: "cargo osdk test frame_probe_spike_slab_growth_is_neutral (QEMU TCG)"
        status: pass
    human_judgment: false
  - id: D3
    description: "settle returns the frames of a dropped, unmapped user space (delta 0 after settle)"
    requirement: KERN-08
    verification:
      - kind: integration
        ref: "cargo osdk test frame_probe_settle_returns_an_unmapped_frame (QEMU TCG)"
        status: pass
    human_judgment: false
  - id: D4
    description: "park_activation releases the per-CPU activated-space pin: settled() equals the baseline after a space was activated and dropped"
    requirement: KERN-08
    verification:
      - kind: integration
        ref: "cargo osdk test frame_probe_activation_pin_released (QEMU TCG)"
        status: pass
    human_judgment: false
  - id: D5
    description: "pin_check.sh pins both probe crates in Cargo.toml and Cargo.lock, with self-test rounds"
    requirement: KERN-08
    verification:
      - kind: other
        ref: "sh tools/pin_check.sh && sh tools/pin_check.sh --self-test"
        status: pass
    human_judgment: false
duration: 35min
completed: 2026-10-26
---

# Phase 2 Plan 11: Free-frame probe, A1 spike and settle protocol Summary

**A cfg(ktest) free-frame probe that reads the OSDK base crate's own frame and slab counters (A1 proven by an exact 4,096-byte one-frame delta), plus a park-settle-read protocol that makes two reads around an address space's whole life agree to the byte.**

## Task 0: crate legitimacy checkpoint (T-02-SC)

**Answer: `approved`**, given by the user in the session that resumed this plan. An earlier executor stopped at this `blocking-human` checkpoint with no commits and gathered the evidence below on 2026-10-26. I did not re-fetch the crates.io pages; the table is that executor's evidence, relayed in my dispatch prompt.

| Check | `osdk-frame-allocator` 0.19.0 | `osdk-heap-allocator` 0.19.0 |
|-------|------|------|
| Version exists, not yanked | yes | yes |
| Repository | github.com/asterinas/asterinas | github.com/asterinas/asterinas |
| Owner and publisher | StevenJiang1110 | StevenJiang1110 |
| Same account as `ostd` 0.19.0 | yes | yes |
| Publish time vs `ostd` | all three within 10 s on 2026-10-12 | same |
| Licence | MPL-2.0 | MPL-2.0 |
| Recent downloads | ~2,000 (ostd 1,824) | ~2,000 |
| sha256 | 1a3da8b8f6c7bc557dd9f3acb5608e21b072f32734550f6492091bbdb7091e07 | 759767a54d7634b22ed44a1003cff5e9d9d1b7f5ec41a9299a536228b4172a12 |

Also confirmed by the orchestrator: cargo-osdk's generated base crate (`kernel/target/osdk/aok-run-base/Cargo.toml`) already depends on both crates.

**Caveat, not checked: the crate source was not diffed against the git tag.** The SUS flag in 02-RESEARCH.md was on download count only, and the numbers above make it look stale, but "same publisher, same repository, already linked into the image" is not a source audit.

What I did verify myself: the `checksum` lines `cargo` wrote into `kernel/Cargo.lock` equal the two sha256 values above (`1a3da8b8...` and `759767a5...`), so the build fetched the same artifacts the checkpoint evidence describes.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 0 | Crate legitimacy checkpoint | approved by user, no commit |
| 1 | Tracer and A1 spike (probe::free, deps, pin_check) | 815fc4b |
| 2 | Settle and park (TDD) RED | a41ad74 |
| 2 | Settle and park (TDD) GREEN | d35cb8c |

REFACTOR: none needed, no commit.

## A1 result: holds

Observed in `frame_probe_spike_counts_one_frame` (QEMU TCG, 2 GiB guest):

- `osdk_frame_allocator::load_total_free_size()` = 2,098,737,152 and 2,091,208,704 in two runs; non-zero.
- `osdk_heap_allocator::load_total_slab_size()` = 368,640 and 409,600; non-zero.
- One `FrameAllocOptions::new().alloc_frame()`: `free()` went 2,099,105,792 to 2,099,101,696, a delta of exactly 4,096 (`PAGE_SIZE`). Dropping the frame restored the earlier value exactly.
- `frame_probe_spike_slab_growth_is_neutral`: 128 boxed 256-byte values grew the slab from 368,640 to 389,120 (+20,480, five pages) and `free()` did not move. The test asserts the slab grew, so it cannot pass vacuously. Dropping them left `free()` unchanged.

So `aok`'s copy of each crate is the base crate's registered instance, and the D-06 fallback was not taken. The D-03 gate keeps its independence from the teardown code.

`kernel/Cargo.lock` gained exactly two packages: `osdk-frame-allocator` 0.19.0 and `osdk-heap-allocator` 0.19.0. Their own dependencies (`ostd`, `paste`, `log`) were already locked.

## Settle calibration (Task 2)

`frame_probe_settle_returns_an_unmapped_frame` builds a space, maps one fresh frame at 0x40_0000, unmaps it with a TLB flush, drops the space, then logs the missing bytes (`b` minus `free()`) after cumulative settle rounds.

| Cumulative rounds | Delta, RED (settle is a no-op) | Delta, GREEN |
|---|---|---|
| 1 | 4096 | 0 |
| 2 | 4096 | 0 |
| 4 | 4096 | 0 |
| 8 | 4096 | 0 |
| 16 | 4096 | 0 |

The same five zeros appeared in the full unfiltered run. `SETTLE_ROUNDS` stays at **8**. The curve is flat from round 1, so the data does not show that 8 is the threshold; 8 is margin over a 1-round minimum, from one scenario (one mapped page, one boot CPU, QEMU TCG). Whether 8 is enough for 02-07's 10,000x loop is for that plan's own run to show, and if its two reads disagree, raise the constant first.

Pinned delta, logged before the park in `frame_probe_activation_pin_released`: **20,480 bytes** (five pages: the root, three page-table nodes for the first page, and the data frame), as the plan predicted. It is unchanged by `settle` alone and released by `park_activation`: `settled()` then equals the baseline exactly.

## TDD gate compliance (Task 2)

- RED (`a41ad74`): the tests ran against stubs (`settle` and `park_activation` doing nothing, `settled() == free()`). Both target tests executed and failed on the planned assertion: `frames still missing after 16 rounds` with delta 4096, and `the park did not release the pinned space` with delta 20480. Not a build failure, not zero tests, not unexpected green.
- GREEN (`d35cb8c`): both pass.
- `workflow.tdd_mode` is not enabled in this project's config, and ktest output is not a format `gsd_run check tdd-red-evidence` supports (TAP, JUnit, swift-testing, unittest, go test). I did not run the classifier. The RED assessment above is my own semantic inspection of the real run output, not a machine verdict.

## Deviations from Plan

**1. [Plan defect - ktest filter] `cargo osdk test frame_probe_spike_` and `... probe::test` match nothing.** The runner builds a suffix trie over the full path, so a filter must be a path suffix, in practice the test function name. `probe::test` ran `0 passed; 99 filtered out`. Also, `cargo osdk test` takes exactly one name: a second positional is a CLI error. I ran each `frame_probe_*` test by its exact name, and ran the full unfiltered suite once for the combined log. The plan's `<automated>` filtered commands (`... test frame_probe_spike_` and `... test frame_probe_`) cannot work as written. No source change.

**2. [Plan defect] `<automated>` paths point at the main checkout.** Ran the root-relative equivalents in this worktree.

**3. [Rule 1 - Bug] `println!` in OSTD does not capture inline variables.** `println!("... {x}")` fails to compile (`there is no argument named`). Switched my logging to positional arguments. Fixed before the first commit; no behavior change.

**4. [Rule 3 - Blocking] `Cargo.lock` was not written by `cargo osdk test`.** The ktest build resolves in the generated base crate, so `kernel/Cargo.lock` stayed unchanged until `cargo osdk clippy` (run in `kernel/`) updated it. Same end state the plan wanted; the plan's "run `cargo osdk build`" step is not the command that does it.

**5. [Rule 2 - hardening] Added a non-vacuity assertion** to `frame_probe_spike_slab_growth_is_neutral`: it asserts the slab actually grew. Without it, 128 allocations absorbed by existing free slab slots would pass without testing anything.

Total deviations: 5 (2 plan defects, 1 Rule 1, 1 Rule 3, 1 Rule 2). Impact: none on the plan's truths.

## Verification evidence

**All runtime evidence is QEMU under TCG. This host has no /dev/kvm.**

- Full unfiltered `cargo osdk test` (after `rm -rf target/osdk`): `test result: ok. 101 passed; 0 failed; 0 filtered out.` This tree does not include plan 02-05's work, which merges separately.
- `grep -c 'frame_probe: settle rounds=' ` on that log: 5.
- `sh tools/pin_check.sh` prints `pin_check: ok`; `--self-test` prints `pin_check: self-test ok` (it now has two more `Cargo.toml` rounds and two more `Cargo.lock` rounds for the probe crates).
- `python3 -I tools/unsafe_check.py` prints `unsafe_check: ok`; `docs/architecture/unsafe-ledger.md` is unchanged (`git diff --quiet` exits 0).
- `cargo fmt --check` exits 0. `cargo osdk clippy` exits 0 with no clippy warnings. The only `warning:` lines in the build output are the existing rustc `-Ctarget-feature` `ermsb` notice from `aok-abi` and the kernel crate, not from this plan.
- `sh scripts/check.sh --no-kvm` ends `check: static gates passed; LOCAL-ONLY (needs /dev/kvm): cargo osdk test, cargo osdk run boot checks`. It does not compile `cfg(ktest)` code, so the ktest build above is the only compile check of `probe.rs`.
- Acceptance greps: Cargo.toml pin lines 2, Cargo.lock package entries 2, `pin_check.sh` mentions the heap crate 1, `cfg(ktest)` above `pub mod probe` 1, `pub fn free` 1, `settle|park_activation|settled` fns 3, `SETTLE_ROUNDS` uses 4, `fn frame_probe_` 4.

**Still needs the deferred KVM re-run:** the ktest run and the boot checks (`cargo osdk run`) on a KVM host. RCU grace periods advance on task switches, not wall time, so I expect the settle curve to carry over, but I have not measured it under KVM. I did not re-run `cargo osdk run` boot checks at all.

## Known Stubs

None. The RED stubs were replaced in `d35cb8c`.

## Threat Flags

None. The probe is `#[cfg(ktest)]` (T-02-59, checked by the `pub mod probe` grep); no `unsafe`, no OSTD patch, no allocator registered from `aok`. T-02-SC rests on the Task 0 approval and its source-not-diffed caveat. T-02-57 is closed by the exact one-frame delta. T-02-58 is mitigated by `settled()`, with the caveat that the settle length is calibrated on one scenario.

## Self-Check: PASSED

- kernel/src/probe.rs, kernel/Cargo.toml, kernel/Cargo.lock, kernel/src/lib.rs, tools/pin_check.sh present.
- Commits 815fc4b, a41ad74, d35cb8c are ancestors of HEAD.
