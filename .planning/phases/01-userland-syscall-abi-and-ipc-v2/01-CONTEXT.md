# Phase 1: Userland, syscall ABI and IPC v2 - Context

**Gathered:** 2026-10-24
**Status:** Ready for planning (one gray area discussed; three left to research and planning, see Claude's Discretion)

<domain>
## Phase Boundary

A Rust `no_std` ring-3 toolchain, a fuzz-safe syscall ABI and call/reply IPC with kernel-stamped
identity, built on the validated B2 kernel. The user-mode FPU/SSE gate is the stop/go decision for
the OSTD base. Requirements: KERN-01, SAFE-01, KERN-02, KERN-03, KERN-05, KERN-06, KERN-12,
IPC-01..06, TEST-06. Process objects, teardown and timers belong to Phase 2.

</domain>

<decisions>
## Implementation Decisions

### FPU/SSE gate (KERN-01, SAFE-01)
- **D-01:** The safe OSTD route gets one spike plan after the OSTD source read. If it fails, go straight to the contained unsafe route. No open-ended safe-route search.
- **D-02:** Containment rule: all FPU/SSE `unsafe` lives in a single module (for example `kernel/src/fpu.rs`) under `#[allow(unsafe_code)]`, capped at about 3 blocks, each with a `// SAFETY:` comment and an entry in `docs/architecture/unsafe-ledger.md`. A CI grep fails if `unsafe` or `allow(unsafe_code)` appears in any other file. `#![deny(unsafe_code)]` stays crate-wide.
- **D-03:** Gate proof: an XMM value survives a yield, and two threads holding different XMM values do not corrupt each other across preemption (per-thread save, which musl needs). AVX and x87 stress are not required for the gate.
- **D-04:** If both the safe and the contained-unsafe routes fail, the phase stops and Corey decides between an OSTD patch, the ADR 0002 Redox re-score, or a scope cut. No automatic switch. — **Reversibility:** one-way — abandoning the OSTD base discards the B2 kernel and restarts B3 on another foundation.

### Claude's Discretion
Not discussed in this session. Research and planning decide, and flag anything that needs Corey:
- IPC cost target: the roadmap criterion records null-IPC N, median and p99 only. Treat it as report-only unless Corey sets a ceiling. No baseline exists yet (Phase 6 compares against Linux).
- User code layout: where ring-3 crates live, flat bootstrap image versus cpio initramfs (a researcher recommendation, not signed off), and how ring-3 test results are reported. Fallback for the initramfs is `include_bytes!` of a cpio.
- Syscall ABI shape: register convention, error return, versioning, and what fuzz-safe covers (numbers, pointers, lengths, capability handles).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project and scope
- `.planning/PROJECT.md` - Safety constraint (contained unsafe, decided 2026-10-24) and Key Decisions
- `.planning/REQUIREMENTS.md` - KERN, SAFE, IPC, TEST-06 requirement text
- `.planning/ROADMAP.md` - Phase 1 goal, success criteria, research and decisions-due lists
- `.planning/STATE.md` - Blockers: OSTD docs.rs build failure, FPU risk order

### Design
- `docs/architecture/option-b-microkernel.md` - microkernel design, section 7 open questions
- `docs/architecture/scoping.md` - threat model and agent semantics
- `docs/architecture/adr/0002-rust-microkernel.md` - Redox re-score trigger
- `docs/architecture/b2-spike-report.md` - what B2 built, "Not verified" and "Friction" lists
- `docs/architecture/research/microkernel-posix.md`, `docs/architecture/research/rust-kernels-and-adherence.md` - prior art

### Research (MEDIUM confidence, unverified)
- `.planning/research/STACK.md` - custom `x86_64-aok-user.json` target, nightly pin, LLVM 21
- `.planning/research/ARCHITECTURE.md`, `.planning/research/PITFALLS.md`, `.planning/research/SUMMARY.md`

### To create in this phase
- `docs/architecture/unsafe-ledger.md` - not yet written (SAFE-01)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `kernel/src/{agent,cap,futex,ipc,sched,thread,user,vm}.rs`: about 2100 lines including tests; B2 primitives (cspace, agent, endpoints, futex, budget scheduler, CoW clone).
- `kernel/osdk.sh`, `kernel/Dockerfile`, `kernel/OSDK.toml`: Docker build and QEMU test path. Image `agentic-os-kernel-dev:0.19.0`.

### Established Patterns
- Crate-wide `#![deny(unsafe_code)]`; all builds through `kernel/osdk.sh`.
- B2 ring-3 test programs are hand-assembled in `kernel/src/user.rs`; Phase 1 replaces them with a Rust crate.

### Integration Points
- `kernel/src/user.rs` and `kernel/src/thread.rs`: user entry and syscall handling.
- `kernel/src/ipc.rs`: IPC v2 (call/reply, Cancel, tid/tag stamp) extends this.
- `kernel/src/vm.rs`: cross-space copy and CoW break.

</code_context>

<specifics>
## Specific Ideas

No specific requirements beyond the FPU gate rules above.

</specifics>

<deferred>
## Deferred Ideas

None - discussion stayed within phase scope.

</deferred>

---

*Phase: 1-Userland, syscall ABI and IPC v2*
*Context gathered: 2026-10-24*
