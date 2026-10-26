---
phase: 02-process-objects-teardown-and-timers
plan: 03
subsystem: agent-budgets
tags: [AGNT-05, request-ledger, ADR-0003, libsys, ring3-server, EAGAIN]

requires:
  - phase: 01-userland-syscall-abi-and-ipc-v2
    provides: "libsys recv/reply_recv, kernel-stamped IpcArgs.tag, libsys::tsc (ledger row U-05), user_image!/spawn_image test harness, Rust ring-3 bins (01-07)"
provides:
  - "libsys::ledger::{RequestLedger, Verdict, LEDGER_CLIENTS = 64, DEFAULT_LIMIT, DEFAULT_WINDOW_CYCLES}: per-client request ledger keyed on the kernel-stamped tag"
  - "bin ledger_srv: ring-3 server that answers EAGAIN past the limit; arg 1 runs a synthetic-time self-test"
  - "ADR 0003 recording the server CPU-exemption decision (option a)"
  - "PROJECT.md wording that kernel CPU budgets do not cover server-side work"
affects: [02-04, 04-procmgr, 04-vfs]

actuals:
  tokens: 4800
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Server admission control: ledger.admit(args.tag, tsc()) after recv, EAGAIN in words[0] when Busy"
    - "Policy with an injected clock: now is a parameter, so ring-3 self-tests use synthetic times"

key-files:
  created:
    - userland/libsys/src/ledger.rs
    - userland/tests/src/bin/ledger_srv.rs
    - docs/architecture/adr/0003-server-cpu-exemption.md
  modified:
    - userland/libsys/src/lib.rs
    - kernel/src/agent.rs
    - .planning/PROJECT.md

key-decisions:
  - "Task 0 (OD-05): the user chose option-a. Server CPU is exempt from the client's kernel CPU budget; each server keeps a per-client request ledger keyed by the kernel-stamped tag; over the limit or table full answers EAGAIN; no kernel change."
  - "Libsys defaults: LEDGER_CLIENTS = 64, DEFAULT_LIMIT = 128 requests per tag per window, DEFAULT_WINDOW_CYCLES = 2^32 (about one second at 4 GHz). Policy values, not measured. The test server uses L = 4 and a 2^62-cycle window."
  - "TSC source: libsys::tsc() (rdtsc, ledger row U-05, added by 01-07). Not serialised. The self-test does not use it."

requirements-completed: [AGNT-05]

duration: ~30min
completed: 2026-10-26
status: complete
commits: 2
plan_head_before: 09b184a508a905325305892bb997022bd9f665e8
plan_head_after: 176e6996564fda88eb4d147267e8ea0f7eb9d600
---

# Phase 2 Plan 03: Server CPU exemption and request ledger Summary

A no-alloc, no-unsafe `RequestLedger` in libsys (64 tags, L requests per tag per window, expired entries recycled) lets a ring-3 server answer the fifth request of one kernel-stamped tag with EAGAIN while another tag is still served, and ADR 0003 plus PROJECT.md record that kernel CPU budgets do not cover server-side work.

## Performance

- **Duration:** about 30 min (start time was not captured; estimate)
- **Completed:** 2026-10-26
- **Tasks:** 3 of 3 (Task 0 resolved by the user, Tasks 1 and 2 executed)
- **Files modified:** 6 (3 created, 3 modified)

## Task 0: checkpoint answer

The user answered `option-a` for OD-05 in the continuation session: server CPU is exempt from the client's kernel CPU budget; per-client request ledger keyed by the kernel-stamped tag with a documented limit; over the limit or table full answers EAGAIN; no kernel change. Option (b) (reply-handle budget credit) is recorded in ADR 0003 as the alternative not taken now. No commit for Task 0.

## Accomplishments

- **Task 1 (`5a7494e`, tracer).** `userland/libsys/src/ledger.rs`: `RequestLedger::{new, admit, charge, requests, cycles}` and `Verdict::{Admit, Busy}`. `admit` starts a new window for a tag when `now - window_start >= window`, returns `Busy` once `requests` reaches the limit, takes a never-used entry for a new tag and otherwise recycles an entry whose window expired, and returns `Busy` for a new tag when every entry is live. A counter that moves backwards counts as zero elapsed time. `userland/tests/src/bin/ledger_srv.rs`: arg 0 serves slot 0 with L = 4 and a 2^62-cycle window; arg 1 runs a 12-check synthetic-time self-test and exits with the failing check's number. `kernel/src/agent.rs` test module: `request_ledger_limit_answers_eagain_per_tag` (tags 0xA and 0xB from kernel `Thread::new_tagged` clients; 0xA calls 1-4 get 0, call 5 gets `-EAGAIN`; 0xB's four calls get 0; 0xA stays refused; label 0xFFFF stops the server, exit 0) and `request_ledger_self_test`.
- **Task 2 (`176e699`).** `docs/architecture/adr/0003-server-cpu-exemption.md` (Status Proposed, 2026-10-26) with Context, Decision, The ledger (key, window, limit, table size, EAGAIN), Consequences, Alternative considered, What would show this wrong. PROJECT.md: one paragraph after the Core Value paragraph, the Validated CPU-budget line qualified, and a Key Decisions row (outcome Pending).

## Evidence, and what it is

All runtime evidence is **QEMU TCG** (this host has no `/dev/kvm`). Nothing in this plan ran under KVM.

- `sh scripts/check.sh --no-kvm` (run twice, the second time after the final source edit): ended `check: static gates passed; LOCAL-ONLY (needs /dev/kvm): ...`. This covers unsafe_check, userland build/fmt/clippy, kernel fmt/clippy and `cargo osdk build`.
- `./osdk.sh sh -c '../userland/build.sh && cargo osdk test'` (full run, TCG): `test result: ok. 71 passed; 0 failed; 0 filtered out.` Both new tests are in that run and printed `ok`; no `FAILED` line in the log. Log: `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/551cda72-80af-415e-95df-8a64bb7a2418/scratchpad/ktest-full.log`.
- `unsafe_check: ok`. `grep -c 'pub mod ledger' userland/libsys/src/lib.rs` = 1. `grep -c 'fn request_ledger_' kernel/src/agent.rs` (comments excluded) = 2. `grep -c 'server-side work'`: PROJECT.md 2, ADR 0003 2. `grep -c '0003-server-cpu-exemption' .planning/PROJECT.md` = 1. `grep -cE '^- Status: '` on the ADR = 1.
- `cargo osdk run` boot checks were not run: the plan does not require them and the plan touches no boot path.

### Needs the deferred KVM re-run

- Both ktests and the full 71-test run (the logic is time-independent, since `request_ledger_limit_answers_eagain_per_tag` uses a 2^62-cycle window and the self-test uses synthetic times, so a different result under KVM would be a surprise, but it is untested).
- The ring-3 `tsc()` reads in `ledger_srv` (cycles charged per request): under TCG the values are emulator cycles, not hardware. No assertion depends on their magnitude.
- The defaults `DEFAULT_LIMIT = 128` and `DEFAULT_WINDOW_CYCLES = 2^32`: unmeasured under either accelerator.
- The `<human-check>` of Task 2 (PROJECT.md and ADR 0003 wording) is harvested at end of phase. I re-read the Core Value addition, the Validated line and ADR Decision/Consequences while writing them and found no sentence that says or implies a kernel CPU budget limits server work; that is the author's check, not the harvested one.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ktest module lacked `Vec`**
- **Found during:** Task 1, first `cargo osdk test` build (E0425/E0433). `check.sh --no-kvm` had passed, because the static gates do not compile `cfg(ktest)` code.
- **Fix:** `use alloc::vec::Vec;` in the `agent.rs` test module.
- **Files modified:** `kernel/src/agent.rs`. **Commit:** `5a7494e`.

### Plan verify and acceptance items that do not behave as written (not fixed, out of scope)

- **`cargo osdk test request_ledger` filters out every test** (`0 passed; 0 failed; 71 filtered out`), under both `./osdk.sh cargo osdk test request_ledger` and `... aok::agent::test::request_ledger`. The runner's name filter does not match here. The plan's verify command would therefore fail its `ok. [1-9]` grep even with passing tests. I ran the full suite instead. The cause is not investigated.
- **The literal `test <name> ... ok` grep cannot match.** The runner interleaves DEBUG output, so `ok` lands on a following line (log lines 129 and 133). I confirmed both tests by reading those lines.
- **Acceptance "`grep -c 'unsafe'` prints 0 for both files"** returns 1 for `ledger_srv.rs`, from the `#![deny(unsafe_code)]` attribute every bin carries. It contains no `unsafe` block, `ledger.rs` has 0, and `unsafe_check` is ok. I kept the attribute.

### Design choices inside the plan's discretion

- Entries carry a `used` flag so an empty slot is distinct from tag 0.
- `ledger_srv` charges the cycles of a request before `reply_recv`, because `reply_recv` blocks in its receive half. The charged span is receive-return to just before the reply, not including the reply itself.
- The 0xFFFF stop label is handled before the ledger, so it is never refused. It is test control; the doc comment says a real server would not accept it from a client.
- One extra assertion in the ktest: tag 0xA is still refused after 0xB was served.

**Total deviations:** 1 auto-fixed (Rule 3), 3 plan-text mismatches logged. **Impact:** none on scope.

## Not verified

- Anything under KVM (see above).
- That a ring-3 client cannot choose its own tag. The claim rests on OD-13 (`thread_create` with `SPACE_SELF` inherits the tag), which plan 02-04 builds and is not in this tree. Today no ring-3 call creates a thread, so the property holds vacuously; ADR 0003 says so.
- That the defaults protect a real server. No Phase 4 server exists yet.
- Starvation of a server by one client inside the limit (named in ADR 0003 as what would show the decision wrong).
- `request_ledger_limit_answers_eagain_per_tag` uses kernel client threads, not ring-3 clients, so the kernel-stamped tag path from a ring-3 `call` into `IpcArgs.tag` is covered by 01-0x tests (`siblings_are_distinguished_by_tid_and_tag`), not by this one.

## Known Stubs

None. No TODO, FIXME or placeholder text in the created or modified code.

## Threat Flags

None. The plan's threat register (T-02-10 to T-02-13) is covered: T-02-10 by the per-tag limit and ktest; T-02-11 by keying on `IpcArgs.tag` (tag minting caveat above); T-02-12 accepted and recorded in ADR 0003; T-02-13 by the ADR sentence and PROJECT.md edits.

## Self-Check: PASSED

- Files exist: `userland/libsys/src/ledger.rs`, `userland/tests/src/bin/ledger_srv.rs`, `docs/architecture/adr/0003-server-cpu-exemption.md`.
- Commits reachable from HEAD: `5a7494e`, `176e699`.
- `git rev-list --count 09b184a..HEAD` = 2 at the time of writing (before this SUMMARY's commit).
