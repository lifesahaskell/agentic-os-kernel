---
phase: 02-process-objects-teardown-and-timers
verified: 2026-10-26T00:00:00Z
status: human_needed
score: 5/7 must-haves verified
covered_files:
  - ".planning/phases/02-process-objects-teardown-and-timers/02-01-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-01-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-02-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-02-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-03-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-03-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-04-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-04-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-05-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-05-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-06-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-06-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-07-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-07-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-08-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-08-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-09-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-09-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-10-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-10-SUMMARY.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-11-PLAN.md"
  - ".planning/phases/02-process-objects-teardown-and-timers/02-11-SUMMARY.md"
  - "docs/architecture/adr/0003-server-cpu-exemption.md"
  - "kernel/src/agent.rs"
  - "kernel/src/cap.rs"
  - "kernel/src/futex.rs"
  - "kernel/src/ipc.rs"
  - "kernel/src/ipc/timer.rs"
  - "kernel/src/lib.rs"
  - "kernel/src/probe.rs"
  - "kernel/src/sched.rs"
  - "kernel/src/space.rs"
  - "kernel/src/space/mmap.rs"
  - "kernel/src/syscall.rs"
  - "kernel/src/thread.rs"
  - "kernel/src/user.rs"
  - "kernel/src/vm.rs"
  - "scripts/check.sh"
  - "userland/abi/src/lib.rs"
  - "userland/libsys/src/ledger.rs"
  - "userland/libsys/src/lib.rs"
  - "userland/libsys/src/raw.rs"
  - "userland/libsys/src/rt.rs"
covered_digest: "v3:sha256:56ace07d9dc173dffcff45e26812316f65fefefc8c0ec7cd632ab30fd2c56b91"
behavior_unverified: 2
overrides_applied: 0
deferred:
  - truth: "AGNT-02 SIGXCPU leg: CPU-budget exhaustion is reported as SIGXCPU"
    addressed_in: "Phase 5"
    evidence: "Phase 5 success criterion 2: '`SIGXCPU` on CPU-budget exhaustion'; REQUIREMENTS SIG-03 (Phase 5, Pending); OD-12 in 02-OPEN-DECISIONS.md"
  - truth: "Ring-3 kill of a process (Space::kill reachable from user mode)"
    addressed_in: "Phases 4-5"
    evidence: "procmgr owns ring-3 kill (OD-15: Space::kill is a kernel API in Phase 2); ROADMAP Phase 4 (procmgr) and Phase 5 (kill, sigaction)"
behavior_unverified_items:
  - truth: "x87 control word and MXCSR survive a signal redirect and sigreturn"
    test: "Redirect a ring-3 thread that has a non-default x87 control word and MXCSR, run a handler that changes both, sigreturn, then read both back."
    expected: "Both registers hold the pre-redirect values after sigreturn."
    why_human: "RedirectFrame carries a whole FpuContext, so the state is saved by construction, but the sigreturn probe checks only the 16 xmm registers and the GPRs. Presence of the field cannot show the restore path loads it correctly."
  - truth: "cow_clone refuses a range containing a huge page or MMIO mapping with Unsupported and charges nothing"
    test: "Map a 2 MiB page in a parent range, call cow_clone over it, and assert Err(Unsupported(va)), an unchanged agent budget, and no leaked node frames."
    expected: "Unsupported(va) returned, ledger and frame probe unchanged."
    why_human: "vm.rs:94-98 contains the arm and ktests cover the Unsupported error type and its EINVAL mapping, but no ktest builds a huge-page mapping. 02-06 states this arm is verified by inspection only."
human_verification:
  - test: "Re-run the full ktest suite and the three boot checks on a KVM host (`./osdk.sh cargo osdk test`, then `sh scripts/check.sh` without --no-kvm)"
    expected: "181 passed, 0 failed. Idle halts land in [25,200] over 50 ticks. The 10,000x loops stay flat. Timer and timeout tolerance tests pass at D+1..D+3 ticks."
    why_human: "This host has no /dev/kvm. Every run in this verification is QEMU TCG. The ring-3 timer test measured 37 of 40 ticks under TCG, and a 30-tick delay is calibrated to TCG speed. TCG timing says nothing about KVM timing."
  - test: "Review the judgment-tier prohibitions carried in the 02-xx PLAN must_haves (all status unresolved)"
    expected: "A person resolves each one. unverified-prohibition, human review recommended."
    why_human: "Judgment-tier items get a non-authoritative judge verdict at most, never a pass. See the Prohibitions section."
  - test: "Run a ring-3 kill that lands while the target is executing in ring 3, then check the counts and probe"
    expected: "Thread ends Exit::Killed, space torn down, counts flat."
    why_human: "02-06 records no ktest for a kill mid-run in ring 3. kill_stops_a_running_spinner (ok) kills a spinner, which suggests the path is covered, but I did not confirm the spinner was in ring 3 at the instant of the kill. Treat as confirm-or-close."
---

# Phase 2: Process objects, teardown and timers Verification Report

**Phase Goal:** As the procmgr author, I want Space and Thread capabilities, full process teardown and timers, so that processes can be created, signalled and destroyed without leaks.
**Verified:** 2026-10-26
**Status:** human_needed
**Re-verification:** No, initial verification
**Tree:** HEAD 0aa9de3 on research/recursion-spikes. The last code commit is eb3d219; HEAD differs from it by docs only.

All execution evidence is QEMU TCG. There is no /dev/kvm on this host. Nothing here is KVM evidence.

## User Flow Coverage (MVP mode)

Story: "As the procmgr author, I want Space and Thread capabilities, full process teardown and timers, so that processes can be created, signalled and destroyed without leaks." The format guard passes (valid user story). The outcome clause is "created, signalled and destroyed without leaks". procmgr itself is Phase 4, so the flow is driven by ktests acting as procmgr through the kernel API and ring-3 test binaries.

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| Create a process | A Space is built over an image or a CoW clone; a thread starts in it as the space owner | `Space::from_image`/`from_clone` (space.rs:186, 207); `thread_create` (space.rs:552) checks fields, tag rule, CREATE right, limit, cspace room; ktests `thread_create_ring3_holder_runs_thread_in_other_space`, `thread_create_refuses_without_create`, `thread_create_limit_is_64`, `thread_create_refuses_bad_ctx` all ok | VERIFIED |
| Signal it | `thread_interrupt` redirects a thread via a SIGNAL Thread cap; the saved context returns through `sigreturn` | space.rs:611; syscall.rs:666-674; ktests `timer_interrupt_redirects_a_running_thread`, `sigreturn_restores_all_gprs_fpu_and_leaves_the_user_stack_untouched`, `sigreturn_nests_to_depth_8_then_eagain` ok | VERIFIED (x87/MXCSR unprobed) |
| Observe exit and faults | A note naming tid and tag reaches the space endpoint | `note_exit` (space.rs:386); ktests `note_exit_names_tid_and_tag_of_each_sibling`, `note_fault_then_exit_from_one_sibling`, `fault_note_carries_vector_addr_rip_tid_and_tag` ok | VERIFIED |
| Destroy it | Kill or last-thread exit unmaps, refunds, drops timers and notify cap, wakes futex waiters, retires the agent | `Space::kill`/`reap`/`teardown` (space.rs:279-351); ktests `kill_wakes_the_dead_process_futex_waiters`, `counts_drop_the_space_armed_timers`, `retire_returns_carve`, `teardown_unmaps_owned_space_and_leaves_a_tombstone` ok | VERIFIED |
| Without leaks | 10,000 create/destroy turns leave every counter and the free-frame probe flat | `teardown_loop_same_agent_10000x`, `retire_loop_child_agent_10000x`, `frame_probe_one_process_returns_to_baseline` ok (see Truth 3) | VERIFIED on TCG |

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: A holder of a Space capability creates a thread in another space with thread_create and interrupts it; a non-holder is refused | VERIFIED | `thread_create` resolves the target through `cspace.get(slot)?.resolve(Rights::CREATE)` and returns `Cap`/`WrongType` errors for non-holders (space.rs:571-585). The new thread gets the space owner's agent, not the creator's (space.rs:589, OD-11). `thread_interrupt` needs SIGNAL (space.rs:615-620). Ring 3 reaches both through syscalls 0x104/0x105 (syscall.rs:406-416, 662-666). Behavioral: `thread_create_ring3_holder_runs_thread_in_other_space`, `thread_create_refuses_without_create`, `thread_create_kernel_path_runs_in_other_space`, `thread_create_self_inherits_tag`, `thread_create_writes_tid_or_starts_nothing`, `thread_create_full_cspace_is_emfile`, `killed_space_refuses_thread_create`; syscall decode tests in syscall.rs:1015-1068 and errno table at 1220-1263. Accepted deviation: `thread_interrupt` to an entry in [0,0x40_0000) returns EPERM, not EINVAL (02-04). |
| 2 | SC2: A fault or exit in one of two forked siblings reaches the process endpoint naming tid and tag; a redirect saves full context on a kernel-held stack and sigreturn restores it; the kernel writes nothing to the user stack | VERIFIED (GPR and xmm; see Truth 6 for x87/MXCSR) | Notes: `siblings_are_distinguished_by_tid_and_tag`, `note_exit_names_tid_and_tag_of_each_sibling`, `note_fault_then_exit_from_one_sibling`, `fault_note_carries_vector_addr_rip_tid_and_tag` ok. Stack: `RedirectFrame` with `MAX_FRAMES = 8` in thread.rs:78-90, `SYS_SIGRETURN` pops it (syscall.rs:673-674) and `restore_frame` applies it (user.rs:342). Behavior: `sigreturn_restores_all_gprs_fpu_and_leaves_the_user_stack_untouched`, `sigreturn_resumes_the_interrupted_loop`, `sigreturn_moves_rsp_below_the_red_zone`, `sigreturn_on_an_empty_stack_is_einval`, `sigreturn_nests_to_depth_8_then_eagain`, `sigreturn_frames_dropped_at_kill` ok. Negative controls via the `CORRUPT_FRAME` fault injection: `sigreturn_probe_detects_a_corrupted_rbx` and `sigreturn_probe_detects_a_skipped_fpu_restore` ok, so the probe is shown able to fail. Caveat: `sigreturn_after_a_blocked_call_returns_eintr` is a characterization test never seen to fail. |
| 3 | SC3: A 10,000x fork+exit loop leaves page and capability counts flat, and a dead process's futex waiters are woken | VERIFIED on TCG only | `teardown_loop_same_agent_10000x` (same agent, CoW clone, store into the CoW page, join, drop) asserts `mem_used_pages`, cspace room, `assert_counts_flat` (nine counters incl. live_threads, live_spaces, dropped_spaces == iterations, futex keys, deadlines, armed timers, reply waits) and `assert_probe_flat` against the frame allocator at tolerance F = 688128 bytes. `retire_loop_child_agent_10000x` additionally asserts CPU carve, live cap count, endpoint refcount and live agents. Both ok in the full log. `frame_probe_one_process_returns_to_baseline` asserts exact equality after one process. `kill_wakes_the_dead_process_futex_waiters` and `kill_frees_a_thread_in_an_uninterruptible_reply_wait` ok. The "fork" is `Space::from_clone` over `cow_clone`, driven from the kernel API; a real `fork` syscall is procmgr, Phase 4. The loops ran under TCG, so the KVM re-run is owed. |
| 4 | SC4: With every agent blocked the CPU halts; timed futex and recv waits expire within a stated tolerance; a timer delivers a message to an endpoint | VERIFIED on TCG only | Halt: `idle_halt_check` (lib.rs:59) is a boot check that poweroff depends on (lib.rs:178). I re-ran it on HEAD under TCG: it passed with 44 idle halts over 53 ticks, inside the [25,200] band. `check.sh --no-kvm` skips it, so this run is the only HEAD evidence. Idle task: sched.rs `idle_body` loops `halt_cpu()`; ktests `sched_idle_is_picked_only_when_nothing_is_eligible`, `sched_idle_is_never_counted_as_others`, `sched_enqueue_requests_preempt_when_idle_runs` ok. Tolerance (D+1..D+3 ticks at 1 kHz): `timeout_futex_expires_within_tolerance`, `timeout_futex_ring3_never_early`, `timeout_recv_expires_within_tolerance`, `timeout_recv_ring3_never_early`, `timeout_recv_message_beats_deadline` ok. Timer: `timer_msg_delivers_to_endpoint`, `timer_msg_ring3_arm_and_recv`, `timer_msg_limits`, `timer_msg_label_is_reserved`, `timer_msg_full_queue_retries` ok. |
| 5 | SC5: A ring-3 mmap past the memory budget returns ENOMEM; the server CPU-exemption decision is recorded with a per-client request ledger and documented limit; PROJECT.md does not claim kernel CPU limits cover server work | VERIFIED | `vmem_enomem_past_budget_from_ring3`, `vmem_enomem_maps_nothing`, `vmem_mmap_after_teardown_is_enomem`, `vmem_mmap_munmap_loop_keeps_pages_flat` ok. ADR 0003 exists (Status: Proposed, option (a), chosen by Corey 2026-10-26 at the 02-03 checkpoint) with the ledger design, `L = 128` per `W = 2^32` cycles defaults, 64-tag table and EAGAIN. `userland/libsys/src/ledger.rs` implements `RequestLedger::admit`/`charge`, keyed on the kernel-stamped tag. ktests `request_ledger_limit_answers_eagain_per_tag` (ring-3 server, limit 4, fifth request gets EAGAIN, other tag unaffected) and `request_ledger_self_test` ok. PROJECT.md lines 20-21, 36 and 113 state that the CPU budget covers an agent's own threads and not server-side work. The ADR stays Proposed until Phase 4 servers adopt it, as it says. |
| 6 | x87 control word and MXCSR survive redirect and sigreturn (part of "all FPU state" in SC2) | PRESENT_BEHAVIOR_UNVERIFIED | See behavior_unverified_items. Routed to human verification. |
| 7 | cow_clone refuses a huge-page or MMIO range with `Unsupported` and charges nothing | PRESENT_BEHAVIOR_UNVERIFIED | See behavior_unverified_items. The arm is at vm.rs:94-98. Routed to human verification. |

**Score:** 5/7 truths verified (2 present, behavior-unverified). Truths 1-5 are the roadmap contract; 6 and 7 are plan-level sub-claims split out so the gap is visible.

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | SIGXCPU on CPU-budget exhaustion (the SIGXCPU leg of AGNT-02) | Phase 5 | Phase 5 SC2 names `SIGXCPU` on CPU-budget exhaustion; SIG-03 is mapped to Phase 5; OD-12 records the split and Phase 2 reserves a kernel label for it |
| 2 | Ring-3 kill and a real fork syscall | Phases 4-5 | Phase 2 delivers `Space::kill` as a kernel API (OD-15); procmgr owns the ring-3 surface |

### Required Artifacts

`verify.artifacts` was not used: the plan frontmatter was read directly. Each artifact below was opened and checked for substance and wiring.

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `kernel/src/space.rs` | Space object, thread_create/interrupt, teardown, kill, notify, ledger, `counts()` | VERIFIED | Read in full (lines 1-672 plus loop ktests). Wired from syscall.rs and user.rs. |
| `kernel/src/thread.rs` | Frame stack, `try_interrupt`, `kill`, `CORRUPT_FRAME` | VERIFIED | `MAX_FRAMES` 8, `RedirectFrame`, `try_interrupt` under lock. |
| `kernel/src/vm.rs` | Fallible `cow_clone`, `CloneError`, `copy_to_user` | VERIFIED | Callers: `Space::from_clone`, futex/IPC copy paths. Huge-page arm unexercised (Truth 7). |
| `kernel/src/agent.rs` | `Payer`, `enter_space`/`leave_space`, `retire`, `return_carve`, `live_agents` | VERIFIED | Symbols present at agent.rs:38, 121, 304, 315, 346, 379; retire ktests ok. |
| `kernel/src/ipc.rs`, `ipc/timer.rs` | Charged staging, timers | VERIFIED | `StagingCharge` ipc.rs:35; `arm`, `drop_space`, `armed_count` in timer.rs. |
| `kernel/src/futex.rs` | `From<AgentError>` mapping, timeout | VERIFIED | futex.rs:55; ktests ok. |
| `kernel/src/sched.rs`, `lib.rs` | Idle task, boot halt check | VERIFIED | `idle_body`, `IDLE_HALTS`, `idle_halt_check`; boot check re-run at HEAD. |
| `kernel/src/space/mmap.rs` | mmap/mprotect/munmap/brk charged to the ledger | VERIFIED | 18 `vmem_*` ktests ok. |
| `kernel/src/probe.rs` | cfg(ktest) free-frame probe with `settled()` | VERIFIED | Gated `#[cfg(ktest)]` in lib.rs:14. |
| `userland/libsys/src/ledger.rs` | `RequestLedger` | VERIFIED | Substantive; used by `ledger_srv.rs` bin and the ktest. |
| `userland/tests/src/bin/*` | 28 ring-3 test programs | VERIFIED | Each referenced by a ktest that passed. |
| `docs/architecture/adr/0003-server-cpu-exemption.md` | AGNT-05 decision | VERIFIED | Status Proposed by design. |

### Key Link Verification

`verify.key-links` cannot check non-bare-path `from:` values (a known plan-text defect), so links were checked by grep and by the ktests that traverse them.

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| syscall.rs `SYS_THREAD_CREATE` | `space::thread_create` | decode at 406, dispatch at 607/662 | WIRED | |
| syscall.rs `SYS_THREAD_INTERRUPT` | `space::thread_interrupt` | 416, 666 | WIRED | |
| syscall.rs `SYS_SPACE_NOTIFY` | `space::set_notify` | 420, 670 | WIRED | |
| syscall.rs `SYS_SIGRETURN` | `Thread::pop_frame` then `restore_frame` | 673-674 then user.rs:342 | WIRED | |
| syscall.rs `SYS_TIMER_ARM` | `ipc::timer::sys_timer_arm` | 401, 659 | WIRED | |
| user.rs thread exit | `Space::clear_child_tid`, `note_exit`, `reap` | user.rs:195-197, 216-218 | WIRED | Both exit points. |
| `Space::teardown` | `timer::drop_space`, unmap, `uncharge_pages`, `owner.leave_space()` | space.rs:322-351 | WIRED | Order matches the doc comment. |
| `Space::kill` | `Thread::kill` for each live thread, then `teardown` if none | space.rs:279-294 | WIRED | |
| `thread_create` | `user::spawn_thread` | space.rs:605 | WIRED | |
| `Space` as `Payer` | `Agent::charge_pages` plus ledger | space.rs:455-474 | WIRED | Refund capped to the ledger. |
| `Space::from_clone` | `cow_clone` via `Tally` | space.rs:215-235 | WIRED | Node frames charged and recorded. |
| `StagingCharge` | sender's `Payer`, uncharged on drop | ipc.rs:35-53, 759 | WIRED | `staging_clone_uncharges_once` ok. |
| `libsys::ledger` | per-client tag from `IpcArgs.tag` | ledger.rs `admit(tag, now)` | WIRED | Keyed on the kernel stamp, not the body. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `Counts` / `counts()` | nine counters | live atomics and tables in thread, space, agent, futex, timer, ipc | Yes: each reads the structure it names; `dropped_spaces == iterations` is asserted | FLOWING |
| `charged_pages` ledger | pages | `charge_pages` on every mmap, CoW break and clone | Yes: refunded at teardown, asserted flat | FLOWING |
| Note payload `[kind, value, 0, 0]` | exit kind and value | `Exit` from the thread, mapped in `note_exit` | Yes: asserted per sibling | FLOWING |
| `RedirectFrame` | UserContext, FpuContext, FS base | captured at redirect, restored at sigreturn | Yes for GPR and xmm (negative controls). x87/MXCSR unprobed | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full unfiltered ktest on this tree | orchestrator run; log `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/551cda72-80af-415e-95df-8a64bb7a2418/scratchpad/full.clean` | `ok. 181 passed; 0 failed`; my parser resolved 181 of 181 per-test results (`.../scratchpad/res.py`) | PASS (TCG) |
| Idle halt, CPU budget and FPU preemption boot checks at HEAD | `./osdk.sh cargo osdk run` equivalent, log `.../scratchpad/v/boot-head.log` | exit 0, three checks passed, 44 halts over 53 ticks | PASS (TCG) |
| Static gates | `sh scripts/check.sh --no-kvm` (orchestrator) | passes; static gates and `osdk build` only, does not compile cfg(ktest) | PASS |
| Unsafe ledger | `python3 -I tools/unsafe_check.py` | `unsafe_check: ok`; `#![deny(unsafe_code)]` at lib.rs:6; no `unsafe` token in kernel/src outside that attribute | PASS |
| Debt markers | grep `TBD\|FIXME\|XXX` over kernel/src, userland, scripts, ADR 0003 | no matches | PASS |
| Stub markers | grep `TODO\|HACK\|PLACEHOLDER\|todo!\|unimplemented!` over kernel/src and userland | no matches | PASS |
| Leftover containers | `docker ps -q` | empty | PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist and no plan declares a shell probe. The phase's probe is the cfg(ktest) free-frame probe (`kernel/src/probe.rs`), exercised by the ktests above (`frame_probe_*`, footprint 688128 bytes). Step 7c: SKIPPED (no shell probes declared).

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| KERN-04 | 02-04, 02-05, 02-06 | Space and Thread capabilities, thread create and interrupt | SATISFIED | Truth 1; REQUIREMENTS.md checked |
| KERN-07 | 02-01 | Idle task halts the CPU when nothing is runnable | SATISFIED (TCG) | Truth 4, boot check at HEAD. REQUIREMENTS.md line 23 and traceability line 188 still say unchecked/Pending: tracking lag, needs updating |
| KERN-08 | 02-02 | Timers and timed waits | SATISFIED (TCG) | Truth 4; REQUIREMENTS.md checked |
| KERN-09 | 02-08 | Saved-context signals and sigreturn | SATISFIED (x87/MXCSR unprobed) | Truth 2, Truth 6; REQUIREMENTS.md checked |
| KERN-10 | 02-05, 02-06, 02-07 | Process teardown without leaks | SATISFIED (TCG) | Truth 3; REQUIREMENTS.md checked |
| KERN-11 | 02-09, 02-10 | Kernel calls LIBC-04 needs (set-FS-base, thread create, exit-and-unmap) scoped here | SATISFIED | `arch_prctl` round trip, `exit_unmap`, `clear_tid` ktests ok. GS codes return EINVAL (OD-03). Two clear_tid tests assert a non-event |
| AGNT-02 | 02-07, 02-09 | Budget exhaustion maps to errno and SIGXCPU | PARTIAL, SATISFIED per its Test clause | Test clause (mmap past budget gives ENOMEM) is met. The SIGXCPU leg is not built: deferred to Phase 5 (SIG-03, OD-12). REQUIREMENTS.md marks AGNT-02 Complete, which overstates it |
| AGNT-05 | 02-03 | Server CPU exemption recorded with ledger and limit | SATISFIED | Truth 5. REQUIREMENTS.md line 122 and traceability line 253 still say unchecked/Pending: tracking lag |

All eight IDs from the phase are accounted for. No ORPHANED requirement: every ID REQUIREMENTS.md maps to Phase 2 appears in a plan. SIG-03 (Phase 5) and LIBC-04 (Phase 6) are correctly not claimed.

### Anti-Patterns Found

None blocking. No debt markers, no stubs, no empty implementations flagged on the files modified by this phase. Notes:

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `kernel/src/user.rs` | 359-361 | `Exit::Budget` covers both spent budget and failed allocation (D-09) | Info | Documented, accepted; split when exit causes reach ring 3 |
| `kernel/src/space.rs` | 55-59 | Data frames shared with a CoW clone stay charged to the parent | Info | Documented under-charge, never a leak (D-07) |
| `kernel/src/lib.rs` | 18 | cfg(ktest) probe module | Info | Test-only, not shipped |

### Classification of the Known Open Items

| Item | Classification | Reason |
|------|----------------|--------|
| SIGXCPU leg of AGNT-02 (SIG-03, OD-12) | Deferred (accepted deviation) | Phase 5 SC2 names it. OD-12 records the split. AGNT-02's own Test clause is met. REQUIREMENTS.md should not show AGNT-02 as fully Complete until SIG-03 lands |
| All evidence is TCG, KVM re-run owed (10,000x loops, 37/40 timer ticks, 30-tick TCG calibration) | Human-needed | The 5 roadmap truths are demonstrated on TCG only. Timing tests are the ones most likely to differ on KVM |
| 02-04: `thread_interrupt` [0,0x40_0000) returns EPERM, not EINVAL | Accepted deviation | Refusal is correct and the errno is a documented choice; no must-have names the errno |
| 02-06: no ktest for a kill landing mid-run in ring 3 | Human-needed (confirm or close) | `kill_stops_a_running_spinner` is ok, but I did not confirm it hits ring 3 at the instant of the kill |
| 02-06: huge-page arm by inspection only | Human-needed (Truth 7, PRESENT_BEHAVIOR_UNVERIFIED) | Real arm, no ktest builds a huge mapping |
| 02-06: adopted spaces get no refund | Accepted deviation | By design (OD-19): the caller keeps that memory. `teardown_keeps_an_adopted_space_mapped` and `retire_adopted_spaces_do_not_count` document it |
| 02-07: staged pages in a retired agent's queued messages count against nobody until consumed | Accepted deviation | Bounded by queue length, not a leak: the charge drops with the message. Revisit with procmgr |
| 02-07: `Exit::Budget` covers spent budget and failed allocation | Accepted deviation | D-09, documented in user.rs |
| 02-08: x87/MXCSR not probed | Human-needed (Truth 6, PRESENT_BEHAVIOR_UNVERIFIED) | The full `FpuContext` is saved, but no test reads those registers back |
| 02-08: `sigreturn_after_a_blocked_call_returns_eintr` never seen to fail | Warning | A characterization test is weaker evidence than the negative-controlled probes; it does not carry a must-have |
| 02-10: GS arch_prctl codes return EINVAL (OD-03) | Accepted deviation | Recorded decision; ARCH_SET_FS/GET_FS are what LIBC-04 needs |
| 02-10: two clear_tid tests assert a non-event | Warning | They check that nothing is written, so they would pass on a no-op. The positive test `lifecycle_clear_tid_zeroed_and_woken_at_exit` has its own failing-first record (commit 244c886) |

### Prohibitions (judgment tier)

The 02-xx PLANs carry judgment-tier prohibitions, all `unresolved`. Per the verifier contract these get a non-authoritative judge verdict and the flag `unverified-prohibition, human review recommended`. I did not adjudicate each one individually. Spot checks that bear on the common ones: no `unsafe` was added (`unsafe_check` ok and no `unsafe` token in kernel/src); the kernel writes nothing to the user stack on redirect (ktest `..._leaves_the_user_stack_untouched`); the ledger keys on the kernel tag, not the message body (ledger.rs). Judge verdict: no violation seen. Status: flagged for human review, not passed.

### Human Verification Required

1. **KVM re-run.** Test: `./osdk.sh cargo osdk test`, then `sh scripts/check.sh` without `--no-kvm`, on a KVM host. Expected: 181 passed, 0 failed; boot checks pass; timing tolerances hold. Why human: no /dev/kvm here; all evidence is TCG.
2. **Judgment-tier prohibitions.** Test: resolve each in the 02-xx PLAN must_haves. Expected: explicit human resolution. Why human: judgment tier is never auto-passed.
3. **x87 control word and MXCSR across sigreturn.** Test and expected as in behavior_unverified_items.
4. **cow_clone huge-page refusal.** Test and expected as in behavior_unverified_items.
5. **Ring-3 kill mid-run.** Confirm that `kill_stops_a_running_spinner` kills a thread executing in ring 3, or add a ktest.

### Gaps Summary

No FAILED truth, missing artifact, unwired link or blocking anti-pattern. The goal is achieved in the code on the evidence available, which is TCG. The status is human_needed because of the owed KVM run, two behavior-dependent sub-claims that no test exercises, one unconfirmed kill path, and the flagged judgment-tier prohibitions.

Tracking to fix (not code gaps): REQUIREMENTS.md still shows KERN-07 and AGNT-05 unchecked and Pending although both are evidenced; it marks AGNT-02 Complete although its SIGXCPU leg is deferred to Phase 5. ADR 0003 is Proposed by design, to be Accepted after Phase 4 servers adopt the ledger.

Not read: 02-PATTERNS, 02-RESEARCH, 02-VALIDATION and 02-DISCUSSION-LOG; they carry no must-haves.

---

_Verified: 2026-10-26_
_Verifier: Claude (gsd-verifier)_
