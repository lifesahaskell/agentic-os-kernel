---
phase: 01-userland-syscall-abi-and-ipc-v2
plan: 04
subsystem: kernel-cap
tags: [cspace, capabilities, KERN-05]
requires: [01-02]
provides: ["CSpace::MAX_SLOTS (65,536)", "growable CSpace with free list"]
affects: [01-06]
key-files:
  modified: [kernel/src/cap.rs]
decisions:
  - "CSpace grows on demand to MAX_SLOTS = 65,536; freed slots reused LIFO, lowest-free not guaranteed"
status: complete
commits: 2
plan_head_before: 18db3cdb79bedc3fe21a8ff7c6f2ddff8d0ae519
plan_head_after: 7bc21ef8af847552305145ce2272fa10a05f7fc8
actuals:
  tasks: 2
  commits: 2
---

# Phase 1 Plan 04: Growable CSpace Summary

`CSpace` now holds up to 65,536 capabilities in a growing `Vec<Option<Cap>>` with a free list; `insert`/`get`/`remove`/`new` keep their signatures, so `agent.rs` and `ipc.rs` are untouched.

## Commits

- 1cbb7ad feat(01-04): growable CSpace with free list, bounded by MAX_SLOTS (includes rewritten `cspace_slots`)
- 7bc21ef test(01-04): four new ktests for install/derive/revoke/child grants beyond slot 64

## Results

- ktests: 37 passed, 0 failed (includes `cspace_slots`, `cspace_grows_past_64_with_install_and_derive`, `cspace_reuses_freed_slots`, `revoke_reaches_copies_above_slot_64`, `spawn_child_grants_from_slot_above_64`).
- `sh scripts/check.sh`: all gates passed (unsafe_check, fmt, clippy, ktests, boot checks).
- `cspace_slots` fills the full `MAX_SLOTS` (65,536), not a test-only bound, by cloning one cap. Not timed separately; the whole `cargo osdk test` run took about 30 s under QEMU/KVM.

## Deviations from Plan

None in behavior. Process note: the RED step was compile-level only (the rewritten `cspace_slots` references `MAX_SLOTS`, absent at base); the tests were written together with the implementation and the first run was green. The new tests passed without needing any fix to `CSpace`.

## Known Stubs

None.

## Threat Flags

None. T-01-15 mitigated (`MAX_SLOTS` returns `Full`, freed slots reused first); T-01-16 mitigated (`derive` unchanged, attenuation and revocation asserted above slot 64).

## Self-Check: PASSED

- kernel/src/cap.rs modified, only file changed by this plan's commits (SUMMARY aside).
- Commits 1cbb7ad and 7bc21ef are ancestors of HEAD.
