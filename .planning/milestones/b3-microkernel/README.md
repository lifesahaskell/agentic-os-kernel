# Archived milestone: B3 POSIX core on the microkernel

Archived 2026-10-27. This directory is a record; nothing in it is live planning.

## What the milestone was

B3 turned the B2 spike kernel (a Rust capability microkernel on Asterinas OSTD 0.19.0, chosen in
[ADR 0002](../../../docs/architecture/adr/0002-rust-microkernel.md)) into a system meant to run
unmodified POSIX software while an agent stayed a kernel object: a ring-3 syscall ABI and IPC v2,
process objects and teardown, then vfs, exec, procmgr, signals and tty as userspace servers, then
musl, BusyBox `ash` and an os-test gate. It held 73 v1 requirements in 6 phases.

## Why it stopped

On 2026-10-27 Corey dropped the OSTD/B2 microkernel path and chose Linux integration
([ADR 0004](../../../docs/architecture/adr/0004-linux-prototype-first.md), status Proposed;
[scoping-linux.md](../../../docs/architecture/scoping-linux.md)). The microkernel work is frozen at
git tag `archive/ostd-b2-spike`, commit `1f1333e` ("docs(pause): phase 02 paused at UAT test 4 of
5"). ADR 0002 and ADR 0003 stay as records.

## State at archive

| Phase | State | Evidence |
|---|---|---|
| 1. Userland, syscall ABI and IPC v2 | Complete 2026-10-26, 8/8 plans | `.planning/phases/01-userland-syscall-abi-and-ipc-v2/` |
| 2. Process objects, teardown and timers | Paused and superseded. 11/11 plans executed and merged; UAT stopped at test 4 of 5: tests 1-3 passed, tests 4 (`cow_clone` huge-page refusal) and 5 (ring-3 kill mid-run) pending. Verification status `human_needed`; the phase was never marked complete. Test 1's KVM re-run was reported by the user, not observed by Claude. | `.planning/phases/02-process-objects-teardown-and-timers/02-UAT.md`, `.continue-here.md`, `HANDOFF.json` |
| 3. Bootstrap, server runtime and vfs | Never started; dropped | none |
| 4. exec and procmgr | Never started; dropped | none |
| 5. Signals, sessions and tty | Never started; dropped | none |
| 6. libc, BusyBox, conformance and measurement | Never started; dropped | none |

Requirements at archive: 20 of 73 Complete, 53 Pending (traceability table in the copied
REQUIREMENTS.md). Two Phase 2 requirements were still Pending: KERN-07 (idle task) and AGNT-05
(server CPU-exemption record), both marked Pending in the traceability table at the time of the copy.

Phase 7 (recursion and cheap-inference spikes) was on the B3 roadmap as a post-MVP item. It is
not archived here: it stays on the live roadmap as a separate track, with its dependencies
restated there.

## Files

- `ROADMAP.md`: byte-for-byte copy of `.planning/ROADMAP.md` as of 2026-10-27, before the rewrite.
- `REQUIREMENTS.md`: byte-for-byte copy of `.planning/REQUIREMENTS.md` as of the same date.

The phase directories were not moved; they stay under `.planning/phases/`. `kernel/` and
`userland/` stay in the tree (ADR 0004, Consequences).
