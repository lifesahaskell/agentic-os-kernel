# Phase 2: Process objects, teardown and timers - Context

**Gathered:** 2026-10-26
**Status:** Planned. Eleven plans (02-01..02-11) reflect D-01 through D-09; 02-11 was added for D-06.

<domain>
## Phase Boundary

Space and Thread capability objects, `thread_create` and `thread_interrupt` from ring 3, exit and
fault notifications naming `tid` and `tag`, leak-free process and agent teardown, an idle task,
timed futex and `recv` waits, a timer that messages an endpoint, a kernel-held saved-context
stack with `sigreturn`, cap-less self-memory calls (`mmap`, `munmap`, `mprotect`, `brk`), and the
LIBC-04 kernel calls. Requirements: KERN-04, KERN-07, KERN-08, KERN-09, KERN-10, KERN-11,
AGNT-02, AGNT-05. The ring-3 kill call, procmgr and the `SIGXCPU` leg belong to Phases 4-5.

</domain>

<decisions>
## Implementation Decisions

The 19 numbered decisions (OD-01..OD-19) with full wording, syscall numbers, types, rights, labels
and limits live in `02-OPEN-DECISIONS.md`. They were written by the planner as assumptions. This
file records which are now signed off and how. Read both.

### Discussed and decided in this session: teardown and leak proof

- **D-01 (OD-04, refined):** Teardown has two levels, process (Space) then agent, as assumed. When
  a child agent retires, its CSpace is drained and its carved budgets return to the parent. The
  kernel does **not** revoke grants the dead agent derived to others (a server, a sibling, a
  grandchild); those derived copies survive. Verified in `kernel/src/ipc.rs:455`: capability
  transfer is `derive`, a copy, so the parent's originals never move and "returned to the parent"
  means "the child's copies are dropped". Revocation stays an explicit act by a holder of the
  revoke right. — **Reversibility:** costly — procmgr (Phase 4) and the vfs server build on "a
  delegation outlives its delegator"; adding revoke-on-retire later changes the CSpace drain, the
  forwarder registry and the 10,000x loop's failure paths.
- **D-02 (OD-19, confirmed):** A child agent retires automatically when its last counted Space is
  torn down. No explicit retire call and no pin count in this phase. A root agent never retires
  automatically. Consequence: an agent with zero processes between commands is destroyed unless a
  long-lived process holds it. Phase 4 or 7 adds a pin only if a daemon-agent case appears. —
  **Reversibility:** reversible — a pin count is an additive field and method on `Agent`.
- **D-03 (OD-08, confirmed and tightened):** The 10,000x fork+exit loop **fails** if the free-frame
  probe is low after the loop by more than one process footprint. Counters are the primary check
  (live threads, live and dropped Spaces, live agents, agent pages used, CSpace live slots,
  futex table keys, deadlines, armed timers, parked replies). The probe is the only check
  independent of the teardown code, so it is a gate, not a report. Add a settle loop (yields)
  before the second count to absorb RCU-delayed frees. No tracked frame-meta module, so the Kernel
  table of the unsafe ledger stays empty. — **Reversibility:** reversible.
- **D-04 (OD-07, OD-15, accepted as assumed):** Image, stack and IPC staging pages are charged to
  the owning agent and uncharged in teardown (closes the 01-05 SUMMARY hole and the `ipc.rs:510`
  shortcut). `Space::kill` is a kernel API driven by ktests; the ring-3 kill call arrives with
  procmgr. — **Reversibility:** reversible.
- **D-05 (new, RR-01 and RR-03 carried from the Phase 1 verification):** Make `cow_clone`
  fallible in this phase. It returns `Result`, unwinds a partial clone (uncharges the pages
  already copied, drops the half-built child), and replaces the `unwrap()` calls and the huge-page
  `assert_eq!` (`kernel/src/vm.rs:30-48`). `futex.rs:88` stops mapping every `handle_cow_fault`
  error to `FutexError::Budget` and returns the real errno. A ktest forces a partial-clone failure
  with a tiny budget. No Phase 2 plan covers this today: add a task to 02-06 (the clone and
  unwind) and one to 02-07 (the charge/uncharge accounting and the futex mapping). The three
  frame-allocation failure branches from 01-08 remain covered by inspection only, because OSTD
  0.19.0 has no failure injection. — **Reversibility:** reversible.

### Decided after the 2026-10-26 research refresh (02-RESEARCH.md "Open decisions needing the user")

- **D-06 (OD-08 amended):** The free-frame probe reads `osdk_frame_allocator::load_total_free_size()`
  plus `osdk_heap_allocator::load_total_slab_size()`, added as direct `aok` dependencies. OD-08's
  "no new crate" no longer holds for these two. Both were flagged SUS by the legitimacy check (low
  downloads only; asterinas repo). The plan must include a Wave 0 spike proving `aok` builds with
  them and shares the OSDK base crate's counters, and a `checkpoint:human-verify` on the two
  crates. If the spike fails, fall back to counters plus probe-by-exhaustion and record that the
  D-03 gate lost its independence from the teardown code. — **Reversibility:** reversible.
- **D-07 (D-05 refined):** `cow_clone` charges the child's page-table node frames only, through a
  `&dyn Payer` (root first, then 3 for the first page, then 1 per newly crossed 2 MiB, 1 GiB or
  512 GiB window), so a tiny budget forces a mid-clone failure. The shared data frames stay charged
  to the parent: a documented under-charge gap in 02-06. The existing ktest
  `cow_clone_isolates_writes_and_charges_the_writer` stays as written. Error type `CloneError`
  (Range, Unsupported(Vaddr), Budget). — **Reversibility:** reversible.
- **D-08 (D-01 and D-02 refined):** A retiring child agent returns its carve to the parent only when
  its own `children` count is zero, cascading upward as each descendant retires, so a grandparent's
  carve is never double-spent. The retiring agent still drops its CSpace and stops accepting
  Spaces at once. — **Reversibility:** reversible.
- **D-09 (defaults, not discussed):** `Exit::Budget` is not split from NoMemory (user.rs:279); the
  limitation is recorded in the SUMMARY. `FutexError::NoMemory` maps to `ENOMEM` (12), the same
  number as today. — **Reversibility:** reversible.

### Accepted as assumed, not discussed

Corey chose to discuss only teardown. These were written into CONTEXT.md as signed off exactly as
written in `02-OPEN-DECISIONS.md`. Two are marked costly there; 02-03 and 02-06 still stop at a
blocking `checkpoint:decision` before code regardless.

- **Settled earlier (no discussion needed):** OD-01 (FPU state on the signal stack, depth 8; the
  roadmap's decision due, research confirmed OSTD can), OD-02 (cap-less self-memory calls), OD-03
  (LIBC-04 call scope: set-FS-base, thread create with `SPACE_SELF`, exit-and-unmap), OD-10
  (Phase 1 is complete, so 01-07 landed), OD-12 (`SIGXCPU` belongs to SIG-03).
- **Object model:** OD-11 (promote `Space` to a kernel object), OD-13 (`SPACE_SELF` inherits the
  caller's tag), OD-14 (Thread caps `Weak`, Space caps `Arc`, tombstone Space), OD-18 (set-once
  notify endpoint for a child). OD-11 and OD-14 are costly.
- **Notification and signals:** OD-06 (fault is a kernel-made call to the notify endpoint, verdict
  in reply `words[0]`), OD-17 (remove the per-thread exception handler), OD-16 (redirect skips the
  128-byte red zone), OD-09 (timer tolerance +1 to +3 ticks at 1 kHz).
- **AGNT-05:** OD-05, option (a). Server CPU spent for a client is exempt from the client's kernel
  budget; each server keeps a per-client request ledger keyed by the kernel-stamped tag and
  answers `EAGAIN` over a documented limit. PROJECT.md must not claim kernel CPU limits cover
  server-side work. Costly. The 02-03 checkpoint is where this is confirmed with the cost in view.

### Claude's Discretion

- The exact settle-loop length and the probe tolerance value, as long as the gate in D-03 is
  enforced and the tolerance is justified by a measured baseline from the first run.
- The `cow_clone` error type and how many pages the partial-clone ktest budgets, under D-05.
- Everything under "Discretionary ABI choices" in `02-OPEN-DECISIONS.md` (syscall numbers, types,
  rights, labels, limits), unless a plan finds it wrong.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 2 working set
- `.planning/phases/02-process-objects-teardown-and-timers/02-OPEN-DECISIONS.md` - OD-01..OD-19, ABI tables, "What would show a decision was wrong"
- `.planning/phases/02-process-objects-teardown-and-timers/02-RESEARCH.md` - OSTD 0.19.0 findings, "Open decisions needing the user" (items 1-10)
- `.planning/phases/02-process-objects-teardown-and-timers/02-PATTERNS.md`, `02-VALIDATION.md` - analogs and validation strategy
- `.planning/phases/02-process-objects-teardown-and-timers/02-0{1..10}-PLAN.md` - existing plans, to be replanned

### Project and scope
- `.planning/PROJECT.md` - Core Value wording that AGNT-05 constrains; Safety constraint
- `.planning/REQUIREMENTS.md` - KERN-04, 07-11, AGNT-02, AGNT-05 text
- `.planning/ROADMAP.md` - Phase 2 goal, success criteria, decisions due
- `.planning/STATE.md` - Blockers: Phase 2 carry-forward RR-01 and RR-03; TCG-not-KVM caveat on all runtime evidence

### Prior phase
- `.planning/phases/01-userland-syscall-abi-and-ipc-v2/01-CONTEXT.md` - D-02 unsafe containment rule, still binding
- `.planning/phases/01-userland-syscall-abi-and-ipc-v2/01-VERIFICATION.md`, `01-REVIEW-DISPOSITION.md` - source of RR-01 and RR-03

### Design
- `docs/architecture/option-b-microkernel.md` - process = Space + threads inside an Agent; section 7 open questions
- `docs/architecture/scoping.md` - threat model
- `docs/architecture/adr/0002-rust-microkernel.md` - Redox re-score trigger (an OSTD patch or uncontained `unsafe`)
- `docs/architecture/b2-spike-report.md` - "Not verified" and "Friction" lists (teardown gap, kernel writes into user memory)
- `docs/architecture/unsafe-ledger.md` - Kernel table must stay empty under D-03

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `kernel/src/agent.rs` (257 lines): `Agent::spawn_child` carves memory and CPU budgets; `charge_pages` has no inverse yet (KERN-08).
- `kernel/src/cap.rs`, `kernel/src/ipc.rs`: capability transfer is `derive_caps` (copy), forwarders give per-grant revocation.
- `kernel/src/vm.rs` (315 lines): `cow_clone`, `handle_cow_fault`, `copy_to_user`/`copy_from_user`.
- `kernel/src/futex.rs`, `sched.rs`, `thread.rs`, `user.rs`: futex table, budget scheduler, `block_until`, user-mode loop.
- `userland/abi`, `userland/libsys`: shared syscall ABI and ring-3 runtime used by the Rust test bins.

### Established Patterns
- Crate-wide `#![deny(unsafe_code)]`; any `unsafe` only in a ledgered module.
- All builds and ktests through `kernel/osdk.sh`; evidence so far is QEMU TCG, not KVM.
- Ring-3 test bins replace the B2 hand-assembled programs (01-07).

### Integration Points
- `vm::cow_clone` callers: Space creation in 02-06 and 02-07, later procmgr fork (Phase 4).
- `futex.rs:88` error mapping, `ipc.rs:510` staging-page shortcut.
- `Agent` retirement hook driven from the last-Space teardown (D-02).

</code_context>

<specifics>
## Specific Ideas

- The 10,000x fork+exit loop (success criterion 3) is the acceptance test for D-01..D-05 together:
  flat counters, a passing free-frame probe, and a partial-clone failure test.
- No specific references beyond the ODs.

</specifics>

<deferred>
## Deferred Ideas

- Revoke-on-retire of a dead agent's outgoing grants. Revisit if a Phase 3 or 4 server needs to
  stop honouring a dead originator's delegations.
- An agent pin count for daemon agents with zero processes. Revisit in Phase 4 (procmgr) or Phase 7.
- Re-running all Phase 1 and Phase 2 runtime evidence on a KVM host (STATE.md blocker).

</deferred>

---

*Phase: 2-Process objects, teardown and timers*
*Context gathered: 2026-10-26*
