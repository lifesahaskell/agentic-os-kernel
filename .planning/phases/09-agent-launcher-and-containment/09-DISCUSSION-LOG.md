# Phase 9: Agent launcher and containment - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-05
**Phase:** 9-Agent launcher and containment
**Areas discussed:** Seccomp default action, Identity record home, Launcher shape and fallbacks, Landlock fs vs S3 bound

---

## Seccomp default action

| Option | Description | Selected |
|--------|-------------|----------|
| EPERM known, ENOSYS unknown | Denied syscalls the filter knows return EPERM; numbers above the table's highest return ENOSYS so a newer libc probing a new syscall falls back. | ✓ |
| EPERM for everything off-list | Simplest; what sandbox.c C1b/C1c does, so baseline S3 numbers carry over. Breaks callers that only fall back on ENOSYS. | |
| KILL_PROCESS | Fail-closed, but kills a program that merely probes and contradicts LNCH-06. Needs an LNCH-06 amendment. | |

**User's choice:** EPERM known, ENOSYS unknown (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Allow clone3 | Agent can spawn sub-agents with CLONE_INTO_CGROUP; namespace flags go unseen by seccomp; gap recorded, ADV-04 case, Phase 11 closes via BPF-LSM userns_create. | ✓ |
| Deny clone3 with ENOSYS | libc falls back to clone() with visible flags, but a sub-agent spawn loses CLONE_INTO_CGROUP or moves outside the parent's domain. | |

**User's choice:** Allow clone3 (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Wide default, narrow opt-in | Covers glibc, musl, BusyBox ash and os-test; denies a short set. Narrow profile per launch. Filter decides on syscall number only. | ✓ |
| Narrow, in the style of sandbox.c | 64-entry list; stronger, but S9 unlikely to reach 0 with a full grant. | |
| Wide only, no profiles | One list; smallest surface, one S3 number. | |

**User's choice:** Wide default, narrow opt-in (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| ERRNO, same as any denial | LNCH-06 stays one invariant. sandbox.c line 168 KILL_PROCESS would change. | ✓ |
| KILL_PROCESS, one documented exception | Usual hardening; LNCH-06 test set lists a single exception. | |

**User's choice:** ERRNO, same as any denial (Recommended)
**Notes:** Area closed with "Next area".

---

## Identity record home

| Option | Description | Selected |
|--------|-------------|----------|
| Cgroup path plus a launcher registry | agent_id is the directory name, parent the parent directory; on_behalf_of in a launcher-written registry keyed by cgroup id. | ✓ |
| Registry only, keyed by cgroup id | Launcher writes all three fields; sub-agent spawns need a registrar service. | |
| user.* xattrs on the cgroup directory | No separate store; VERIFIED forgeable by a Landlocked same-uid process. | |

**User's choice:** Cgroup path plus a launcher registry (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Agent dir plus a `main` leaf | Agent directory holds no processes, so it can enable controllers for sub-agents. | ✓ |
| Agent dir holds its processes directly | No leaf to strip, but a sub-agent gets no memory.max or pids.max of its own. | |

**User's choice:** Agent dir plus a `main` leaf (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed at root launch, inherited | One registry entry per root; no registrar service; two humans means two root launches. | ✓ |
| Parent names it at each spawn | Needs a trusted writer outside the agents' domain for every spawn. | |

**User's choice:** Fixed at root launch, inherited (Recommended)
**Notes:** Area closed with "Next area". The xattr option was killed by a probe on this host: a Landlocked same-uid process overwrote and removed the xattr.

---

## Launcher shape and fallbacks

| Option | Description | Selected |
|--------|-------------|----------|
| Rust, new crate under tools/ | aok-adv conventions; library plus one CLI; outside unsafe_check.py and the ledger. | ✓ |
| C, grown from cg.c and sandbox.c | Fastest start; but measurement code with fixed profiles and security-relevant parsing in C. | |

**User's choice:** Rust, new crate under tools/ (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| One CLI for root and sub launches | aok-launch is exec'd by the agent as its own child, inheriting its domain. Language-agnostic. | ✓ |
| Library only, linked by the agent | No binary to grant, but ties the agent harness to Rust or FFI and `sh -c` spawns bypass it. | |

**User's choice:** One CLI for root and sub launches (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| External seccomp shim | Test wrapper fakes the missing feature with an older kernel's errno; no test-only branch ships. Results labelled simulated. | ✓ |
| Launcher flag `--force-fallback` | Skips the probe, so detection stays untested, and ships a weakening flag. | |
| Natural kernels only | Fallback tests stay pending until a host lacking the feature exists. | |

**User's choice:** External seccomp shim (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Refuse to launch, floor is a setting | Non-zero exit with a named errno; the floor is one config constant. | ✓ |
| Launch degraded, record it in the registry | Runs on more kernels, but the first line of defence silently narrows. | |

**User's choice:** Refuse to launch, floor is a setting (Recommended)
**Notes:** Area closed with "Next area".

---

## Landlock fs vs S3 bound

| Option | Description | Selected |
|--------|-------------|----------|
| Build to spec, measure depth, record | Build the Landlock fs layering; indicative openat read at depth 1, 2, 4; no S3 verdict. | ✓ |
| Treat the bound as a design goal now | Sweep ruleset size and depth to pick the cheapest shape; chases a number not readable here. | |
| Defer fs narrowing to BPF-LSM | Changes LNCH-03 and success criterion 2; untestable before the reference host. | |

**User's choice:** Build to spec, measure depth, record (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Add a wrap hook to run.sh, tagged runs | Additive `--wrap` starts only the timed micro process through aok-launch at depth N; C0 rows are the baseline. | ✓ |
| Launcher bench subcommand, own runner | No run.sh edit, but loses interleaving, pinning and the spread checks. | |

**User's choice:** Add a wrap hook to run.sh, tagged runs (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| ABI 6 feature set on every host | One ruleset shape everywhere; newer rights arrive later as a named opt-in profile. One ruleset per spawn. | ✓ |
| Best effort up to the host's ABI | Tighter on newer hosts, but results differ by host and do not transfer. | |

**User's choice:** ABI 6 feature set on every host (Recommended)
**Notes:** Area closed with "Next area". Final check: "I'm ready for context".

---

## Claude's Discretion

- Names and locations of the crate, registry file, `--wrap` flag and floor config.
- Registry boot binding, lifetime and cleanup.
- Exit contract (leader pidfd versus `populated 0`).
- Exact wide-allowlist and deny set; narrow-profile format.
- Documented errno per remaining denial class.
- Whether the launcher shares `sys.rs` from aok-adv.
- Row-eligibility rule for the depth reads.
- The agent's default grant (S9 full grant is Phase 12).

## Deferred Ideas

- Clone3 namespace-flag mediation (Phase 11).
- S9 full-grant profile and distro matrix (Phase 12).
- Newer Landlock rights as an opt-in profile.
- Layer elision for no-op layers.
- `--allow-degraded` launch mode.
