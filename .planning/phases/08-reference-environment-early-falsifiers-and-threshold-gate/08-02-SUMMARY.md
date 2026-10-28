---
phase: 08-reference-environment-early-falsifiers-and-threshold-gate
plan: 02
subsystem: aok-adv adversarial suite and stub broker
tags: [rust, adversarial-testing, cgroups, ptrace, io_uring, unsafe-ledger-exempt]
dependency-graph:
  requires: []
  provides:
    - tools/aok-adv (bin `aok-adv`): stub, run, list-routes, report subcommands
    - D-16 23-row route table (kill/ptrace/bind/impersonation, 17 sibling + 6 ancestor)
    - ADV-01 8-class S1 case skeleton
    - ADV-03 per-row JSON schema and report --merge-counters join/merge logic
  affects:
    - 08-03 (the --adv wrapper this suite expects for its cgroup subtree and stub restarts)
    - 08-05 (the reference-host agent-cgroup run that fills agent_result and deny_counter_delta)
tech-stack:
  added: []
  patterns:
    - "#![deny(unsafe_code)] crate-wide with exactly one #[allow(unsafe_code)] module (src/sys.rs); every unsafe block there carries a // SAFETY: comment"
    - "hand-rolled flat-object JSON (no serde): escape/unescape, a single-level object parser, route/case merge keyed on (route-or-case, topology)"
key-files:
  created:
    - tools/aok-adv/src/cases.rs
    - tools/aok-adv/src/report.rs
  modified:
    - tools/aok-adv/src/main.rs
    - tools/aok-adv/src/routes.rs
decisions:
  - "run's --socket is optional: the S1 case suite always runs; route rows run only when --socket names a live broker. The plan's artifacts_this_phase_produces line lists --socket without brackets (implying required); this plan treats it as optional so the case suite verifies stand-alone on WSL2 without a broker process."
  - "--cases and --routes take comma-separated filter lists, matching the existing --topology flag's shape; --broker-pid is checked for /proc liveness only, not cross-checked against the socket's peer credentials."
  - "report's two --in files are positional by order: first is the non-agent baseline, second is the agent-cgroup run. Nothing in the flag names enforces this order; it is an assumption of this plan's parser."
  - "dispatch_attempt maps route name to one of routes.rs's attempt_* functions; 'pathname' and 'abstract' bind routes, and payload_claim, reuse the single --socket value for both the pathname and the abstract-namespace name, since run's flag set has no separate --abstract-name."
actuals:
  tokens: 28300
  tasks: 3
  commits: 3
plan_head_before: cf8766e5ccc6a82660c210827a18b825cf0abd87
plan_head_after: fb1ab825610c8afb1505a5aedec319e8f97c84fd
requirements-completed: [ADV-01, ADV-02, ADV-03]
status: complete
---

# Phase 8 Plan 2: aok-adv adversarial suite and stub broker Summary

Rust crate `tools/aok-adv` (edition 2024, toolchain 1.99.1, `libc =0.2.191` only dependency):
a stub broker answering the kernel-stamped-identity protocol, the D-16 23-row route table
(kill/ptrace/bind/impersonation attempts with a non-agent baseline reachability rule), the
ADV-01 8-class S1 side-effect case skeleton, and a hand-rolled JSON report/merge pipeline —
all building and unit-testing on WSL2, with the agent-cgroup run itself deferred to the
reference host (08-05).

## What was verified (this session, on WSL2, uid 1000, no root, no /dev/kvm, no BPF LSM, ptrace_scope=1, no delegated cgroup)

- `cargo test`: 25 tests pass (16 from Tasks 1-2 plus 9 new: 5 in `cases.rs`, 4 in `report.rs`).
- `cargo fmt --check`: clean.
- `cargo clippy -- -D warnings`: clean (fixed 5 `collapsible_if` findings — 2 pre-existing in
  `cases.rs`, 2 new in `main.rs`, using edition-2024 let-chains; 1 `dead_code` finding on
  `DenyCounterLine::hook`, resolved with a scoped `#[allow(dead_code)]` and a doc note that it's
  kept for a future per-hook breakdown).
- `cargo run -- list-routes`: 23 rows, 17 sibling / 6 ancestor, confirmed by direct count.
- Hard grep rules, all re-run after Task 3's edits: `kill\((0|-1)[,)]` matches 0 lines anywhere
  in `src/`; `src/sys.rs` has 31 `// SAFETY:` comments and 31 bare `unsafe` occurrences (count
  equal, as required); `main.rs`, `stub.rs`, `routes.rs`, `cases.rs`, `report.rs` each have 0
  bare-word `unsafe` matches (one doc-comment occurrence in `cases.rs`'s module header was
  reworded to avoid the literal word, matching the same fix already applied to `sys.rs` and
  `routes.rs` in Task 2).
- Manual smoke test beyond the plan's required verify commands (not a committed test, run and
  discarded): started the stub broker on a real `AF_UNIX` socket, ran `aok-adv run --no-cgroup`
  with no `--socket` (produced exactly 8 case rows, all `baseline_ok: true` on this host,
  `ptrace_scope: "1"`), then ran it again with `--socket` against the live broker over a safe
  subset of routes (excluding the two bind routes, which would have unbound the broker's own
  socket out from under it). Observed: `kill`/`tgkill`/`rt_sigqueueinfo`/`pidfd_send_signal`
  reachable; `cgroup.kill`/`cgroup.procs`/`clone3_CLONE_INTO_CGROUP` skipped
  (`skipped: needs delegated cgroup`) under `--no-cgroup`; `PTRACE_ATTACH`/`PTRACE_SEIZE`/
  `pidfd_getfd` reachable in both topology rows (this host's descendant relationship satisfies
  Yama scope 1); `process_vm_readv`/`process_vm_writev`/`/proc/pid/mem` void in both topology
  rows (same-uid cross-memory access without an active ptrace attach is void under scope 1, as
  RESEARCH A5 predicts); `scm_rights`/`payload_claim` reachable. Then fed that JSONL plus a
  synthetic one-line agent JSONL and a synthetic `deny-counters.txt` through
  `aok-adv report --merge-counters`: the merged output and `report.json` correctly picked up
  `agent_result` and summed `deny_counter_delta` (3, from `(6-4)+(1-0)`) for the one
  `(kill, sibling)` key, left every other row's `agent_result`/`deny_counter_delta` as `-`/null,
  and the report text contained both the "0 of about 8 is not evidence of completeness" and
  "Read-then-act race" lines. `report.json` round-tripped through `python3 -I json.load`.

## Not run (needs reference host, per user instruction — recorded, not fabricated)

- The actual agent-cgroup runs (08-05): `agent_result` and `deny_counter_delta` on every route
  row in this plan's own test output are synthetic/null, never measured against a real BPF-LSM
  denial.
- The non-agent baseline run on the reference host (as opposed to this WSL2 smoke run).
- `deny_counter_delta` against the pinned per-hook map (08-05's wrapper produces
  `deny-counters.txt`; this plan only unit-tests the parser and the merge arithmetic on
  synthetic input).
- The delegated-cgroup execution of `cgroup.kill`, `cgroup.procs`, and
  `clone3_CLONE_INTO_CGROUP` (this host has no delegated cgroup subtree under
  `/sys/fs/cgroup/aok-phase8/adv/`, so every run here hit the `--no-cgroup` skip path, never the
  real write/clone3 path).
- Kernel behavior assumptions A1-A17 in `08-RESEARCH.md` remain assumed, not verified by this
  plan; this plan's `PTRACE_ATTACH` 'reachable' / `process_vm_readv` 'void' results on this host
  are *consistent with* A5's prediction for ptrace_scope 1, not a separate confirmation of A5
  itself.

## Known Stubs

1. **Ancestor topology is a label filter, not a different process relationship.**
   `dispatch_attempt` never branches on `row.topology`; the "ancestor" rows for the six ptrace
   routes run the exact same `spawn_target()`-based attempt as their sibling counterpart. The
   plan's own edge-coverage note (ADV-02 concurrency, index 32) and acceptance language describe
   ancestor topology as "the attacker forks the stub as its own child" so that Yama scope 1
   permits the attach via ancestry — that fork relationship is not implemented. The smoke-test
   "ancestor" reach values above are therefore not yet evidence of the ancestor-exemption
   behavior; they are duplicate measurements of the sibling case. Flagged per my dispatch
   instructions ("Do NOT implement ... the ancestor clone3 fork; record ... as Known Stubs").
2. **No per-row stub restart.** The plan's edge-coverage note (ADV-02 concurrency, index 27/32)
   calls for the runner to restart the stub broker before every row so one route's effect on the
   broker (e.g. the bind-hijack routes) never masks the next route's baseline. `run_cli` does not
   manage the broker process at all; the caller is responsible for keeping one broker alive (or
   restarting it) across the whole run. Flagged per the same dispatch instruction.
3. **Bind routes will break a live broker they're run against.** `attempt_bind_pathname` and
   `attempt_bind_abstract` unlink and rebind the broker's own socket name, by design (that's the
   exploit path under test) — but since nothing restarts the broker (stub #2 above), running
   `--routes pathname` or `--routes abstract` against a broker you still need for later rows will
   leave it unreachable afterward. The smoke test above deliberately excluded these two routes
   from its live-broker run for this reason.
4. **`--socket` is optional, diverging from the plan's literal CLI contract.** See the Decisions
   section in the frontmatter: the plan's `artifacts_this_phase_produces` line writes
   `aok-adv run --socket PATH [...]` without brackets around `--socket`, but this plan's
   `run_cli` treats it as optional (cases always run; routes only run when given) so the case
   suite is independently verifiable on WSL2 without standing up a broker.
5. **`report`'s two `--in` files are order-dependent (nonagent, then agent) with no flag naming
   to enforce it.** A caller that reverses the order gets a merge keyed on the wrong baseline
   silently (no error).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `cases.rs` module doc comment matched the crate's bare-`unsafe` grep rule**
- **Found during:** Task 3 verification (the same bare-word check Task 2 already fixed in
  `sys.rs` and `routes.rs`)
- **Issue:** The doc comment at the top of `cases.rs` read `All \`unsafe\` stays in \`sys\``,
  tripping the plan's `grep -w unsafe` rule even though no actual unsafe code is present in the
  file
- **Fix:** Reworded to "D-13's raw-syscall confinement holds here too" — same meaning, no literal
  match
- **Files modified:** `tools/aok-adv/src/cases.rs`
- **Commit:** `fb1ab82`

**2. [Rule 2 - missing functionality] clippy `collapsible_if` findings**
- **Found during:** `cargo clippy -- -D warnings` after wiring `main.rs`
- **Issue:** Two nested `if let ... { if ... { } }` patterns already present in `cases.rs` (the
  `model` and `API key` case closures) and two new ones added in `main.rs`'s filter logic tripped
  `-D warnings`
- **Fix:** Collapsed all four into single `if let ... && ... { }` let-chains (edition 2024)
- **Files modified:** `tools/aok-adv/src/cases.rs`, `tools/aok-adv/src/main.rs`
- **Commit:** `fb1ab82`

**3. [Rule 3 - blocking] `routes.rs`'s `#[allow(dead_code)]` sprinkling became stale**
- **Found during:** wiring `main.rs`'s `run_cli`/`dispatch_attempt` to every `routes::attempt_*`
  function, `evaluate_row`, `read_ptrace_scope`, `needs_cgroup`, `assert_cgroup_kill_path`,
  `CGROUP_ADV_PREFIX`, `SKIPPED_NEEDS_CGROUP`, `Reach::as_str`, and the `RunOutcome` enum
- **Issue:** Every one of these carried a Task-2-era `#[allow(dead_code)]` (clippy runs without
  `--tests`, so anything only reachable from `#[cfg(test)]` code needs the allow); once `main.rs`
  reaches them through its own call graph they are no longer dead, and the plan's own pending
  instructions called for removing attributes that become unnecessary
- **Fix:** Removed all 26 occurrences in `routes.rs` and the 1 in `cases.rs` (`run_all_cases`);
  rebuilt clean with none reintroduced by clippy
- **Files modified:** `tools/aok-adv/src/routes.rs`, `tools/aok-adv/src/cases.rs`
- **Commit:** `fb1ab82`

**4. [Rule 2 - missing functionality] `DenyCounterLine.hook` parsed but never read**
- **Found during:** `cargo build` after writing `report.rs`
- **Issue:** `rustc`'s dead-code lint flagged the `hook` field (parsed from `deny-counters.txt`
  but `merge` only sums by `(route, topology)`, never breaks out by hook)
- **Fix:** Added a scoped `#[allow(dead_code)]` on the field with a doc note that it is kept for
  a possible future per-hook breakdown, rather than deleting a field the schema explicitly
  documents
- **Files modified:** `tools/aok-adv/src/report.rs`
- **Commit:** `fb1ab82`

No architectural (Rule 4) deviations. No auth gates encountered.

## Carried-over deviations from Task 2 (already committed in `aad5db6`, restated for completeness)

- `sys.rs` is outside Task 2's declared `<files>` list but was touched there (added
  `waitpid_reap`, two `msghdr` `// SAFETY:` comments, and a reworded kill-guard error string)
  because the plan's own kill-grep rule matched a doc string committed in Task 1.
- Local `PTRACE_DETACH=17` define (not in the pinned `libc =0.2.191`'s public constants on this
  target).
- Mixed-case `SYS_lsm_list_modules` constant with a scoped lint allow.

## Out-of-scope files touched (one line each, none from this plan's `<files>` list)

- None in Task 3. (`Cargo.lock` was committed in Task 1, outside that task's `<files>` list — see
  the 08-02 Task-1/2 record; not touched again in Task 3.)

## `unsafe_check.py` exemption proof (re-confirmed, file untouched)

`tools/unsafe_check.py`'s `rs_files()` (lines 100-102) globs only `kernel/src/**/*.rs` and
`userland/*/src/**/*.rs`; its `crate_roots()` (lines 105-108) only resolves
`kernel/src/lib.rs`, `userland/*/src/lib.rs`, and `userland/tests/src/bin/*.rs`. None of these
patterns matches anything under `tools/aok-adv/`, confirming the ledger-exemption decision
(D-13) this plan relies on: `tools/aok-adv`'s single `#[allow(unsafe_code)] mod sys` is outside
the kernel unsafe ledger's scan scope, not merely excused from it by convention. This tension —
an `unsafe`-allowing module that is structurally invisible to the ledger script rather than
formally exempted inside it — is not settled by this plan.

## Threat Flags

None. `cases.rs`'s three loopback listeners (network, model, API-key cases) and its dummy-secret
file are exactly the surface the plan's own threat register already dispositions under T-08-08
("API-key case ... a dummy file `aok-dummy-key-<runid>` with an asserted prefix, sent only to
127.0.0.1; no real secret is ever read or transmitted") and the "suite → filesystem/network"
trust boundary row. No new surface outside the registered threat model was introduced.

## Self-Check: PASSED

- `tools/aok-adv/src/{main,sys,stub,routes,cases,report}.rs`, `Cargo.toml`, `rust-toolchain.toml`,
  `Cargo.lock`: all FOUND.
- Commits `11796a1`, `aad5db6`, `fb1ab82`: all confirmed ancestors of current `HEAD`
  (`fb1ab825610c8afb1505a5aedec319e8f97c84fd`) via `git merge-base --is-ancestor`.
- `git rev-list --count cf8766e5ccc6a82660c210827a18b825cf0abd87..HEAD` = 3, matching `commits: 3`
  in the frontmatter.
