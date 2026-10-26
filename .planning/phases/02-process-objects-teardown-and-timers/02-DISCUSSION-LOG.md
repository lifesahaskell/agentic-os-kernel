# Phase 2: Process objects, teardown and timers - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-26
**Phase:** 02-Process objects, teardown and timers
**Areas discussed:** Teardown and leak proof

---

## Gray area selection

Offered: Teardown and leak proof; Server CPU exemption (AGNT-05); Space and Thread object model;
Notification shape and signal ABI. **Selected:** Teardown and leak proof only. The other three
stay as the assumed decisions in `02-OPEN-DECISIONS.md` and were recorded as signed off without
discussion.

Pre-answered and not offered: OD-01, OD-02, OD-03 (roadmap "Decisions due"), OD-10 (Phase 1
complete), OD-12 (roadmap mapping rule).

---

## Teardown and leak proof

### Q1: Revoke a retiring agent's outgoing grants?

| Option | Description | Selected |
|--------|-------------|----------|
| Drop only, no revoke | Delegations outlive the delegator; revocation stays explicit | ✓ |
| Revoke outgoing grants on retire | Walk the agent's forwarders and kill them; needs a forwarder registry | |
| Process-level only | No agent teardown this phase; fails KERN-08 text | |

**User's choice:** Drop only, no revoke.
**Notes:** Verified before asking that capability transfer is a `derive` copy (`ipc.rs:455`). The
option text wrongly said "seL4/EROFS-style"; corrected in the next message (EROFS is an errno).

### Q2: Trigger for agent retirement (OD-19)

| Option | Description | Selected |
|--------|-------------|----------|
| Auto on last Space | As assumed | ✓ |
| Explicit retire call | Lives until a capability holder retires it; leaks on a crashed parent | |
| Auto, with a pin count now | Auto-retire at zero Spaces and zero pins | |

**User's choice:** Auto on last Space.

### Q3: Free-frame probe as gate or report (OD-08)

| Option | Description | Selected |
|--------|-------------|----------|
| Probe is a hard gate | Fails past one process footprint; settle loop for RCU | ✓ |
| Counters gate, probe reports | No flakiness; misses leaks the counters cannot see | |
| Gate on both, tune on first run | Tolerance from a measured baseline | |

**User's choice:** Probe is a hard gate.

### Q4: RR-01 / RR-03 carry-forward (not in any plan)

| Option | Description | Selected |
|--------|-------------|----------|
| Make cow_clone fallible now | Result, partial-clone unwind, real errno at `futex.rs:88` | ✓ |
| Prove the range, keep the panics | Document and debug-assert | |
| Defer to Phase 4 | Leave until ring-3 fork | |

**User's choice:** Make cow_clone fallible now.
**Notes:** Grep of the ten plans found no mention of RR-01 or RR-03. Adds tasks to 02-06 and 02-07.

### Q5: More questions?

OD-07 and OD-15 accepted as assumed. Moved on.

---

## Claude's Discretion

Probe tolerance value and settle-loop length; `cow_clone` error type; the ABI tables in
`02-OPEN-DECISIONS.md`.

## Deferred Ideas

- Revoke-on-retire of outgoing grants.
- Agent pin count for daemon agents.
- Re-running Phase 1 and 2 runtime evidence on KVM.
