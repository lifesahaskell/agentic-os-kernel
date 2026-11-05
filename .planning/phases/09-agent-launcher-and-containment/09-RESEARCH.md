# Phase 9: Agent launcher and containment - Research

**Researched:** 2026-11-05
**Domain:** Linux unprivileged process containment — cgroup v2, clone3/CLONE_INTO_CGROUP, Landlock, seccomp-bpf, pidfd identity
**Confidence:** HIGH (8 of 9 research items VERIFIED by probe on this host or by reading v6.18 kernel source this session; one — the libc-fallback claim in D-01 — stays ASSUMED pending the final allowlist)

## Summary

Every mechanism Phase 9 relies on was exercised unprivileged on this host (WSL2, kernel
6.18.33.2, Landlock ABI 7) inside a `systemd-run --user -p Delegate=yes` scope, or read from the
v6.18 kernel source. The locked decisions survive: the 16-layer Landlock cap and its `E2BIG` errno
(D-18) are exact; `cgroup.freeze`/`cgroup.kill` on a non-leaf agent directory reach the whole
subtree (D-07, LNCH-05); the cgroup id equals the directory inode and `PIDFD_GET_INFO` returns it
(D-09, LNCH-02); `user.*` xattrs on a cgroup directory are forgeable under a Landlock domain
(D-10); seccomp filters cannot be removed or weakened by stacking (D-03, LNCH-03); and the
no-internal-process rule bites exactly as D-07 assumes, which is what makes the `<root>/<sub>/main`
layout buy memory.max/pids.max for sub-agents.

**One research item resolves *in favour* of a locked decision that the CONTEXT flagged as a
possible design finding:** cgroup2 directory **rename is impossible at the kernel level** (EPERM,
because `cgroup_kf_syscall_ops` has no `.rename`), so a sub-agent's path-derived `agent_id` cannot
be changed by rename. D-06 is safe; this is **not** the feared finding. The real identity-mutation
vector is research item 9 — an agent **can** move a task between leaves inside its own subtree
(`cgroup.procs` write succeeds) — which reattributes that task's `agent_id`. That is by design and
is a candidate ADV case, not a defect.

**Primary recommendation:** Build `aok-launch` against the **`landlock` 0.4.7** and
**`seccompiler` 0.5.0** crates for the Landlock/seccomp layers (both verified legitimate; neither
is forbidden — STACK.md bars `rustix` in *user* code only, not in `tools/`), and copy the
`clone3`/pidfd/`CLONE_INTO_CGROUP` primitives from `tools/aok-adv/src/sys.rs`. Using the two crates
collapses most of the hand-rolled `unsafe` the C baseline carries into audited library code,
leaving `sys.rs`-style raw syscalls only for clone3 and the pidfd ioctl.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions (D-01 .. D-18 — do not re-litigate; research is how to implement, what breaks them)

**Seccomp filter (LNCH-06)**
- **D-01:** Denied syscall the filter's table knows → `EPERM`; number above the table's highest →
  `ENOSYS` (newer libc probing a new syscall falls back as on an older kernel). The claim that
  typical probe code handles only `ENOSYS` is from memory; verify against musl and glibc.
- **D-02:** `clone3` is **allowed**; its flags sit behind a pointer the filter may not read, so
  namespace flags go unseen by seccomp. Gap recorded; ADV-04/Phase 11 close it with the BPF-LSM
  `userns_create` hook. Also lets an agent run `aok-launch` for a sub-agent with
  `CLONE_INTO_CGROUP` (D-11).
- **D-03:** Default allowlist **wide, narrow profiles opt-in**. Covers glibc, musl, BusyBox `ash`,
  os-test; denies only `bpf`, mount family, `ptrace`, `perf_event_open`, `kexec_*`, module
  loading, `userfaultfd`, `io_uring_*`. A narrow profile must still include the launcher's own
  syscalls or an agent cannot spawn a sub-agent (a usable leaf-agent property).
- **D-04:** Filter decides on **syscall number and architecture only**; inspects no pointer
  argument (what lets the seccomp action cache serve it). Cache was ASSUMED; verify.
- **D-05:** Arch mismatch (32-bit compat, x32) returns `ERRNO`, same as any denial — LNCH-06 stays
  one invariant. Launcher differs from `sandbox.c:168` (`KILL_PROCESS`) on purpose.
- **Consequence:** C1b's 4.03%/3.31% were measured with the 64-entry filter, not the wide one;
  they do not transfer. D-16 depth runs wrap the real launcher.

**Identity and registry (LNCH-01, LNCH-02)**
- **D-06:** `agent_id` = cgroup path from the root agent (e.g. `r7/s1/s2`), set by `mkdir`.
  `parent` = parent directory. `on_behalf_of` lives in a launcher-written registry, one entry per
  root agent. Reversibility costly (Phase 10/11 key on this identity shape).
- **D-07:** Each agent = a directory plus a `main` leaf holding its processes:
  `<root>/<sub>/<sub>/main`. The agent directory holds no processes, so it can enable `+memory
  +pids` for its sub-agents (no-internal-process rule). `cgroup.freeze`/`cgroup.kill` on the agent
  directory reach the whole subtree. Reversibility costly.
- **D-08:** `on_behalf_of` fixed at root launch, inherited by tree position; names the human
  principal. No sub-agent registry write; no registrar service. Registry keyed by
  `(boot_id, cgroup_id)` of the root agent's directory, not readable/writable under an agent grant.
- **D-09:** Resolution: pidfd → `PIDFD_GET_INFO`+`PIDFD_INFO_CGROUPID` → `/proc/PID/cgroup` path,
  checked against id → drop `main` → nearest registered root ancestor gives `on_behalf_of`.
  Fallback (no `PIDFD_GET_INFO`): `SO_PEERCRED` + `/proc/PID/cgroup`, racy vs pid reuse; test
  reports the race. "cgroup id == directory inode" was ASSUMED.
- **D-10:** `user.*` xattrs on the cgroup directory are **rejected** as an identity store. A
  Landlocked same-uid process can overwrite/remove them. Re-create the scratch probe as a repo
  negative-control test.

**Launcher shape and fallbacks (LNCH-01 .. LNCH-05)**
- **D-11:** Launcher = Rust in a new crate under `tools/` (library + CLI `aok-launch`), following
  `tools/aok-adv`: edition 2024, stable toolchain pinned, `libc` pinned, all `unsafe` in one `sys`
  module with `// SAFETY:`, outside `tools/unsafe_check.py` and the unsafe ledger. Unlike aok-adv,
  the launcher runs in the agent's domain and sets up containment, so D-13's "test tooling, not
  TCB" rationale does not carry over. `sys.rs` already has `clone3_fork_into_cgroup`, `pidfd_open`,
  `pidfd_send_signal`, `send_fd`, `recv_fd`. `cg.c`/`sandbox.c` are rewritten, not wrapped.
- **D-12:** One CLI serves root and sub launches. An agent spawning a sub-agent execs `aok-launch`
  as its own child, so the spawn inherits the parent's cgroup, Landlock domain and seccomp filter;
  layering/stacking come from the kernel. CLI distinguishes root vs sub by its own cgroup path and
  the registry. No daemon. The agent's ruleset must grant exec of the binary and write to its own
  agent directory — an authority ADV probes.
- **D-13:** Fallback paths tested through an external seccomp shim that fakes the missing feature
  with an older kernel's errno (`ENOTTY` for the `PIDFD_GET_INFO` ioctl by request number; `ENOSYS`/
  `EINVAL` for `clone3`). Launcher sees a real errno; no test-only branch ships. Results labelled
  "simulated on a kernel that has the feature". This host lacks `CLONE_NNP` (arrives 7.1), so the
  `prctl(PR_SET_NO_NEW_PRIVS)` path runs natively and the `CLONE_NNP` success path cannot be
  exercised this phase.
- **D-14:** Below the Landlock floor the launcher **refuses to launch**: non-zero exit, named
  errno, message naming the missing right. Floor (proposed ABI 6) is one constant read from
  config. No `--allow-degraded`.

**Landlock layering against the S3 bound (LNCH-03)**
- **D-15:** Phase 9 builds the Landlock fs layering and keeps it. Landlock fs alone (C2a) adds
  21.04% p50 / 14.95% to openat, expected to miss the proposed S3 bound; this phase gives **no S3
  verdict** (that is the reference host + gate). Whether file narrowing moves to BPF-LSM stays open.
- **D-16:** Indicative openat read at layer depth 1/2/4 on this host with the real launcher, via an
  additive `--wrap` hook in `run.sh` (08 D-07 `--tag` pattern). `micro.c`/`sandbox.c` unchanged;
  C0 rows are the baseline; `summarize.py`/`check_report.py` apply unchanged. Plan 08-05 also uses
  `run.sh`; the edit stays additive and is flagged to Phase 8. "Depth raises cost" is a hypothesis.
- **D-17:** Apply the **Landlock ABI 6 feature set on every host**: fs rights, TCP connect rules,
  abstract-unix-socket and signal scopes. Newer rights not used (opt-in profile later). **One
  ruleset per spawn** — a sub-agent adds one layer.
- **D-18:** Nesting cap = kernel stacked-ruleset limit, **16 layers** (verify number + errno).
  Exceeding → kernel's `E2BIG`. Revisited with D-16 data.

### Claude's Discretion
- Names/locations: crate directory, registry file, `--wrap` flag name, config file for the
  Landlock floor constant. Registry likely under `$XDG_RUNTIME_DIR` (reboot clears it; `boot_id`
  in the key is the second guard).
- Registry binding to a boot, lifetime across reboots, cleanup of dead-root entries.
- The exit contract (default: leader pidfd readable = leader exited; `populated 0` in
  `cgroup.events` = subtree drained).
- The exact wide-allowlist and deny set (D-03), and the narrow-profile format.
- The documented errno for each denial class D-14/D-18 do not fix.
- Whether the launcher shares `aok-adv/src/sys.rs` as a path dependency, copies it, or extracts a
  shared crate (aok-adv is pinned test tooling; a dependency that direction needs a look).
- How the depth runs report row eligibility (reuse the R1 rule, as 08 D-18 does).
- The agent's default grant (only needs to run the LNCH tests and `sh -c`; S9 full grant is
  Phase 12).

### Deferred Ideas (OUT OF SCOPE)
- clone3 namespace-flag mediation → Phase 11 (BPF-LSM `userns_create`, ADV-04).
- S9 full-grant profile and per-distro matrix → Phase 12.
- Newer Landlock rights (ABI 7 audit flags, ABI 10 UDP) as an opt-in profile → after ABI 6 base
  measured.
- Layer elision for no-op layers.
- `--allow-degraded` launch mode — not built.
- Moving file narrowing to BPF-LSM — gate/Phase 11 decision.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LNCH-01 | Spawn each agent with `clone3`+`CLONE_INTO_CGROUP` into its own cgroup v2 cgroup in a delegated subtree; sub-agents are child cgroups; fork-move-exec fallback on kernels without the flag | Item 2 (VERIFIED clone3 into owned leaf; EBUSY into an internal/unowned node); reuse `sys.rs::clone3_fork_into_cgroup`; fallback = `cg.c` fork→`cgroup.procs` move→exec, window reported |
| LNCH-02 | Identity record bound to cgroup id; peer resolves via `PIDFD_GET_INFO`+`PIDFD_INFO_CGROUPID`; fallback `SO_PEERCRED`+`/proc/PID/cgroup`, racy | Item 5 (VERIFIED cgroupid==inode==5335; `PIDFD_GET_INFO` returns it). D-09 fallback race is real and must be reported |
| LNCH-03 | Sub-agent gets a subset of parent authority: Landlock layered, seccomp stacked, `no_new_privs` set | Items 1 & 3 (VERIFIED 16-layer stack, E2BIG at 17; seccomp cannot be removed or weakened by stacking — lowest action wins) |
| LNCH-04 | Non-agent child (`sh -c`) inherits cgroup + Landlock domain + seccomp filter | Landlock/seccomp are per-thread and inherited across fork/exec unconditionally; cgroup membership inherited unless moved. Item 9 shows the only way out is an explicit `cgroup.procs` write the grant can withhold |
| LNCH-05 | Freeze/thaw/kill whole subtree via `cgroup.freeze`/`cgroup.kill`; report exit via pidfd | Item 7 (VERIFIED: freezing the non-leaf agent dir freezes the sub-leaf; thaw clears it; `cgroup.kill` reaps subtree) |
| LNCH-06 | Denials return standard errno; filter makes no pointer-argument decision | Items 4 & 6 (VERIFIED seccomp `SECCOMP_RET_ERRNO` clamps to errno; filter loads only `nr`/`arch`; action cache serves const-allow syscalls) |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Spawn into own cgroup | Kernel (cgroup v2 + clone3) | `aok-launch` (sys module) | `CLONE_INTO_CGROUP` places the child atomically at creation; launcher supplies the target fd |
| Identity binding | Kernel (pidfd + cgroup id) | `aok-launch` registry | Identity is kernel-stamped (cgroup id = inode); the launcher only records `on_behalf_of` per root |
| FS/net narrowing | Kernel (Landlock LSM) | `aok-launch` ruleset builder | Landlock is a stackable per-thread domain; one ruleset per spawn |
| Syscall narrowing | Kernel (seccomp-bpf) | `aok-launch` filter builder | Stacked filters, lowest action wins; cannot be removed |
| Lifecycle (freeze/kill) | Kernel (cgroup v2 core) | `aok-launch` CLI | `cgroup.freeze`/`cgroup.kill`/`cgroup.events` act on the whole subtree |
| Budget (memory.max/pids.max) | Kernel (memcg/pids) | Phase 10 broker | Requires controllers delegated to the agent directory (no-internal-process rule) |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `landlock` (crate) | 0.4.7 | Build/stack Landlock rulesets, best-effort ABI downgrade, `restrict_self` + `no_new_privs` | Maintained by the Landlock LSM project (github.com/landlock-lsm/rust-landlock); supports ABI V1–V9 incl. the V6 scopes D-17 needs; 533k downloads/wk [VERIFIED: crates.io] |
| `seccompiler` (crate) | 0.5.0 | Assemble/install seccomp-bpf filters from a rule table | rust-vmm project (used by Firecrawl/Firecracker); 873k downloads/wk; installs `SECCOMP_RET_ERRNO` allowlists [VERIFIED: crates.io] |
| `libc` (crate) | pin (aok-adv uses `=0.2.191`) | clone3, pidfd ioctl, `cgroup.procs`/`cgroup.kill` raw ops | Already the crate's only dep in aok-adv; `sys.rs` primitives copy over directly |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| (none beyond the above) | — | — | The launcher is deliberately thin; std + libc + the two LSM crates cover it |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `landlock`/`seccompiler` crates | Raw syscalls hand-defined in a `sys` module (the `sandbox.c` approach, already prototyped) | D-11 anticipates copying `sys.rs`; raw syscalls keep one audited `unsafe` module but re-implement ABI-probing the `landlock` crate already does well. The crates cut the `unsafe` surface sharply and are not forbidden in `tools/`. **Recommend the crates for Landlock + seccomp; keep raw syscalls only for clone3 + the pidfd ioctl.** |
| `seccompiler` | `libseccomp` (crate, 0.3.x, libseccomp-rs) | `libseccomp` needs the C library + bindgen at build time; `seccompiler` is pure Rust and matches the "no pointer-arg decision" model (D-04) cleanly |

**Installation (if the crate path is taken):**
```bash
cargo add landlock@0.4.7 seccompiler@0.5.0
```

**Version verification:** `landlock` max_stable **0.4.7** (updated 2026-07-27), `seccompiler`
max_stable **0.5.0** (updated 2025-03-07) [VERIFIED: crates.io API this session].

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `landlock` | crates.io | since 2021-04 | 533k/wk | github.com/landlock-lsm/rust-landlock | OK | Approved |
| `seccompiler` | crates.io | since 2021-06 | 873k/wk | github.com/rust-vmm/seccompiler | OK | Approved |
| `libseccomp` | crates.io | since 2021-04 | 24k/wk | github.com/libseccomp-rs/libseccomp-rs | OK | Approved (alternative only) |

**Packages removed due to SLOP:** none. **Packages flagged SUS:** none.
All three returned `OK` from `gsd-tools query package-legitimacy check --ecosystem crates`, none
has a postinstall hook, none is deprecated.

## Architecture Patterns

### System Architecture Diagram

```
  agent principal (already in its own cgroup + Landlock domain + seccomp filter)
        |
        | exec aok-launch <sub-spec>          (D-12: sub-agent spawn is a child exec)
        v
  +-------------------------------------------------------------+
  | aok-launch (runs INSIDE the parent's domain)                |
  |                                                             |
  |  1. resolve role: read own /proc/self/cgroup + registry     |
  |       root launch  -> write registry entry (boot_id,cgid)   |
  |       sub launch   -> inherit on_behalf_of by tree position |
  |  2. mkdir <parent>/<name> and <parent>/<name>/main          |  --> cgroup v2
  |       enable +memory +pids on <parent>/<name> (D-07)        |      (kernel places,
  |  3. open main leaf O_PATH/O_DIRECTORY -> cgroup_fd          |       accounts, limits)
  |  4. build Landlock ruleset (ABI6 set, best-effort) -> layer |  --> Landlock LSM
  |  5. build seccomp allowlist (nr+arch only) -> stack         |  --> seccomp-bpf
  |  6. prctl(NO_NEW_PRIVS) [native] / CLONE_NNP [>=7.1]        |
  |  7. clone3(CLONE_INTO_CGROUP|CLONE_PIDFD, cgroup=main_fd)   |  --> child born in leaf,
  |       child: restrict_self happens pre-exec in inherited    |       domain+filter already
  |       order; execve(target)                                 |       installed & inherited
  |       parent: hold child_pidfd for exit (poll POLLIN)       |
  +-------------------------------------------------------------+
        |                                   |
        | lifecycle (LNCH-05)               | identity (LNCH-02)
        v                                   v
  cgroup.freeze / cgroup.kill          peer holds pidfd ->
  on <agent-dir> -> whole subtree      PIDFD_GET_INFO+CGROUPID -> cgroup id (==inode)
  cgroup.events: frozen/populated      -> /proc/PID/cgroup path (checked vs id)
                                       -> drop 'main' -> agent_id -> root -> on_behalf_of
```

### Recommended Project Structure
```
tools/aok-launch/            # new crate (D-11), mirrors tools/aok-adv conventions
├── Cargo.toml               # edition 2024, pin stable toolchain, libc pinned (+ landlock/seccompiler)
├── rust-toolchain.toml      # channel = "1.99.1" (same pin aok-adv uses)
├── src/
│   ├── main.rs              # aok-launch CLI (root vs sub role dispatch)
│   ├── lib.rs               # library surface Phase 10/ADV reuse
│   ├── cgroup.rs            # mkdir/main-leaf/subtree_control/freeze/kill/events
│   ├── identity.rs          # registry (boot_id,cgid) + pidfd resolution + fallback
│   ├── landlock.rs          # ABI6 ruleset builder (landlock crate) + floor check (D-14)
│   ├── seccomp.rs           # wide allowlist + narrow profiles (seccompiler), nr+arch only
│   └── sys.rs               # the ONE unsafe module: clone3_into_cgroup, pidfd ioctl, NNP
└── tests/                   # negative-control tests incl. the xattr forgery test (D-10)
```

### Pattern 1: Delegated-root evacuation before controller delegation
**What:** The delegated subtree root (the `systemd-run --scope` cgroup) holds the invoking
process. You cannot enable `+memory +pids` on a cgroup that directly contains processes — the
no-internal-process rule returns `EBUSY`. You must first move the launcher's own process into a
child leaf, *then* enable controllers on the root.
**When to use:** At root-agent setup, before building the agent tree.
**Example:** `tools/linux-baseline/cg.c:133-145` (`cg_setup`) already does exactly this — creates a
`self` leaf, writes `0` to its `cgroup.procs`, then writes `+memory +pids` to the root's
`cgroup.subtree_control`. [VERIFIED: cg.c:133-145 read this session; reproduced by probe —
`q4_move_self_out_of_root: ok` then `q4_enable_on_root_after_evac: ok`, root `subtree_control`
became `memory pids`.]

### Pattern 2: One ruleset per spawn, kernel stacks it (D-17)
**What:** Each `aok-launch` builds a single Landlock ruleset for the spawn and calls
`landlock_restrict_self` once; the kernel layers it on whatever domain the process already carries.
A sub-agent therefore adds exactly one layer. No code reads or re-applies the parent's rules.
**When to use:** Every spawn. Subset-safety (LNCH-03) is the kernel's AND-of-layers, not the
launcher's responsibility.

### Anti-Patterns to Avoid
- **Enabling controllers on a cgroup that holds processes** — `EBUSY` (`cgroup_vet_subtree_control_enable` → `cgroup_has_tasks` → `-EBUSY` [CITED: kernel/cgroup/cgroup.c v6.18]). Use the main-leaf layout.
- **`cgroup.kill` on an ancestor that contains your own launcher process** — it SIGKILLs the launcher. Place the launcher outside any cgroup it may later kill. (Observed: a probe moved itself into a leaf then killed that leaf's ancestor and died with signal 9.)
- **Trusting `user.*` xattrs on a cgroup directory for identity** — forgeable even under Landlock (D-10, verified below).
- **Expecting seccomp to see clone3/namespace flags** — flags are behind a pointer; the filter cannot read them (D-02). Namespace mediation is Phase 11.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Landlock ruleset + ABI downgrade | Hand-coded `landlock_create_ruleset`/`add_rule` with manual ABI probing | `landlock` 0.4.7 (`CompatLevel::BestEffort`, `Ruleset`, `restrict_self`) | The crate already encodes the per-ABI feature matrix and the best-effort downgrade D-17 needs; hand-rolling re-derives it and risks an off-by-one against the floor |
| seccomp allowlist assembly | Hand-written BPF `sock_filter[]` (the `sandbox.c:162-182` approach) | `seccompiler` 0.5.0 rule table → `SeccompFilter` | Pure-Rust, produces the nr+arch-only program D-04 wants, handles the ERRNO/ALLOW epilogue |
| pidfd exit wait | Busy-poll `/proc/PID` | `poll(pidfd, POLLIN)` — `sys.rs::poll_readable` exists | pidfd readability is the documented exit signal; no pid-reuse race |
| cgroup id | Parse `/proc/self/cgroup` and guess | `stat().st_ino` of the cgroup dir == cgroup id; `PIDFD_GET_INFO`+`PIDFD_INFO_CGROUPID` for a peer | VERIFIED equal this session |

**Key insight:** The containment primitives are all in the kernel; the launcher's job is
orchestration and identity bookkeeping, not policy mechanism. The two LSM crates exist precisely
because hand-rolled BPF/ABI probing is where subtle authority bugs hide.

## Runtime State Inventory

> This is a greenfield crate, not a rename/refactor. One shared-file edit matters:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — launcher creates cgroups at runtime; registry lives under `$XDG_RUNTIME_DIR` (cleared on reboot, `boot_id`-keyed) | none |
| Live service config | `systemd --user` delegated scope via `systemd-run --user -p Delegate=yes` (not persisted; recreated per session) | none |
| OS-registered state | None | none |
| Secrets/env vars | None | none |
| Build artifacts | New crate `tools/aok-launch` adds a `target/`; no existing artifact renamed | none |
| **Shared source file** | `tools/linux-baseline/run.sh` — Phase 8 plan 08-05 invokes it with the committed defaults; D-16 adds a `--wrap` hook here | **Additive only** (see Validation Architecture); flag to Phase 8 |

## Common Pitfalls

### Pitfall 1: Delegated root is an internal node
**What goes wrong:** `+memory +pids` write to the delegated root's `cgroup.subtree_control`
returns `EBUSY` (or `ENOENT` for the controller name when it isn't yet available upward).
**Why it happens:** The `systemd-run --scope` cgroup holds the launcher process; the
no-internal-process rule forbids both tasks and a populated `subtree_control` on the same cgroup.
**How to avoid:** Evacuate into a leaf first (Pattern 1 / `cg_setup`).
**Warning signs:** empty `cgroup.controllers` on the agent dir; `EBUSY` on the enable write.

### Pitfall 2: Killing the subtree you live in
**What goes wrong:** `cgroup.kill` on an ancestor cgroup that still contains the launcher kills the
launcher (SIGKILL, exit 137).
**Why it happens:** `cgroup.kill` sends SIGKILL to every task in the subtree, descendants included
(`cgroup_for_each_live_descendant_pre` [CITED: kernel/cgroup/cgroup.c v6.18]).
**How to avoid:** The launcher process must live outside any agent subtree it manages — e.g. in the
delegated root's `self`/manager leaf, never inside an agent directory.

### Pitfall 3: `CLONE_NNP` cannot be exercised on this host
**What goes wrong:** The `CLONE_NNP` success path (D-13) cannot run; only the
`prctl(PR_SET_NO_NEW_PRIVS)` path does.
**Why it happens:** `CLONE_NNP` arrives in kernel 7.1; this host is 6.18 and the CONTEXT confirms
Ubuntu HWE 7.0 lacks it too. [VERIFIED: /proc/self/status shows the filter installs NoNewPrivs via
prctl; CLONE_NNP constant is not present.]
**How to avoid:** Report the `CLONE_NNP` path as "not exercised in Phase 9"; the prctl path runs
natively and is the one tested.

## Code Examples

### clone3 + CLONE_INTO_CGROUP (reuse the existing primitive)
```rust
// Source: tools/aok-adv/src/sys.rs:854-892 (clone3_fork_into_cgroup), read this session.
// args.flags = CLONE_INTO_CGROUP | CLONE_PIDFD; args.cgroup = cgroup_fd as u64;
// VERIFIED by probe: child born directly in the target leaf (q2_child_in_target: true),
// and PIDFD_GET_INFO on the returned pidfd reported cgroupid == target dir inode.
```

### pidfd identity (fixed ioctl request number)
```
// PIDFD_GET_INFO = _IOWR(0xFF, 11, struct pidfd_info[64 bytes]) = 0xc040ff0b
// mask = PIDFD_INFO_CGROUPID (1<<2); returned cgroupid == stat(target_dir).st_ino.
// Source: tools/linux-baseline/uapi_compat.h:76-96 (struct + magic), read this session.
// VERIFIED: probe returned pidfd_cgroupid == target_dir_inode == 5335.
```

### Landlock ruleset (crate path, ABI 6 feature set, best-effort)
```rust
// Source: docs.rs/landlock/0.4.7 — Ruleset + CompatLevel::BestEffort + restrict_self.
// Handle ABI V1 fs rights + V4 net connect-tcp + V6 scopes (AbstractUnixSocket, Signal),
// set CompatLevel::BestEffort so newer-only rights downgrade silently (D-17).
// restrict_self() returns RestrictionStatus { no_new_privs: true, FullyEnforced } on success.
// The raw-syscall equivalent is tools/linux-baseline/sandbox.c:206-259 (apply_landlock).
```

### seccomp allowlist (nr + arch only, ERRNO on miss)
```c
// Source: tools/linux-baseline/sandbox.c:162-182 (build_allowlist), read this session.
// Loads seccomp_data.arch then seccomp_data.nr ONLY; match -> ALLOW, miss -> ERRNO|EPERM.
// Launcher CHANGES line 168 from SECCOMP_RET_KILL_PROCESS to SECCOMP_RET_ERRNO (D-05),
// so an arch mismatch also returns an errno, keeping LNCH-06 one invariant.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hand-written BPF `sock_filter[]` | `seccompiler` rule table | rust-vmm, stable 0.5.0 | Less unsafe, same nr+arch model |
| Manual `landlock_*` syscalls + ABI `if`-ladders | `landlock` crate best-effort | 0.4.x | Encodes the ABI feature matrix once |
| pid-based identity | pidfd + `PIDFD_GET_INFO` cgroup id | kernel 6.5+ `PIDFD_GET_INFO` | Reuse-proof, kernel-stamped |

**Deprecated/outdated:** nothing in scope; all mechanisms are current on 6.18.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | D-01's premise that "typical libc probe code handles only `ENOSYS`, treats `EPERM` as a hard error" holds for every syscall in the finalized wide allowlist | User Constraints / LNCH-06 | A libc routine that falls back on `EPERM` could mask a denial, or one that treats `ENOSYS` as fatal could break on an above-table number. The CONTEXT already marks this for verification against musl/glibc; the planner must check it against the *final* allowlist (glibc is known to fall back on `ENOSYS` for `clone3`→`clone`, `statx`, `faccessat2`, `close_range`, but this was not re-read from musl/glibc source this session). |
| A2 | `clone3 CLONE_INTO_CGROUP` into a cgroup the agent owns needs only write on the target leaf's `cgroup.procs` plus write on the common ancestor's `cgroup.procs` — satisfied automatically inside a delegated subtree | Research item 2 | If a stricter permission applied, a sub-agent spawn could fail; mitigated — the positive case is VERIFIED and the ancestor rule is read from source (below), but I did not construct a same-uid *un*-delegated counter-case |

**If the planner finalizes the allowlist and checks A1 against musl/glibc source, this table should
shrink to A1-resolved + A2 (A2 is low risk given the verified positive path).**

## Open Questions

1. **D-01 libc fallback semantics (A1 above)**
   - What we know: seccomp returns `EPERM` for a known-but-denied syscall and the kernel returns
     `ENOSYS` for a number above the filter's max (the allowlist epilogue returns `ERRNO|EPERM`;
     anything the kernel itself doesn't implement is `ENOSYS`). glibc's new-syscall probes
     generally branch on `ENOSYS`.
   - What's unclear: whether *every* syscall in the final wide allowlist, across musl and glibc,
     degrades gracefully on the errno it would receive.
   - Recommendation: the planner derives the allowlist from the syscall table, then greps musl and
     glibc for the specific fallbacks; treat as a `checkpoint` if any probe branches on a
     non-`ENOSYS` errno.

2. **Registry location/lifetime (Claude's discretion, not a blocker)**
   - Recommendation: `$XDG_RUNTIME_DIR/aok-launch/registry.<boot_id>`; one line per root agent
     keyed `(boot_id, cgroup_id)`; cleaned when the root's cgroup id no longer resolves.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cgroup v2 delegated subtree | LNCH-01/05 | ✓ | `systemd-run --user -p Delegate=yes`, systemd 255 | — |
| `CLONE_INTO_CGROUP` | LNCH-01 | ✓ | kernel 6.18.33.2 | fork→`cgroup.procs` move→exec (D-13), window reported |
| `PIDFD_GET_INFO`+`PIDFD_INFO_CGROUPID` | LNCH-02 | ✓ | ioctl `0xc040ff0b` returns cgroupid | `SO_PEERCRED`+`/proc/PID/cgroup` (D-09), racy |
| Landlock | LNCH-03 | ✓ | ABI **7** | refuse below floor (D-14) |
| seccomp filter + action cache | LNCH-03/06 | ✓ | `CONFIG_SECCOMP_FILTER=y` | — |
| `cgroup.freeze` / `cgroup.kill` | LNCH-05 | ✓ | present on delegated leaf and non-leaf | — |
| `CLONE_NNP` | LNCH-03 (success path) | ✗ | arrives 7.1 | `prctl(PR_SET_NO_NEW_PRIVS)` runs natively (D-13); CLONE_NNP path not exercisable this phase |
| memory/pids controllers | budget (Phase 10 seam) | ✓ | `+memory +pids` delegable after root evacuation | — |
| Rust stable toolchain | build | ✓ | 1.99.1 installed (default is nightly 1.100.0; crate pins stable per D-11) | — |
| `landlock` 0.4.7 / `seccompiler` 0.5.0 crates | build (if crate path) | ✓ (crates.io) | 0.4.7 / 0.5.0 | raw syscalls in `sys.rs` |

**Missing with no fallback:** none. **Missing with fallback:** `CLONE_NNP` (prctl path).
`CONFIG_SECCOMP_CACHE_DEBUG` is **not** set, so `/proc/PID/seccomp_cache` is not exposed — the
cache cannot be observed directly on this host (see item 6).

## Research Items (the nine, with evidence)

### Item 1 — Landlock ABI 6 vs 7 feature sets; the 16-layer cap and the errno
- **ABI feature map** [CITED: docs.kernel.org/userspace-api/landlock.html]: V1 base fs rights; V2
  `REFER`; V3 `TRUNCATE`; V4 TCP bind/connect; V5 `IOCTL_DEV`; **V6 scopes
  (`LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET`, `LANDLOCK_SCOPE_SIGNAL`)**; **V7 logging/audit flags for
  `landlock_restrict_self` (`LOG_SAME_EXEC_OFF`, `LOG_NEW_EXEC_ON`, `LOG_SUBDOMAINS_OFF`)**; V8
  TSYNC/all-threads; V9 `RESOLVE_UNIX`; V10 UDP. D-17's "ABI 6 feature set" = fs rights + TCP
  connect + the two scopes; the V7 audit flags are the only thing ABI 7 adds and D-17 correctly
  excludes them (opt-in profile later). Host reports **ABI 7** [VERIFIED: probe `landlock_abi: 7`].
- **16-layer cap** [VERIFIED: security/landlock/limits.h v6.18 — `#define LANDLOCK_MAX_NUM_LAYERS
  16`; security/landlock/ruleset.c v6.18 `landlock_merge_ruleset`: `if (parent->num_layers >=
  LANDLOCK_MAX_NUM_LAYERS) return ERR_PTR(-E2BIG);`]. Reproduced: **16 rulesets applied, the 17th
  `landlock_restrict_self` failed with `E2BIG`** ("Argument list too long") [VERIFIED: probe
  `layers_applied: 16, restrict_fail_at_layer: 17, restrict_errno_at_cap: "Argument list too
  long"`]. D-18 is exact — 16 and `E2BIG` both confirmed.

### Item 2 — clone3 + CLONE_INTO_CGROUP permissions in a delegated subtree
- Positive: clone3 into a leaf the agent created inside its delegated subtree **succeeds**, child
  is placed in that leaf [VERIFIED: probe `clone3_into_leaf: ok`, `q2_child_in_target: true`].
- Negative: clone3 into the parent of the delegated root (`app.slice`, an internal node) fails
  **`EBUSY`** [VERIFIED: probe `q2b_clone3_into_unowned: "Device or resource busy"`] — the
  no-internal-process constraint, not a permission error.
- Permission model [CITED: kernel/cgroup/cgroup.c v6.18]: `cgroup_css_set_fork` accepts the cgroup
  fd via `CLASS(fd_raw,...)` so an **`O_PATH`** fd is allowed ("This can even be an O_PATH file
  descriptor. But it can never be a cgroup.procs file descriptor."), calls
  `cgroup_may_write(dst_cgrp)` (→ `inode_permission(..., MAY_WRITE)` on the dst `cgroup.procs`) and
  `cgroup_attach_permissions`, which walks to the **common ancestor** (`while
  (!cgroup_is_descendant(dst_cgrp, com_cgrp)) com_cgrp = cgroup_parent(com_cgrp)`) and requires
  `cgroup_may_write(com_cgrp)` too. Inside a delegated subtree the caller owns both, so the spawn
  is permitted. So: the caller needs **write on the target leaf and on the common ancestor**, both
  satisfied by delegation (A2).

### Item 3 — Can any path remove or weaken a seccomp filter or Landlock domain?
- **No, for seccomp** [VERIFIED: probe]. After installing a filter that denies `sysinfo` with
  `EPERM`, installing a *second* allow-all filter on top left `sysinfo` **still denied** (`EPERM`),
  with `Seccomp_filters: 2` [probe `q3_seccomp`: `install_allow_all_on_top: ok`,
  `sysinfo_after_allow_all_stacked: "Operation not permitted"`]. There is no "remove filter" API;
  `seccomp_run_filters` keeps the **lowest** action value across all stacked filters
  (`ACTION_ONLY(cur_ret) < ACTION_ONLY(ret)` [CITED: kernel/seccomp.c v6.18]), so a later
  permissive filter cannot lift an earlier denial. This retires gap-analysis 1c ("cannot be
  removed", previously labelled A).
- **No, for Landlock** [CITED: docs.kernel.org/userspace-api/landlock + security/landlock/ruleset.c]:
  `landlock_restrict_self` only *adds* a layer; access is the AND across all layers; there is no
  unrestrict call. The 16-layer probe confirms layers only accumulate.

### Item 4 — No-internal-process rule; the `<root>/<sub>/main` layout for memory.max/pids.max
- Enabling `+memory` on a cgroup that **holds a process** fails [VERIFIED: probe
  `q4_proc_into_internal_agent_EBUSY: "Device or resource busy"`; `cgroup_vet_subtree_control_enable`
  → `if (cgroup_has_tasks(cgrp)) return -EBUSY` [CITED: kernel/cgroup/cgroup.c v6.18]].
- The D-07 layout works: after evacuating the root (Pattern 1), enabling `+memory +pids` on the
  **agent directory** (an internal node whose only child is `main`) succeeds, and the **sub-agent
  directory** then has `memory.max`/`pids.max`, both writable [VERIFIED: probe
  `q4_enable_on_agent: ok`, `q4_sub_has_memory_max: true`, `q4_sub_has_pids_max: true`,
  `q4_set_memory_max_on_sub: ok` → readback `104857600`, `q4_set_pids_max_on_sub: ok` → `128`].
  Putting a process in the leaf `main` succeeds [`q4_proc_into_leaf_main_ok: ok`]. **D-07's `main`
  leaf buys exactly what it claims** — this retires the "not yet re-checked on this host" note.

### Item 5 — cgroup id == directory inode; PIDFD_GET_INFO returns it
- **Yes** [VERIFIED: probe]. `stat(target_dir).st_ino == 5335` and `PIDFD_GET_INFO` with
  `PIDFD_INFO_CGROUPID` on the child's pidfd returned `cgroupid == 5335`
  [`cgroupid_eq_inode: true`]. D-09's ASSUMED premise is now verified. The resolution chain
  (pidfd → cgroup id → `/proc/PID/cgroup` path checked against the id → drop `main`) is sound; the
  `SO_PEERCRED` fallback's pid-reuse race remains real and must be reported by the fallback test.

### Item 6 — Seccomp action cache applies to the wide filter; is it exposed?
- **The cache serves the wide allowlist's *allowed* syscalls** [CITED: kernel/seccomp.c v6.18]:
  `seccomp_cache_check_allow` is called at the top of `seccomp_run_filters` and returns
  `SECCOMP_RET_ALLOW` without running any filter when the syscall's bit is set. A syscall is cached
  as "allow" only if `seccomp_is_const_allow` holds for it across every stacked filter —
  i.e. the filter loads **only** `offsetof(seccomp_data, nr)` and `offsetof(seccomp_data, arch)`
  (exactly D-04's model) and returns a constant `SECCOMP_RET_ALLOW`. Denied syscalls are marked
  non-cacheable (`__clear_bit`) and always run the filter. So the wide allowlist's hot path (the
  allowed measured syscalls) is cache-served; this is consistent with D-04.
- **Not observable on this host**: `CONFIG_SECCOMP_CACHE_DEBUG is not set` [VERIFIED:
  /proc/config.gz], so `/proc/PID/seccomp_cache` does not exist (the file is `#ifdef
  CONFIG_SECCOMP_CACHE_DEBUG` [CITED: kernel/seccomp.c]). The cache itself is unconditional; only
  the debug inspection file is gated. The planner cannot assert cache hits by reading that file
  here; the D-16 depth runs measure the filter's real cost regardless.

### Item 7 — cgroup.freeze on a delegated leaf and on a non-leaf agent directory
- Freezing the **non-leaf agent directory freezes the whole subtree** [VERIFIED: probe]. With a
  process in `agent/sub/main`, writing `1` to `agent/cgroup.freeze` gave `agent/cgroup.events:
  "populated 1\nfrozen 1"` **and** `agent/sub/main/cgroup.events: "populated 1\nfrozen 1"`. Thaw
  (`0`) cleared it: sub-leaf `frozen 0`. `cgroup.kill` on the agent directory reaped the subtree
  [`q7_kill_agentdir: ok`]. This retires the ASSUMED "`cgroup.freeze` works on a delegated leaf"
  item for LNCH-05. (`cgroup.freeze`/`cgroup.kill` exist on both leaf and non-leaf
  [`q7_has_cgroup_freeze_on_agentdir: true`, `q7_has_cgroup_kill_on_agentdir: true`].)

### Item 8 — cgroup2 directory rename (possible design finding against D-06)
- **Rename is impossible — D-06 is SAFE, this is NOT the feared finding.** `rename(2)` of a cgroup2
  directory within one parent fails **`EPERM`** [VERIFIED: probe `q8_rename_cgroup_dir: "Operation
  not permitted"`], because cgroupfs registers **no `.rename`** handler: `cgroup_kf_syscall_ops`
  has `.show_options`, `.mkdir = cgroup_mkdir`, `.rmdir = cgroup_rmdir`, `.show_path` — and **no
  `.rename`** [CITED: kernel/cgroup/cgroup.c v6.18]. kernfs rejects rename with no handler before
  Landlock is even consulted. Therefore a sub-agent's path-derived `agent_id` **cannot** be changed
  by rename, with or without a Landlock grant. The sub-question "can a grant allow mkdir/rmdir
  without rename?" is moot for cgroup2 (rename is unconditionally denied); for the record, Landlock
  does treat `MAKE_DIR`/`REMOVE_DIR` as bits distinct from `REFER` (rename/link), so even on an
  ordinary filesystem a grant can allow dir creation/removal without allowing rename. **No design
  change to D-06; no new ADV case needed for rename.**

### Item 9 — Moving a process between cgroups inside one's own subtree
- **Allowed, as expected** [VERIFIED: probe]. Writing a task's pid to a sibling leaf's
  `cgroup.procs` inside the agent's own subtree **succeeds** (`q9_move_main_to_submain: ok`;
  `q9_submain_procs` then listed the pid). This **reattributes that task's `agent_id`** — the next
  pidfd resolution reads the new cgroup id/path. This is by design (an agent governs its own
  subtree) and does not grant authority *above* the agent's own directory (writes outside the
  grant are `EACCES`, per D-06's existing verification). **Record as an ADV candidate** (an agent
  reshuffling its own sub-agents' task attribution), not a defect; the broker (Phase 10) always
  reads identity fresh from the kernel, so a moved task is attributed to wherever it currently
  sits.

### Xattr forgery probe → repo negative-control test (D-10)
- Re-ran the scratch probe (`ll_xattr_probe.py`) on this host [VERIFIED]: a same-uid process under
  a Landlock domain handling **all** fs rights with **no** rules could still
  `setxattr`(overwrite), `setxattr`(new) and `removexattr` `user.aok.agent_id` on a cgroup
  directory — all returned `ok`; the final `getxattr` returned `ENODATA`, proving the removal took.
  (`mkdir` and opening `cgroup.procs` for write were denied `EACCES`, confirming the domain was
  live.) Root cause [CITED: docs.kernel.org/userspace-api/landlock + security/landlock/fs.c
  `landlock_hooks[]`]: Landlock has **no `inode_setxattr`/`inode_removexattr` hook** (the 22 hooks
  are the `path_*`/`file_*`/`sb_*` set; no xattr hook, no `inode_permission`), and the docs state
  chmod/chown/**setxattr**/stat/utime "cannot currently be restricted". So Landlock cannot protect
  an xattr-based identity record — **D-10's rejection of `user.*` xattrs is correct.**
- **As a repo test:** add to `tools/aok-launch/tests/` a negative-control test `xattr_forgery`
  that (1) creates a throwaway cgroup leaf in a delegated subtree, (2) sets
  `user.aok.agent_id=orig`, (3) forks a child that installs a Landlock domain handling all fs
  rights with no rules + `NO_NEW_PRIVS`, (4) asserts the child **can** overwrite and remove the
  xattr (the forgery succeeds), and (5) cleans up the cgroup. **It asserts the negative control:**
  "an xattr on a cgroup dir is NOT a tamper-resistant identity store, therefore identity lives in
  the launcher-written registry keyed by kernel-stamped cgroup id (D-08), not in an xattr." If a
  future kernel/Landlock version ever *denies* the forgery, the test flips and signals that an
  xattr store has become viable — a useful tripwire. Keep it isolated (own delegated scope, full
  cgroup cleanup, no repo writes).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust, per AGENTS.md testing guidance) in the new `tools/aok-launch` crate; integration tests under `tools/aok-launch/tests/` |
| Config file | none beyond `Cargo.toml` (matches `tools/aok-adv`) |
| Quick run command | `cargo test -p aok-launch --lib` (unit: ruleset/filter builders, registry parsing — no cgroup needed) |
| Full suite command | `systemd-run --user --scope -p Delegate=yes -- cargo test -p aok-launch -- --test-threads=1` (integration tests need a delegated subtree; serialize so cgroup teardown doesn't race) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LNCH-01 | child born in its own cgroup via clone3; sub is descendant | integration | `cargo test -p aok-launch spawn_into_cgroup` | ❌ Wave 0 |
| LNCH-01 | fork-move-exec fallback + reported window | integration (via D-13 shim: `clone3`→`ENOSYS`) | `cargo test -p aok-launch spawn_fallback_window` | ❌ Wave 0 |
| LNCH-02 | peer resolves same agent_id from a pidfd | integration | `cargo test -p aok-launch identity_from_pidfd` | ❌ Wave 0 |
| LNCH-02 | SO_PEERCRED fallback reports pid-reuse race | integration (via D-13 shim: ioctl→`ENOTTY`) | `cargo test -p aok-launch identity_fallback_race` | ❌ Wave 0 |
| LNCH-03 | sub cannot open parent-lacked path; cannot drop a layer; NoNewPrivs=1 | integration | `cargo test -p aok-launch subset_landlock subset_seccomp nnp` | ❌ Wave 0 |
| LNCH-04 | `sh -c` grandchild reports agent cgroup id, refused out-of-grant path | integration | `cargo test -p aok-launch shellout_inherits` | ❌ Wave 0 |
| LNCH-05 | freeze→`frozen 1` subtree-wide; kill→all reaped; exit via pidfd | integration | `cargo test -p aok-launch freeze_thaw_kill` | ❌ Wave 0 |
| LNCH-06 | every denial returns a documented errno; filter reads no pointer arg | unit + integration | `cargo test -p aok-launch errno_contract filter_no_ptr` | ❌ Wave 0 |
| D-10 | xattr forgery negative control | integration | `cargo test -p aok-launch xattr_forgery` | ❌ Wave 0 |
| D-16 | openat depth read at 1/2/4 layers | harness | `bash tools/linux-baseline/run.sh --run-id <id> --suites micro --wrap <aok-launch> --tag lldepth-N` | ❌ Wave 0 (additive run.sh edit) |

### "Simulated" fallback tests through the external seccomp shim (D-13)
- The shim is a **test wrapper** that installs a seccomp filter faking a missing feature's errno,
  so the launcher's real detection + fallback body run with no test-only branch in the binary:
  - `PIDFD_GET_INFO` absent → filter returns **`ENOTTY`** for the `ioctl` matched on request number
    `0xc040ff0b` → exercises the `SO_PEERCRED`+`/proc/PID/cgroup` fallback (LNCH-02).
  - `clone3` absent → filter returns **`ENOSYS`** (or `EINVAL`) for `clone3`, denied whole (the
    shim cannot read flags) → exercises the fork→`cgroup.procs` move→exec fallback (LNCH-01), which
    reports its spawn-time window.
- Every such result is labelled **"simulated on a kernel that has the feature"**. The `CLONE_NNP`
  success path is **not** simulable this way (it is a kernel 7.1 capability, not a syscall the shim
  can fake) — report it as "not exercised in Phase 9; prctl path tested natively".

### Sampling Rate
- **Per task commit:** `cargo test -p aok-launch --lib` (builders + parsers, fast, no cgroup).
- **Per wave merge:** full integration suite inside one delegated scope, `--test-threads=1`.
- **Phase gate:** full suite green before `/gsd-verify-work`; D-16 depth runs committed under
  `tools/linux-baseline/results/<run-id>/` with `env.txt`.

### Wave 0 Gaps
- [ ] `tools/aok-launch/Cargo.toml` + `rust-toolchain.toml` (copy aok-adv pins; add
      `landlock`/`seccompiler` if crate path chosen)
- [ ] `tools/aok-launch/tests/common/` — a delegated-scope + cgroup-cleanup fixture (every
      integration test creates its own leaf and tears it down; never kill a cgroup containing the
      test process)
- [ ] the external seccomp shim test wrapper (D-13)
- [ ] additive `--wrap`/`--tag` hook in `tools/linux-baseline/run.sh` for the D-16 depth reads
      (flagged to Phase 8 plan 08-05 — same file)

## Security Domain

`security_enforcement` is enabled (`security_asvs_level: 1`). This phase *is* a security control
(the agent containment boundary), so the relevant ASVS surface is the control's own correctness.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V1 Architecture / trust boundaries | yes | Containment boundary = cgroup + Landlock domain + seccomp filter per agent; identity kernel-stamped (cgroup id), never message-supplied |
| V4 Access Control | yes | Subset-only authority enforced by the kernel's layer-AND (Landlock) and lowest-action-wins (seccomp); verified unremovable |
| V5 Input Validation | yes | Registry parsing must reject malformed `(boot_id, cgroup_id)` lines; the agent-supplied sub-spec (name) must be validated before `mkdir` (no `..`, no `/`, no overlong path per `cg.c`'s `ENAMETOOLONG` guard) |
| V6 Cryptography | no | No secrets in Phase 9; the broker (Phase 10) holds credentials |
| V7 Error Handling | yes | Every denial returns a documented errno (LNCH-06); below-floor refusal is explicit (D-14) |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Identity spoofing via message field | Spoofing | Kernel-stamped cgroup id from pidfd; never trust a claimed id (this phase's whole point; ADV-01/BRK-01 probe it) |
| Identity record tampering via xattr | Tampering | `user.*` xattrs rejected (D-10); registry not under any agent's Landlock grant (D-08) |
| Task reattribution inside own subtree | Tampering/Repudiation | Allowed by design (item 9); broker reads identity fresh; candidate ADV case |
| Namespace escape via clone3 flags | Elevation | Out of seccomp's view (D-02); deferred to Phase 11 BPF-LSM `userns_create`; ADV-04 |
| Authority widening via the `aok-launch` grant | Elevation | The exec+write-own-dir grant (D-12) is the authority ADV probes; sub-agent adds one layer, never removes one (verified) |
| Killing the manager by targeting its ancestor | Denial of Service | Launcher lives outside any agent subtree it can kill (Pitfall 2) |

## Sources

### Primary (HIGH confidence)
- Linux v6.18 kernel source (read this session): `security/landlock/limits.h` (16-layer cap),
  `security/landlock/ruleset.c` (`E2BIG`), `security/landlock/fs.c` (`landlock_hooks[]`, no xattr
  hook), `kernel/seccomp.c` (cache, const-allow, lowest-action-wins, ERRNO clamp,
  `CONFIG_SECCOMP_CACHE_DEBUG` gate), `kernel/cgroup/cgroup.c` (`cgroup_kf_syscall_ops` no
  `.rename`, `cgroup_procs_write_permission` common-ancestor, `cgroup_css_set_fork` O_PATH,
  `cgroup_vet_subtree_control_enable` `EBUSY`, `cgroup_kill` descendant walk).
- Host probes this session (WSL2 6.18.33.2, unprivileged, inside `systemd-run --user -p
  Delegate=yes`): all nine items + the xattr forgery; scripts in the session scratchpad.
- In-repo (read this session): `tools/aok-adv/src/sys.rs`, `tools/linux-baseline/cg.c`,
  `tools/linux-baseline/sandbox.c`, `tools/linux-baseline/uapi_compat.h`,
  `tools/linux-baseline/run.sh`, `tools/linux-baseline/summarize.py`.
- crates.io API (this session): `landlock` 0.4.7, `seccompiler` 0.5.0 — version, repo, downloads.

### Secondary (MEDIUM confidence)
- docs.kernel.org/userspace-api/landlock.html — ABI version feature map, 16-layer E2BIG statement,
  xattr/chmod/chown non-restrictable list, O_PATH guidance.
- docs.rs/landlock/0.4.7 — ABI V1–V9, best-effort CompatLevel, `restrict_self` `no_new_privs`.

### Tertiary (LOW confidence)
- D-01's "typical libc probe handles only ENOSYS" — training knowledge of glibc fallback patterns;
  not re-read from musl/glibc source this session (A1).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — crates verified on crates.io; raw-syscall fallback already in-repo.
- Architecture / 9 research items: HIGH — 8 of 9 VERIFIED by probe or v6.18 source this session.
- Pitfalls: HIGH — each pitfall was triggered or read from source.
- D-01 libc fallback: LOW — single ASSUMED item (A1), flagged for the planner.

**Research date:** 2026-11-05
**Valid until:** ~2026-12-05 for the crate versions; the kernel-source facts are pinned to v6.18
and stable for this host's lifetime.
