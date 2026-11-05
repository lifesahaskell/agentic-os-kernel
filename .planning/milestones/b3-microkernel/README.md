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
| 2. Process objects, teardown and timers | Paused and superseded. 11/11 plans executed and merged; UAT stopped at test 4 of 5: tests 1-3 passed, tests 4 (`cow_clone` huge-page refusal) and 5 (ring-3 kill mid-run) pending. Verification status `human_needed`; the phase was never marked complete. Test 1's KVM re-run was reported by the user, not observed by Claude. | `.planning/phases/02-process-objects-teardown-and-timers/02-UAT.md`, `continue-here-phase-02.md`, `HANDOFF.json` (both in this directory) |
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
- `HANDOFF.json`: the Phase 2 pause record (UAT stopped at test 4 of 5), moved here 2026-11-05.
  The dotless names below are deliberate: the resume workflow globs for `.continue-here*.md` and
  would otherwise pick these up as live checkpoints.
- `continue-here-root.md`: the 2026-10-24 project-init checkpoint (formerly `.planning/.continue-here.md`).
- `continue-here-phase-01.md`, `continue-here-phase-02.md`: the per-phase checkpoints (formerly
  `.continue-here.md` in each phase directory).

The phase directories were not moved; they stay under `.planning/phases/`. `kernel/` and
`userland/` stay in the tree (ADR 0004, Consequences).

## Open concerns at archive

Moved out of the live `STATE.md` on 2026-11-05. They belong to the B3 microkernel path and are not
tracked against the Linux-prototype phases.

### Decisions

- [Roadmap]: 6 phases, not the researched 8. Research phases 1+2 and 7+8 merged; Phase 6 is the largest (16) and splits after SHELL-04 if planning finds it too big.
- [Roadmap]: Phases 1-5 use Rust ring-3 test binaries; the C os-test suites first run in Phase 6.
- [Roadmap]: Requirement maps to the phase that builds its primary component; later legs are separate requirements.
- [Phase 1]: 01-01: abi path dependency resolves under OSDK; Message is Clone (not Copy) carrying Arc<ReplyWait>; sys_call rejects buffer/cap fields until later plans.

### Blockers and concerns

- [Phase 1]: All Phase 1 runtime evidence is QEMU TCG, not KVM. OSDK 0.19.0 never emits `-enable-kvm` and OSDK.toml sets no accelerator, so earlier "QEMU/KVM" labels (b2-spike-report.md line 14, 01-04 SUMMARY) are unverified. The WSL2 guest has no `svm` flag, so there is no `/dev/kvm`. IPC-06 (median 5.43M cycles, p99 7.54M, N=10,000, as recorded in ipc-measurements.md; other TCG runs gave 5.45M/7.34M and 5.79M/8.37M) is provisional. Re-run 01-01..01-07 ktests, boot checks and IPC-06 on a KVM host, and decide on `-enable-kvm` in OSDK.toml (the `-cpu Icelake-Server` model may not run under KVM on AMD).
- [Phase 1]: User-mode FPU/SSE under OSTD 0.19.0 is the highest-risk unknown. Order: safe route, then contained `unsafe` (ledgered, SAFE-01), then Redox re-score only if both fail or an OSTD patch is needed.
- [Phase 1]: OSTD 0.19.0 docs.rs build failed; read the source for `boot_info()`, initramfs and serial RX.
- [Phase 2]: Carry-forward from the Phase 1 verification. RR-01: `cow_clone` unwraps and asserts become ring-3 reachable when procmgr fork or spawn calls it (plans 02-06, 02-07); make it fallible or prove the range. RR-03: `futex.rs:88` maps every `handle_cow_fault` error to `FutexError::Budget`. The three frame-allocation failure branches (01-08) are covered by inspection only; OSTD 0.19.0 has no failure injection.
- [Phase 3]: Open decisions need Corey's sign-off: read-only grant errno (`EROFS` vs `EACCES`) and cross-domain `kill` errno (`ESRCH` vs `EPERM`). One ADR amending option-b.
- [Phase 4]: musl is the default libc on source-level evidence only; nothing was built. The Phase 4 spike confirms or overturns it.
- [Phase 6]: KVM on the CI host is assumed, not checked.
- [Docs]: Kernel gap IDs (K1-K12) in REQUIREMENTS.md do not match the K-numbering in research/ARCHITECTURE.md (only K10 agrees). Use requirement IDs, not K-numbers.
