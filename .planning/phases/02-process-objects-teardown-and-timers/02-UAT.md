---
status: testing
phase: 02-process-objects-teardown-and-timers
source: [02-VERIFICATION.md]
started: 2026-10-26T16:21:02Z
updated: 2026-10-27T20:50:28Z
---

## Current Test

number: 4
name: cow_clone huge-page refusal
expected: |
  Map a 2 MiB page in a parent range, call cow_clone over it: Err(Unsupported(va)), agent budget unchanged, no leaked node frames. vm.rs:94-98 is covered by inspection only.
awaiting: user response

## Tests

### 1. KVM re-run of the full ktest suite and boot checks
expected: `./osdk.sh cargo osdk test`, then `sh scripts/check.sh` without --no-kvm, on a KVM host: 181 passed, 0 failed; boot checks pass; timing tolerances hold. All Phase 2 runtime evidence so far is QEMU TCG (this host has no /dev/kvm).
result: pass

### 2. Judgment-tier prohibitions in the 02-xx PLAN must_haves
expected: A person resolves each one (all are status unresolved). Verifier spot checks found no violation: no unsafe added, no kernel write to the user stack on redirect, ledger keys on the kernel tag.
result: pass

### 3. x87 control word and MXCSR across sigreturn
expected: Redirect a ring-3 thread with a non-default x87 control word and MXCSR, run a handler that changes both, sigreturn, read both back: both hold the pre-redirect values. No test reads them today.
result: pass

### 4. cow_clone huge-page refusal
expected: Map a 2 MiB page in a parent range, call cow_clone over it: Err(Unsupported(va)), agent budget unchanged, no leaked node frames. vm.rs:94-98 is covered by inspection only.
result: [pending]

### 5. Ring-3 kill mid-run
expected: Confirm kill_stops_a_running_spinner kills a thread executing in ring 3 (Exit::Killed, space torn down, counts flat), or add a ktest.
result: [pending]

## Summary

total: 5
passed: 3
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
