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

## Gap closure

The merged crate failed several `must_haves.truths` because the route functions attacked a
throwaway self-spawned child instead of the stub broker, topology was a label, nothing started
or restarted the stub, `--mode agent` did not place anything in a cgroup, and the D5 oracles
did not check their own postconditions. This section fixes those five defects, documents a
sixth found during end-to-end verification, and re-checks every `must_haves.truths` entry
against the fixed code.

### D1: routes attacked a throwaway child, not the given target

**Root cause:** every `attempt_*` function in `routes.rs` spawned its own `/bin/sh -c "sleep 5"`
child and attacked that, ignoring whatever broker or stub the plan actually describes.

**Fix (`tools/aok-adv/src/routes.rs`):** every `attempt_*` function now takes a `Target { pid,
socket_path, abstract_name }` parameter and performs its syscall against exactly that target;
none of them spawns anything. `main.rs` owns every process this suite ever signals, ptraces, or
connects to.

**Commit:** `b42f8cd`

### D2: topology was a label, ancestry was never real

**Root cause:** nothing in the pre-fix code forked the stub as the attacker's own child; both
"sibling" and "ancestor" rows ran the identical self-spawned-child attack (the original
SUMMARY's own Known Stub #1 names this directly).

**Fix (`tools/aok-adv/src/main.rs`):** sibling topology spawns the stub and the attacker as the
runner's own two children (`run_one_row`'s `Sibling` branch). Ancestor topology re-execs into a
hidden `attempt` subcommand that spawns the stub itself as its own child, waits for it to
accept, and (in agent mode) places it in the broker cgroup before the attacker would enter its
own agent cgroup, so the stub's parent is the attacker, not the runner. Verified directly:
`tests/topology_and_lifecycle.rs`'s `sibling_topology_shares_the_runner_as_parent_for_stub_and_attacker`
and `ancestor_topology_makes_the_attacker_the_stubs_parent` read `/proc/<pid>/stat`'s ppid field
for both processes in both topologies.

**Commit:** `ac7bb69`, `9a7562d` (tests)

### D3: nothing started or restarted the stub

**Root cause:** `run_cli` never spawned a stub broker at all; the plan assumed an external,
already-running broker that nothing in the pre-fix binary could start, restart, or tear down.

**Fix (`tools/aok-adv/src/main.rs`):** `run` now owns the stub lifecycle end to end per row:
`spawn_stub` re-execs `current_exe()` into the hidden `stub` subcommand, `wait_for_stub_ready`
polls a real connect attempt (bounded by `STUB_ACCEPT_TIMEOUT` = 5s) instead of sleeping a fixed
guess, and `kill_and_reap` tears it down after the row. Every row gets a fresh stub child, never
a reused one. `--broker-pid` is rejected outright with an `io::ErrorKind::Unsupported` error
naming "ADV-05 (Phase 10)"; this suite never signals or ptraces a pid it did not spawn itself.
Verified: `tests/topology_and_lifecycle.rs`'s `consecutive_rows_get_a_fresh_stub_pid` parses two
consecutive rows' `stub_pid=` diagnostics from a real `run` invocation and asserts they differ;
`tests/broker_pid_rejected.rs` and `main.rs`'s own
`broker_pid_flag_is_rejected_with_adv05_message` cover the rejection at both the CLI and
in-process boundary.

**Commit:** `ac7bb69`, `9a7562d` (tests)

### D4: `--mode agent` was a label, nothing was placed in a cgroup

**Root cause:** `--mode` was parsed and stamped onto the JSON `mode` field but never consulted
for process placement.

**Fix (`tools/aok-adv/src/main.rs`, `tools/aok-adv/src/routes.rs`):** in agent mode (and
`!no_cgroup`), the attacker is placed in `<cgroup-root>/agent-a` before it is signaled "go", the
stub is placed in `<cgroup-root>/broker`, and the impersonation helper (`scm_rights`'s
`scm-helper` subprocess) is placed in `<cgroup-root>/agent-b`, each via a `cgroup.procs` write
through `routes::place_in_cgroup`, itself guarded by the existing `assert_cgroup_kill_path`
prefix check. `--no-cgroup` skips both the cgroup-bearing route rows (unchanged `D-16` skip rule)
and all placement, even when `--mode agent` is given. Verified against an injectable tempdir
standing in for a delegated cgroup v2 mount (no real `/sys/fs/cgroup` access needed):
`routes.rs`'s `agent_mode_placement_writes_each_pid_to_its_own_leafs_cgroup_procs` (acceptance 4).

**Commit:** `b42f8cd`, `ac7bb69`

### D5: the oracles never checked their own postconditions

**Root cause:** every impersonation/bind route returned `Ok` on a bare round trip without ever
checking that the thing the route claims to prove actually happened.

**Fix (`tools/aok-adv/src/routes.rs`):**
- `cgroup.procs` (`attempt_cgroup_procs_move`): moves this process into `agent_b_dir`'s
  `cgroup.procs`, then asks the stub for the kernel-stamped identity on this connection and
  checks it equals `agent_b_dir`'s own inode.
- `scm_rights` (`attempt_scm_rights`): a real `scm-helper` subprocess connects to the stub and
  hands its connected fd to this process over a side-channel `SCM_RIGHTS` socket
  (`main.rs`'s `scm_helper_cli`); this process speaks to the stub through the borrowed fd, and
  when an `agent_b_dir` is given, checks the reply's identity against it.
- `payload_claim` (`attempt_payload_claim`): now fails when the reply's `cgroupid` does **not**
  equal this process's own kernel-stamped id (`own_cgroup_id`) — i.e. it is a control that
  passes only when the claimed id was correctly ignored. The pre-fix code returned `Ok` on any
  reply at all, including one that honored the claim. Verified directly:
  `payload_claim_fails_when_reply_carries_the_claimed_id` stands up a malicious stand-in stub
  that echoes the claimed id back and asserts the route rejects it (acceptance (d)).
- `clone3_CLONE_INTO_CGROUP` (`attempt_clone3_into_cgroup`): verifies the clone3 child actually
  landed in `target_cgroup_dir` (via `PIDFD_GET_INFO` on the pidfd `clone3` returns) before
  killing and reaping it, rather than assuming the flag worked.
- `pathname`/`abstract` binds (`attempt_bind_pathname`, `attempt_bind_abstract`): unlink/rebind
  the broker's socket name, then prove a **fresh** connect reaches the attacker's own listener
  (a thread-served fixed marker) rather than merely checking that `bind` itself returned `Ok`.
  `main.rs`'s `abstract` branch of `run_one_row` creates the kill-stub-then-bind-then-restart gap
  this route needs by killing the stub before dispatching and letting the next row's own fresh
  spawn start a new one afterward.
- `payload_claim` is excluded from the S5 tally and tagged `oracle: "control"` in `report.rs`
  (`is_control_route`), per D-15.

**Commit:** `b42f8cd`

### D6 (found during end-to-end verification, not one of the original five): `spawn_stub`'s abstract-mode flag set

**Root cause:** `stub::run_cli` unconditionally requires `--socket PATH` (the CLI contract is
`stub --socket PATH [--abstract NAME]`, never `--abstract` alone). `spawn_stub` originally
passed only one of `--socket`/`--abstract` depending on `abstract_mode`, so every `abstract`-route
stub child exited immediately with `"stub: --socket PATH is required"`. Because `spawn_stub`
sets the child's `stderr` to `Stdio::null()` (so a stub log line can never corrupt the
`stub_pid=`/verdict diagnostics the runner and `attempt` parse off their own stderr/stdout —
itself one of this gap closure's intentional fixes), that error message was silently swallowed.
The runner then spent a full 5 seconds in `wait_for_stub_ready` polling a socket nothing was
listening on, before failing with an `io::ErrorKind::TimedOut` that carries no `raw_os_error` —
surfacing as `"void (baseline failed)"` with `errno: null` and **no** `stub_pid=` diagnostic
line, which is how this was caught (the diagnostic was conspicuously missing from the `abstract`
row's stderr during the acceptance-3 end-to-end run, below).

**Fix (`tools/aok-adv/src/main.rs`):** `spawn_stub` now always passes `--socket socket_path`, and
additionally passes `--abstract socket_path` when `abstract_mode` is true. `stub::serve` already
binds the abstract name instead whenever one is given and never touches the socket-path argument
in that case, so this does not change the stub's own bind behavior, only satisfies `run_cli`'s
required-flag check.

**Commit:** `ac7bb69`

### Measured end-to-end table (acceptance 3)

Built release (`cargo build --release`), then ran `target/release/aok-adv run --socket
<fresh-dir>/s.sock --cgroup-root <fresh-dir>/c --routes <ROUTE> --topology <TOPO> --mode
nonagent --no-cgroup --out <fresh-dir>/o.jsonl` once per `list-routes` row, each under its own
temp directory with a fresh socket. Measured on this host: uid 1000, `ptrace_scope` = `1`
(confirmed, matches `ptrace_scope` field on every row below), no delegated cgroup under
`/sys/fs/cgroup/aok-phase8/adv/`.

| # | family | route | topology | reach | errno |
|---|--------|-------|----------|-------|-------|
| 1 | kill | kill | sibling | reachable | - |
| 2 | kill | tgkill | sibling | reachable | - |
| 3 | kill | rt_sigqueueinfo | sibling | reachable | - |
| 4 | kill | pidfd_send_signal | sibling | reachable | - |
| 5 | kill | cgroup.kill | sibling | skipped: needs delegated cgroup | - |
| 6 | ptrace | PTRACE_ATTACH | sibling | void (baseline failed) | 1 (EPERM) |
| 7 | ptrace | PTRACE_SEIZE | sibling | void (baseline failed) | 1 (EPERM) |
| 8 | ptrace | process_vm_readv | sibling | void (baseline failed) | 1 (EPERM) |
| 9 | ptrace | process_vm_writev | sibling | void (baseline failed) | 1 (EPERM) |
| 10 | ptrace | /proc/pid/mem | sibling | void (baseline failed) | 13 (EACCES) |
| 11 | ptrace | pidfd_getfd | sibling | void (baseline failed) | 1 (EPERM) |
| 12 | ptrace | PTRACE_ATTACH | ancestor | reachable | - |
| 13 | ptrace | PTRACE_SEIZE | ancestor | reachable | - |
| 14 | ptrace | process_vm_readv | ancestor | reachable | - |
| 15 | ptrace | process_vm_writev | ancestor | reachable | - |
| 16 | ptrace | /proc/pid/mem | ancestor | reachable | - |
| 17 | ptrace | pidfd_getfd | ancestor | reachable | - |
| 18 | bind | pathname | sibling | reachable | - |
| 19 | bind | abstract | sibling | reachable | - |
| 20 | impersonation | cgroup.procs | sibling | skipped: needs delegated cgroup | - |
| 21 | impersonation | scm_rights | sibling | reachable | - |
| 22 | impersonation | payload_claim | sibling | reachable (oracle: control, excluded from S5) | - |
| 23 | impersonation | clone3_CLONE_INTO_CGROUP | sibling | skipped: needs delegated cgroup | - |

This matches the acceptance-3 expectation exactly: all 6 sibling ptrace rows void with an
EPERM-class errno (5 literal `EPERM`, 1 `EACCES` — `/proc/<pid>/mem` is a VFS `open()` gated by
the same Yama `ptrace_access_check` under the hood, but the kernel surfaces the denial as
`EACCES` on that path rather than `EPERM`, which is still permission-denied class, not a
different failure mode), all 6 ancestor ptrace rows reachable, and all 3 cgroup-bearing rows
skipped under `--no-cgroup`. The kill family, both bind routes, and `scm_rights`/`payload_claim`
all reach their non-agent baseline on this host (same-uid, no LSM loaded), which is the expected
starting point before any agent-cgroup/BPF-LSM run (deferred to 08-05).

### Truth-by-truth re-check (`must_haves.truths`, `08-02-PLAN.md` lines 28-42)

1. **`cargo test`/`fmt --check`/`clippy -D warnings` clean on 1.99.1 (ADV-01).** MET. 34 tests
   pass (30 unit + `broker_pid_rejected` + 3 in `topology_and_lifecycle`), `fmt --check` and
   `clippy --all-targets -- -D warnings` both clean, confirmed after every commit in this gap
   closure.
2. **`#![deny(unsafe_code)]` crate-wide, exactly one `#[allow(unsafe_code)]` module
   (`src/sys.rs`), every unsafe block carries `// SAFETY:` (D-13).** MET. `main.rs` line 1 is
   `#![deny(unsafe_code)]`, line 13 is the single `#[allow(unsafe_code)] mod sys;`. `sys.rs` has
   33 bare `unsafe` occurrences and 33 `// SAFETY:` comments (the 3-occurrence gap between that
   and the raw `grep -c unsafe` count of 36 is three doc-comment mentions of the word, not code);
   the two functions added for this gap closure (`pidfd_identity`, `owned_fd`) each carry their
   own `// SAFETY:` comment.
3. **libc `=0.2.191` pin; `sys.rs` takes `SO_PEERPIDFD`/`PIDFD_GET_INFO`/`PIDFD_INFO_CGROUPID`/
   `pidfd_info` from libc with the 64-byte/offset-8 compile-time probe (RESEARCH A6).** MET,
   unchanged by this gap closure — `sys.rs`'s pre-existing `pidfd_identity`-adjacent code and the
   probe were not touched beyond adding `pidfd_identity` itself (which reuses the existing
   `PeerIdentity`/`PIDFD_GET_INFO` plumbing) and `owned_fd`.
4. **`sys.rs` hand-defines only `CLONE_INTO_CGROUP`, Landlock, `SYS_lsm_list_modules`, io_uring
   structs; everything else from libc (D-13).** MET, unchanged — no new hand-defined constant was
   added in this gap closure.
5. **Stub HELLO/OK protocol, claimed id ignored, 4-byte LE framing, 4096 cap, unknown verbs
   rejected (ADV-02).** MET, unchanged — `stub.rs` was not modified in this gap closure.
6. **`list-routes` enumerates exactly the D-16 routes plus route (iv) (ADV-03, D-16).** MET.
   `all_rows_has_23_entries_17_sibling_6_ancestor` and
   `exactly_the_six_ptrace_routes_carry_an_ancestor_row` (both pre-existing, re-verified) and the
   `list-routes` output captured for the measured table above both confirm 23 rows, 17 sibling +
   6 ancestor, the six ancestor rows exactly the six ptrace routes.
7. **Reach rule: `void (baseline failed)` when the non-agent baseline fails, `reachable` when it
   succeeds; S5 counts only reachable rows from an agent cgroup (ADV-02, ADV-03).** MET for the
   non-agent half, now backed by a baseline that actually exercises the real target instead of a
   throwaway child (D1) — see the measured table. The agent-cgroup half of the S5 count is
   BACKSTOP, deferred to 08-05 per the plan's own line 302 note; `payload_claim`'s exclusion from
   that count (D-15) is implemented and unit-tested
   (`payload_claim_is_excluded_from_the_s5_tally` in `report.rs`, pre-existing).
8. **Yama `ptrace_scope` table; measured, not predicted, reach per row (ADV-02, RESEARCH A5).**
   MET, now with a real measurement instead of the original SUMMARY's acknowledged duplicate:
   the measured table above shows exactly the scope-1 row documented in the plan (6 sibling void,
   6 ancestor reachable), and the real ancestor-topology fork (D2) is what makes this
   measurement meaningful rather than a relabeled sibling run.
9. **JSON fields and `report --merge-counters` join (ADV-03).** MET. Every row in the measured
   table carries exactly `class, case, oracle, baseline_ok, valid, reach, errno, closable_by,
   route, topology, family, mode, agent_result, deny_counter_delta, ptrace_scope` (confirmed by
   inspecting the raw JSONL); the `errno` field is now populated with a real value on failure
   (D5) rather than always `null`.
10. **S1 skeleton, ~8 cases, one per class, "0 of about 8 is not evidence of completeness"
    (ADV-01, D-17).** MET, unchanged — `cases.rs` was not modified in this gap closure (out of
    scope per the dispatch instructions); its pre-existing behavior and tests are untouched. See
    "Additional defects found, not fixed" below for two `cases.rs`-adjacent issues this gap
    closure surfaced but did not fix.
11. **`--topology sibling|ancestor` full D2 semantics, stub restart before every row,
    `ptrace_scope` read once, `--no-cgroup` skip rule (ADV-02).** MET. This is the core of D2/D3;
    see those sections above and `tests/topology_and_lifecycle.rs`.
12. **Every target spawned and pidfd-tracked by the suite; never `kill(0)`/`kill(-1)`;
    `cgroup.kill` asserted under `/sys/fs/cgroup/aok-phase8/adv/` (ADV-02 safety).** MET with one
    caveat: every target's pid now genuinely originates from a `Command::spawn()` this suite
    issued (D1), and `sys::kill`/`tgkill`/`rt_sigqueueinfo` never take a hardcoded `0`/`-1`.
    "pidfd-tracked" applies literally to the routes whose own definition is pidfd-based
    (`pidfd_send_signal`, `pidfd_getfd`, and the clone3 route's returned pidfd); the plain
    `kill`/`tgkill`/`rt_sigqueueinfo` routes signal `target.pid` directly, which is correct for
    testing those specific syscalls rather than a gap. `assert_cgroup_kill_path`'s prefix guard
    is unchanged and still covers every cgroup write, including the two new placement call sites
    added for D4.
13. **(backstop) Non-agent baseline of every ADV-01 case and ADV-02 route succeeds on the
    reference host, proving a real path; a non-succeeding case is marked invalid and excluded.**
    BACKSTOP, formally deferred to 08-05. Informally exercised during this gap closure on this
    dev host (not the project's designated reference host) — see the measured table. Read
    literally against that table, the 6 sibling ptrace rows do *not* succeed; that is the
    expected, documented Yama behavior from truth 8, not an "invalid" case needing exclusion —
    the two truths use "baseline" in compatible but not identical senses (D-16's reach rule vs.
    this truth's path-reachability claim), which this gap closure did not need to reconcile
    since both are consistent with what actually measured.
14. **(backstop) Agent-cgroup BPF-LSM `deny_counter_delta` per route, reference host.**
    BACKSTOP, deferred to 08-05 as before. Not exercised in this gap closure (no BPF-LSM hooks
    loaded on this host).
15. **(backstop) The `PIDFD_GET_INFO` read-then-act race is named in the report, not hidden.**
    BACKSTOP/unchanged — `report.rs`'s report text (not touched in this gap closure) already
    carries the "Read-then-act race" line per the original Task 3 implementation.

### Deviations chosen (gap closure)

- The stub is placed in the broker leaf for `cgroup.kill` even in non-agent mode, whenever that
  row actually needs a cgroup; placement is otherwise a no-op outside agent mode.
- Placement uses `cgroup.procs` writes (`routes::place_in_cgroup`) for the attacker/stub/helper,
  not `clone3 CLONE_INTO_CGROUP` — that flag is reserved for route (iv)'s own test subject, a
  fresh clone3 child, not for placing the suite's own already-running processes.
- `--no-cgroup` with `--mode agent` degrades to no placement at all (consistent with D-16's
  existing `--no-cgroup` skip rule for cgroup-bearing routes).
- The `--socket` string doubles as the abstract-namespace name for the `abstract` route and for
  `payload_claim`'s stand-in socket; `run`'s flag set has no separate `--abstract-name` (restated
  from the original SUMMARY's Decisions section, unchanged).
- The S1 case suite is suppressed whenever `--routes` is given to `run`, so a route-focused
  invocation (which `bpflsm.sh`'s `mode_adv` always is — see below) never also re-runs and
  re-appends all 8 cases to the output file on every row.
- `dispatch_attempt` passes `None` for `scm_rights`'s `agent_b_dir` argument (the identity check
  against a specific cgroup is only exercised when a caller passes `Some`, which `dispatch_attempt`
  itself never does); see "Additional defects found, not fixed" below.
- `AOK_ADV_TEST_PAUSE_MS` is a test-only environment-variable hook in the hidden `attempt`
  subcommand, read only when set, used by `tests/topology_and_lifecycle.rs` to observe the
  ancestor stub's parentage before the route attempt can affect it.

### Additional defects found, not fixed (outside this plan's five, per dispatch instructions)

1. **`cases.rs`'s S1 suite ignores `--mode` entirely.** `run_all_cases` takes no mode parameter;
   the 8 S1 cases run identically whether `--mode` is `baseline`, `agent`, or `nonagent`. Agent
   mode is a pure label for the case suite, exactly as the original SUMMARY's Known Stub #1
   described for topology before this gap closure fixed that for the *route* table. Not fixed:
   `cases.rs` is outside this gap closure's scope.
2. **`bpflsm.sh`'s `mode_adv` never passes `--cases` or omits `--routes`.** Reading
   `tools/bpflsm/bpflsm.sh`'s `mode_adv` (lines ~787-822, read-only, not modified): it calls `run
   --routes "$route" --topology "$topo" --mode "$m" ...` once per `list-routes` row and never
   once without `--routes`. Combined with this gap closure's "suppress cases when `--routes` is
   given" deviation (chosen specifically to stop 184 duplicate case rows — 8 cases x 23
   wrapper-invoked rows — from being appended to `$m.jsonl`, which is what the pre-fix,
   non-suppressing code would have done), the wrapper's `--adv` mode never produces any S1 case
   rows in its output at all. Whether the S1 case suite is meant to be wired into `mode_adv`
   through a separate invocation, a different wrapper mode, or not at all, is not resolved by
   this gap closure and is not a `tools/bpflsm/bpflsm.sh` change I am authorized to make under
   this dispatch's constraints; flagging for the plan/phase that owns that wiring.

### Verified (command and result)

- `cd tools/aok-adv && cargo test`: 34 tests pass (30 unit across `main.rs`/`routes.rs`/
  `stub.rs`/`cases.rs`, 1 in `tests/broker_pid_rejected.rs`, 3 in `tests/topology_and_lifecycle.rs`).
- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo build --release`: succeeds.
- `target/release/aok-adv list-routes`: 23 rows, confirmed by direct count and by the pre-existing
  `all_rows_has_23_entries_17_sibling_6_ancestor` test.
- All 23 `list-routes` rows run individually via the release binary with `--mode nonagent
  --no-cgroup`, fresh socket and cgroup-root paths per row: measured table above. `ptrace_scope`
  read as `"1"` on every row.
- Acceptance tests (a)-(e), all added or adapted in this gap closure, pass against the fixed
  code: (a) `kill_route_acts_on_given_pid_and_spawns_no_extra_child`, (b)
  `sibling_topology_shares_the_runner_as_parent_for_stub_and_attacker` +
  `ancestor_topology_makes_the_attacker_the_stubs_parent`, (c)
  `consecutive_rows_get_a_fresh_stub_pid`, (d)
  `payload_claim_fails_when_reply_carries_the_claimed_id`, (e)
  `broker_pid_flag_is_rejected_with_adv05_message` +
  `broker_pid_is_rejected_at_the_cli_with_the_adv05_message`. All five fail on the pre-fix code
  by construction: the pre-fix binary had no `attempt` subcommand, no stub spawn from `run` at
  all, a zero-argument `attempt_kill` that always spawned its own `sleep 5` child regardless of
  what was passed, and a `payload_claim` that returned `Ok` on any reply without checking it —
  none of the pre-fix code paths these tests exercise existed in a form that could pass them.

### Not verified

- Agent-mode placement against a real delegated cgroup v2 mount (this host has none under
  `/sys/fs/cgroup/aok-phase8/adv/`); covered only by the injectable-tempdir unit test
  (acceptance 4).
- BPF-LSM denial / `deny_counter_delta` (no BPF-LSM hooks loaded on this host; backstop, 08-05).
- The three cgroup-bearing route rows (`cgroup.kill`, `cgroup.procs`, `clone3_CLONE_INTO_CGROUP`)
  end to end against a real cgroup: all three measured `skipped: needs delegated cgroup` on this
  host, same as before this gap closure.
- `scm_rights`'s agent-mode identity check (`attempt_scm_rights`'s `agent_b_dir: Some(...)` path):
  reachable only through `dispatch_attempt` passing `Some`, which it never does (see "Additional
  defects found" above); the `None` path (a real borrowed, connected fd obtained and used,
  without an identity check against a specific cgroup) is what the measured table's `scm_rights`
  row exercised.
- Whether this dev host is the project's formally designated "reference host" for the backstop
  truths (13-15); the measurements in this gap closure are offered as evidence gathered while
  fixing the five defects, not as a substitute for the 08-05 reference-host run itself.
