# Phase 9: Agent launcher and containment - Context

**Gathered:** 2026-11-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Build an unprivileged launcher that spawns each agent into its own cgroup v2 cgroup with
`clone3` + `CLONE_INTO_CGROUP`, binds a pidfd-resolvable identity to that cgroup, layers Landlock
and seccomp narrowing per spawn, makes shelled-out children inherit all of it, and freezes, thaws
and kills a whole agent subtree. Requirements: LNCH-01 to LNCH-06. The boundary is fixed by
`.planning/ROADMAP.md`; discussion settled how, not whether.

Every LNCH requirement runs unprivileged on the current host (WSL2, kernel 6.18.33.2, Landlock ABI
7, a delegated subtree through `systemd-run --user -p Delegate=yes`), so the phase needs no
ENV-01. Out of scope: the broker (Phase 10), BPF-LSM mediation and the adversarial run (Phase 11),
stock-kernel boots, the S9 full-grant profile and POSX-02 (Phase 12), the S3 verdict (Phase 8 gate
and the reference host).

DEC-02 and DEC-03 are unmarked in `08-GATE.md`, so every number and floor here is "proposed,
awaiting user acceptance". Phase 9 carries all three fallbacks that LNCH-01, LNCH-02 and LNCH-03
name, whatever DEC-03 later marks; a dropped tier removes a test, not code.

</domain>

<decisions>
## Implementation Decisions

### Seccomp filter (LNCH-06)

- **D-01:** A denied syscall that the filter's table knows returns `EPERM`. A syscall number above
  the table's highest returns `ENOSYS`, so a newer libc probing a new syscall falls back as it
  would on an older kernel. *(interpretation)* The claim that typical probe code handles only
  `ENOSYS` is from memory; the researcher verifies it against musl and glibc.
- **D-02:** `clone3` is **allowed**. Its flags sit in a struct behind a pointer, which LNCH-06 and
  ADR 0004 decision 4 bar the filter from reading, so namespace flags such as `CLONE_NEWUSER` go
  unseen by seccomp. The gap is recorded here, an ADV-04 case probes it, and Phase 11 closes it
  with the BPF-LSM `userns_create` hook. This also lets an agent run `aok-launch` for a sub-agent
  with `CLONE_INTO_CGROUP` (D-11). *(interpretation)* The Ubuntu AppArmor userns restriction
  (08-CONTEXT D-02) narrows the gap on the reference host only, not on Debian or Fedora.
- **D-03:** The default allowlist is **wide, with narrow profiles opt-in** per launch. It covers
  what glibc, musl, BusyBox `ash` and os-test use, and denies only: `bpf`, the mount family,
  `ptrace`, `perf_event_open`, `kexec_*`, module loading, `userfaultfd`, `io_uring_*`. The planner
  finalises the list from the syscall table. A narrow profile must still include the launcher's
  own syscalls, or an agent under it cannot spawn a sub-agent. That is a usable property (a leaf
  agent), not a defect.
- **D-04:** The filter decides on **syscall number and architecture only**; it inspects no
  pointer argument. That is what lets the kernel's seccomp action cache serve it. The cache is
  ASSUMED (linux-baseline.md section 9) and has never been read; the researcher checks whether this
  kernel exposes it.
- **D-05:** An architecture mismatch (the 32-bit compat entry, an x32 number) returns `ERRNO`, the
  same as any denial, so LNCH-06 stays one invariant: every denial returns an errno.
  `tools/linux-baseline/sandbox.c` line 168 uses `SECCOMP_RET_KILL_PROCESS` there; the launcher
  differs from the baseline on purpose.
- **Consequence to carry:** C1b's 4.03% (openat_tmpfs) and 3.31% (openat_rootfs) were measured with
  sandbox.c's 64-entry filter, not this wide one. They do not transfer to the launcher's filter.
  The D-16 depth runs wrap the real launcher, so they measure the real filter.

### Identity and registry (LNCH-01, LNCH-02)

- **D-06:** `agent_id` is the **cgroup path from the root agent**, for example `r7/s1/s2`, set by
  `mkdir`. `parent` is the parent directory. `on_behalf_of` lives in a **launcher-written
  registry**, one entry per root agent. VERIFIED on this host (6.18.33.2, Landlock ABI 7): a
  Landlocked process gets `EACCES` on `mkdir` and on `cgroup.procs` writes outside its grant, so
  only granted subtrees can be named. — **Reversibility:** costly — the Phase 10 broker and the
  Phase 11 BPF-LSM policy key on this identity shape.
- **D-07:** Each agent is a directory plus a **`main` leaf** that holds its processes:
  `<root>/<sub>/<sub>/main`. The agent directory holds no processes, so it can enable controllers
  such as `+memory +pids` for its sub-agents (the no-internal-process rule), which keeps Phase 10's
  open "carve a sub-agent budget from its parent" decision possible. `cgroup.freeze` and
  `cgroup.kill` on the agent directory reach the whole subtree (LNCH-05). — **Reversibility:**
  costly — ADV cases, Phase 10 budgets and every path in the registry are written against this
  shape. The no-internal-process rule is from the kernel docs and is not yet re-checked on this
  host (see Research).
- **D-08:** `on_behalf_of` is **fixed at root launch and inherited by tree position**. It names the
  human principal (option-b-microkernel.md line 46). No sub-agent needs a registry write, so no
  registrar service is built. A tree that must act for two humans is two root launches. The
  registry is not readable or writable under an agent's Landlock grant, and is keyed by
  `(boot_id, cgroup_id)` of the root agent's directory.
- **D-09:** Resolution is: pidfd, then `PIDFD_GET_INFO` with `PIDFD_INFO_CGROUPID` for the cgroup
  id, then `/proc/PID/cgroup` for the path, checked against the id, then drop the `main`
  component, then the nearest registered root ancestor supplies `on_behalf_of`. Fallback where
  `PIDFD_GET_INFO` is absent: `SO_PEERCRED` plus `/proc/PID/cgroup`, which is racy against pid
  reuse; the test reports the race it cannot close. That a cgroup id equals the directory's inode
  number is ASSUMED, not checked.
- **D-10:** `user.*` xattrs on the cgroup directory are **rejected**. VERIFIED on this host: a
  Landlocked same-uid process (all fs rights handled, no rules) overwrote and removed
  `user.aok.agent_id` on a cgroup directory, so an agent could rewrite its own or another agent's
  record. The probe script was scratch (not in the repo). The plan must re-create it as a
  negative-control test so the evidence lives in the repo.

### Launcher shape and fallbacks (LNCH-01 to LNCH-05)

- **D-11:** The launcher is **Rust in a new crate under `tools/`**: a library plus one CLI,
  `aok-launch`. It follows `tools/aok-adv`: edition 2024, stable toolchain pinned, `libc` pinned,
  all `unsafe` in one `sys` module with `// SAFETY:` comments, outside `tools/unsafe_check.py` and
  the unsafe ledger. The user took this option with that cost stated. *(interpretation)* Unlike
  aok-adv, the launcher runs in the agent's domain and sets up containment, so D-13's "test
  tooling, not the TCB" rationale from Phase 8 does not carry over. If the ledger question
  matters later, it reopens here, not in the crate's conventions. `tools/aok-adv/src/sys.rs`
  already has `clone3_fork_into_cgroup`, `pidfd_open`, `pidfd_send_signal`, `send_fd` and
  `recv_fd` to copy from. `cg.c` and `sandbox.c` are rewritten, not wrapped.
- **D-12:** **One CLI serves root and sub launches.** An agent that spawns a sub-agent execs
  `aok-launch` as its own child, so the spawn happens inside the parent's cgroup, Landlock domain
  and seccomp filter, and layering and stacking come from the kernel (LNCH-03). The CLI tells a
  root launch from a sub launch by its own cgroup path and the registry. No daemon. The agent's
  ruleset must grant exec of the binary and write access to its own agent directory; that grant is
  an authority the ADV cases probe.
- **D-13:** The fallback paths are tested through an **external seccomp shim**: a test wrapper
  installs a filter that fakes the missing feature with the errno an older kernel returns
  (`ENOTTY` for the `PIDFD_GET_INFO` ioctl, matched on its request number; `ENOSYS` or `EINVAL`
  for `clone3`, denied whole because the shim cannot see flags). The launcher sees a real errno,
  so detection and the fallback body both run, and no test-only branch ships in the binary. Every
  such result is labelled "simulated on a kernel that has the feature". This host lacks
  `CLONE_NNP` (arrives in 7.1; Ubuntu HWE 7.0 lacks it too), so the `prctl(PR_SET_NO_NEW_PRIVS)`
  path runs natively here, and the `CLONE_NNP` success path cannot be exercised in this phase.
  The report says so.
- **D-14:** Below the Landlock floor the launcher **refuses to launch**: non-zero exit, a named
  errno, a message naming the missing right. The floor (proposed ABI 6, DEC-03c) is **one
  constant read from config**, so the user's DEC-03 mark changes a value, not code. An
  `--allow-degraded` flag is not built.

### Landlock layering against the S3 bound (LNCH-03)

- **D-15:** Phase 9 **builds the Landlock fs layering LNCH-03 requires** and keeps the layer in.
  D-18 (Phase 8) keeps the S3 bound at p50 at most 10% and p99 at most 25%, both proposed. Landlock
  fs alone (C2a) adds 21.04% p50 to `openat_tmpfs` and 14.95% to `openat_rootfs` on the WSL2
  baseline, so the layer is expected to miss the bound. This phase gives **no S3 verdict**: D-18
  reads trigger (b) on the reference host, after a profile attributes the excess to the mediation
  path. Whether file narrowing moves to BPF-LSM stays an open item for the gate and Phase 11, and
  reopens ADR 0004 decision 3 if it does.
- **D-16:** An **indicative openat read at layer depth 1, 2 and 4** is taken on this host,
  unprivileged, with the real launcher. The mechanism is an additive `--wrap` hook in
  `tools/linux-baseline/run.sh` (or an environment variable) that starts only the timed `micro`
  process through `aok-launch` at depth N, labelled with `--tag` (the 08 D-07 pattern). `micro.c`
  and `sandbox.c` are unchanged, the C0 rows in the same run are the baseline, and `summarize.py`
  and `check_report.py` apply as they are. Plan 08-05 also uses `run.sh`, so the edit stays
  additive and is flagged to Phase 8. Results are reported "indicative, WSL2". That depth raises
  the cost is a hypothesis, not a measurement.
- **D-17:** The launcher applies the **Landlock ABI 6 feature set on every host**: fs rights, TCP
  connect rules, and the abstract-unix-socket and signal scopes. Rights that only newer kernels
  have are not used, so tests and the depth read taken on this ABI 7 host describe what the ABI 6
  floor distros run. Newer rights (the ABI 7 audit flags, ABI 10 UDP) arrive later as a named
  opt-in profile. **One ruleset per spawn**, so a sub-agent adds one layer.
- **D-18:** The nesting depth cap is the kernel's stacked-ruleset limit, **16 layers** (from
  memory of the Landlock docs; the researcher verifies the number and the errno). Exceeding it is
  reported with the kernel's `E2BIG` as the documented errno. The cap is revisited with the D-16
  data.

### Claude's Discretion

- Names and locations: the crate directory, the registry file, the `--wrap` flag name, the config
  file that holds the Landlock floor constant. *(interpretation)* The registry likely belongs under
  `$XDG_RUNTIME_DIR`, which a reboot clears; the `boot_id` in its key is the second guard.
- Registry binding to a boot, lifetime across reboots, and cleanup of entries for dead roots.
- The exit contract the launcher reports. *(interpretation)* Default: the leader's pidfd readable
  means "the leader exited"; `populated 0` in `cgroup.events` means "the subtree is drained".
- The exact wide-allowlist and deny set (D-03), and the narrow-profile format.
- The documented errno for each denial class that D-14 and D-18 do not already fix, in the
  LNCH-06 test set.
- Whether the launcher shares `tools/aok-adv/src/sys.rs` as a path dependency, copies it, or
  extracts a shared crate. aok-adv is pinned test tooling, so a dependency in that direction needs
  a look.
- How the depth runs report the eligibility of a row. *(interpretation)* Reuse the R1 rule of
  linux-baseline.md section 11 (stable row, spreads under 10%), as 08 D-18 does for the reproduction
  claim.
- The agent's default grant. The S9 "full grant" profile belongs to Phase 12; the grant here only
  needs to run the LNCH tests and `sh -c`.

### Layer elision

No-op layers (a sub-agent whose grant equals its parent's) could be elided to save depth and
cost. Deferred: it complicates the subset argument (LNCH-03) and the data does not yet say it is
needed.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and requirements
- `.planning/ROADMAP.md` - Phase 9 section (lines 251-269): goal, five success criteria, the two
  "wrong thing to do" falsifiers, research items, decisions due. Structure rationale at lines 53-115
- `.planning/REQUIREMENTS.md` - LNCH-01 to LNCH-06 (lines 98-108)
- `.planning/phases/08-reference-environment-early-falsifiers-and-threshold-gate/08-CONTEXT.md` -
  D-07 (`--tag` pattern), D-13/D-14 (Rust crate conventions), D-18 (S3 bound kept), D-19, D-21
- `.planning/phases/08-reference-environment-early-falsifiers-and-threshold-gate/08-GATE.md` -
  DEC-02 (line 130) and DEC-03a-f (lines 143-182), unmarked

### Architecture decisions and criteria
- `docs/architecture/adr/0004-linux-prototype-first.md` - decisions 1 to 5 and fork triggers (a)
  to (d); decision 3 reopens if Landlock fs cannot meet S3
- `docs/architecture/scoping-linux.md` - section 3 (floor matrix, tiers, reachability rule), section
  4 (S1 to S9; S3 and S9 bear on this phase)
- `docs/architecture/option-b-microkernel.md` - line 46 defines `on_behalf_of` as the human principal
  (the B3-era document; the definition is reused, the microkernel is not)
- `docs/architecture/research/linux-baseline.md` - section 4 (C1b and C2a numbers), section 7
  (cgroup probe), section 9 (VERIFIED and ASSUMED list: `cgroup.freeze` on a delegated leaf, the
  seccomp action cache), section 11 (R1 eligibility rule)

### Inputs
- `.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md` -
  1a, 1c ("cannot be removed" is labelled A, not verified), 2f, 2i
- `.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md` -
  per-distro Landlock ABI and feature rows

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `tools/aok-adv/src/sys.rs`: `clone3_fork_into_cgroup`, `pidfd_open`, `pidfd_send_signal`,
  `send_fd`, `recv_fd`; the only `unsafe` module in that crate, with `// SAFETY:` comments.
- `tools/aok-adv/Cargo.toml`, `rust-toolchain.toml`: the pinned conventions to copy.
- `tools/linux-baseline/cg.c`, `cg.h`: `cg_clone3_into` and the cgroup helpers, as a reference
  for the clone3 argument layout.
- `tools/linux-baseline/sandbox.c`, `sandbox.h`, `uapi_compat.h`: the Landlock ruleset builder and
  the seccomp program builder, as a reference. Line 168 (`KILL_PROCESS` on arch mismatch) and line
  179 (`ERRNO|EPERM` on allowlist miss) are the two behaviours D-05 and D-01 change.
- `tools/linux-baseline/run.sh`: the `--tag` mechanism and the `kv` env writer; the `--wrap` hook
  goes here (D-16). `summarize.py`, `check_report.py`, `r1.py`: reusable for the depth reads.
- `tools/bpflsm/`: Phase 8's BPF-LSM wrapper; not needed here, but the Phase 11 hand-off.

### Established Patterns
- Test tooling lives under `tools/<name>/`, one directory per tool, each with its own toolchain
  pin. The Rust crates keep `unsafe` in one `sys` module.
- Results go under `tools/linux-baseline/results/<run-id>/` with an `env.txt` per run.
- Every claim is tagged VERIFIED or ASSUMED; interpretation is marked.

### Integration Points
- The registry and the `agent_id` path are what the Phase 10 broker reads, through a pidfd.
- The agent cgroups are what Phase 11's BPF-LSM policy attaches to (`lsm_cgroup`).
- ADV-01 (Phase 8) supplies the shell-out cases for LNCH-04; ADV-04 (Phase 11) takes the D-02
  `clone3` flags case.
- `run.sh` is shared with Phase 8 plan 08-05.

</code_context>

<specifics>
## Specific Ideas

**Research items for the researcher** (the verified answer goes into RESEARCH.md; none is assumed
settled):
1. Landlock ABI 6 against ABI 7 layering behaviour; the 16-layer cap and the errno at the cap.
2. `clone3` with `CLONE_INTO_CGROUP` from an unprivileged delegated subtree: which permissions on
   the target cgroup and the common ancestor it needs.
3. Whether any path removes an installed seccomp filter (gap analysis 1c, labelled A, not verified).
4. The no-internal-process rule on this host, with and without controllers in `subtree_control`.
5. Whether a cgroup id equals the directory inode number.
6. Whether the seccomp action cache applies to the wide filter, and whether this kernel exposes it.
7. `cgroup.freeze` on a delegated leaf (retires the ASSUMED item, LNCH-05).
8. **Directory rename.** *(interpretation, unverified)* If `rename(2)` of a cgroup2 directory
   within one parent is allowed, and the agent's Landlock grant lets it rename inside its own
   subtree, then a sub-agent's path-derived `agent_id` can change while its cgroup id stays the
   same. The root and `on_behalf_of` are unaffected (the agent cannot write above its own
   directory). Check the rename semantics, and whether a grant can allow `mkdir` and `rmdir`
   without rename. If it cannot, this is a design finding against D-06, and a candidate ADV case.
9. Whether an agent can move a process between cgroups inside its own subtree (writes to
   `cgroup.procs`) and so change which sub-agent a task is attributed to. *(interpretation)*
   Expected to be allowed by design; record it and decide whether an ADV case covers it.

**What would show this phase worked:** the five ROADMAP success criteria, each with its test
output in the phase directory; the fallback paths report their window or race; the depth reads at
1, 2 and 4 layers exist as tagged `run.sh` runs.

**What would show a decision here was wrong:**
- D-01/D-03: an os-test run (Phase 12) differs between a non-agent run and an agent run with a full
  grant, and the filter is the cause.
- D-06: a case where the path and the cgroup id disagree, or a rename changes a sub-agent's id
  (Research item 8).
- D-07: the no-internal-process rule does not bite on this host with the layout in use, so the
  `main` leaf buys nothing.
- D-12: an agent that holds a grant for `aok-launch` can widen its authority through it.
- D-15: the depth reads show the Landlock layer alone is the largest share of the S3 miss. That is
  the ROADMAP's "wrong thing to do" entry, and it goes to the gate, not into this phase.

</specifics>

<deferred>
## Deferred Ideas

- Clone3 namespace-flag mediation: Phase 11 (BPF-LSM `userns_create`), with an ADV-04 case.
- The S9 full-grant profile and the per-distro matrix: Phase 12.
- Newer Landlock rights (ABI 7 audit flags, ABI 10 UDP) as a named opt-in profile: after the
  ABI 6 base is measured.
- Layer elision for no-op layers.
- An `--allow-degraded` launch mode: not built; revisit only if DEC-03 marks a tier degradable.
- Moving file narrowing to BPF-LSM: a gate and Phase 11 decision, only if the reference-host
  reading fires trigger (b).

### Reviewed Todos (not folded)
None.

</deferred>

---

*Phase: 9-Agent launcher and containment*
*Context gathered: 2026-11-05*
