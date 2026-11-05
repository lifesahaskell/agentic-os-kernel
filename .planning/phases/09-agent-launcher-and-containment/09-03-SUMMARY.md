---
phase: 09-agent-launcher-and-containment
plan: 03
subsystem: agent-launcher
tags: [rust, landlock, seccomp, bpf, syscall-allowlist, registry-confinement, errno-contract]
status: complete
requires:
  - phase: 09-01
    provides: aok-launch spine, layer seams (landlock::apply/preflight, seccomp::apply), aok-shim fakes, common fixture
  - phase: 09-02
    provides: the D-08 registry that registry_unreachable_from_agent probes
provides:
  - landlock.rs - D-17 ABI-6 ruleset, grant syntax, default and D-12 implicit grants, D-14 floor from the embedded aok-launch.conf
  - seccomp.rs - hand-assembled x86-64 allow-list filter, v6.18 syscall table, allowlist record and A1 result in the module doc
  - aok-launch.conf - the D-14 floor (landlock_abi_floor = 6, DEC-03c proposal)
  - tests - subset, registry_confinement, errno_contract, shellout; the whole crate green under default-deny
affects: [09-04, 09-05, 09-06]
plan_head_before: 6d34a4affa790889fa6408f3bd77f829c6a54639
plan_head_after: b7e2030f2daea97c9f67308d66e0a70378e1b182
actuals:
  tokens: 19080
  tasks: 3
  commits: 3
tech-stack:
  added: []
  patterns:
    - the floor is read from a file embedded with include_str, so the running launcher opens no file an agent could rewrite
    - the seccomp program is assembled by hand (arch, x32 bit, ascending allow ranges, deny compares, ENOSYS fall-through) and checked by a small in-test BPF interpreter against the table for every number 0..0x1000
    - a stacked-filter test re-runs its own test binary as a child stage so the filters confine the child, not the test process
    - every kernel-facing probe in tests is a python3 -I -B ctypes script or a C probe, so the tests contain no unsafe
key-files:
  created:
    - tools/aok-launch/aok-launch.conf
    - tools/aok-launch/tests/subset.rs
    - tools/aok-launch/tests/registry_confinement.rs
    - tools/aok-launch/tests/errno_contract.rs
    - tools/aok-launch/tests/shellout.rs
  modified:
    - tools/aok-launch/src/landlock.rs
    - tools/aok-launch/src/seccomp.rs
    - tools/aok-launch/tests/common/mod.rs
key-decisions:
  - "Default grant (Claude's discretion under D-12): rx on /usr /bin /lib /lib64 where present, read-only on /etc and /proc, rw on /dev/null. Never /tmp, $HOME or anything under /run."
  - "D-04 is ASSUMED: the filter reads only arch and nr, and that the kernel then serves it from its seccomp action cache is not measured."
  - "statmount (457) and listmount (458) stay allowed: they read the mount table visible to the caller's own namespace; the topology-changing calls are denied."
  - "A mount-family and io_uring denial is EPERM, a table gap or above-table number is ENOSYS, an arch or x32 mismatch is EPERM (never a kill)."
requirements-completed: [LNCH-02, LNCH-03, LNCH-04, LNCH-06]
coverage:
  - id: C1
    description: A no-grant agent is refused a /tmp file the test can read (EACCES); NoNewPrivs is 1 in the agent
    verification: tests/subset.rs::zero_grant_denies, nnp
    status: verified
  - id: C2
    description: A sub-agent granted A+B under a parent granted A is still denied B (layer AND)
    verification: tests/subset.rs::subset_landlock
    status: verified
  - id: C3
    description: The D-08 registry is EACCES on read, append, create and the /proc/self/root route from a default-grant agent, no registry fd is inherited, the outside control succeeds at all four, the registry keeps the launch's line
    verification: tests/registry_confinement.rs::registry_unreachable_from_agent
    status: verified
  - id: C4
    description: An agent cannot clear NoNewPrivs (EINVAL), cannot disable seccomp (EINVAL), cannot lift a denial with a stacked allow-all filter (io_uring_setup still EPERM), and a self-applied allow-all Landlock ruleset still cannot open B
    verification: tests/subset.rs::cannot_remove_or_widen (with an outside control)
    status: verified
  - id: C5
    description: Depth 0 is a usage error (64), depth 16 runs, the 17th stacked ruleset is E2BIG surfaced as exit 125
    verification: tests/subset.rs::depth_cap
    status: verified
  - id: C6
    description: A shell-out grandchild keeps the agent's cgroup and cgroup id, NoNewPrivs, seccomp mode 2 and the Landlock denial of B
    verification: tests/shellout.rs::shellout_inherits
    status: verified
  - id: C7
    description: A host below the ABI floor is refused 125 with EOPNOTSUPP and the missing rights, before any cgroup write
    verification: tests/shellout.rs::floor_refusal (SIMULATED - landlock_create_ruleset returns ENOSYS on a kernel that has Landlock ABI 7, via aok-shim landlock-enosys)
    status: verified-simulated
  - id: C8
    description: A grant that lets the agent run the launcher (and an explicit rx on its directory) cannot widen a sub launch; the sub-agent is denied B and also loses A, which its own layer did not grant
    verification: tests/shellout.rs::launch_cannot_widen
    status: verified
  - id: C9
    description: Known-denied io_uring numbers are EPERM inside the agent where the control sees success or a non-EPERM error; x32 is EPERM; gap and above-table numbers (both gap edges, 470, 5000) are ENOSYS; getpid and clone3 work
    verification: tests/errno_contract.rs::errno_contract
    status: verified
  - id: C10
    description: An i386 int 0x80 call inside the agent returns -EPERM and the agent exits 0 (no SIGSYS, no kill)
    verification: tests/errno_contract.rs::arch_mismatch_eperm
    status: verified
  - id: C11
    description: The assembled program loads only offsets 0 and 4, has no scratch-memory, index or ALU instruction, and returns only ALLOW or ERRNO actions
    verification: tests/errno_contract.rs::filter_no_ptr, seccomp::tests::program_reads_only_arch_and_nr_and_never_kills
    status: verified
  - id: C12
    description: A second allow-all filter stacked on the launcher filter does not lift a denial
    verification: tests/errno_contract.rs::subset_seccomp
    status: verified
  - id: C13
    description: The filter gives the documented action for every number 0..0x1000, for every x32 number and for four foreign arch values (in-test BPF interpreter against the table)
    verification: seccomp::tests::every_number_gets_the_documented_action, x32_and_foreign_arch_are_eperm_for_every_number
    status: verified
---

# Phase 9 Plan 3: Landlock layer, seccomp allow-list and containment tests Summary

The launcher now confines every agent for real: a Landlock ABI-6 ruleset (default grant, operator grants, D-12 implicit grants, D-14 floor from the embedded `aok-launch.conf`) and a hand-assembled seccomp allow-list over the v6.18 x86-64 syscall table, with the whole crate (09-01 and 09-02 tests included) green under default-deny in one delegated scope.

## What was built

- `src/landlock.rs`: `floor_from_conf`, `floor_check`, `parse_grant` (`rx:PATH`, `rw:PATH`, `tcp:PORT`), `preflight` (grant syntax then floor then path existence, all before any cgroup or registry write), `apply` (one `Ruleset` per spawn, `CompatLevel::HardRequirement`, fs V1-V5, `ConnectTcp`, scopes `Signal` and `AbstractUnixSocket`, status must be `FullyEnforced`). A kernel errno anywhere in the `landlock` crate's error chain is kept, which is how the 17th stacked layer surfaces as `E2BIG`.
- `src/seccomp.rs`: `TABLE`, `DENY`, `allow_ranges()`, `program()`, `apply()`. 71 instructions on this table.
- `tools/aok-launch/aok-launch.conf`: `landlock_abi_floor = 6`, labelled as the DEC-03c proposal. The DEC-03 mark edits the value and a rebuild picks it up. Not gitignored.
- Tests: `tests/subset.rs` (5), `tests/registry_confinement.rs` (1), `tests/errno_contract.rs` (4), `tests/shellout.rs` (3), plus 11 landlock and 6 seccomp unit tests.

Tracer gate: the Task 1 slice (`nnp`, `subset_landlock`, `zero_grant_denies`, `registry_unreachable_from_agent`) passed end to end before the seccomp and shell-out expansion. Tracer verified end-to-end, expanding.

## Allowlist record

09-06 writes `09-ALLOWLIST.md` from this record. The same text is the module doc of `src/seccomp.rs`.

- **Source.** `arch/x86/entry/syscalls/syscall_64.tbl` at the Linux v6.18 tag, fetched once during development from the kernel tree. The file is GPL and is not vendored; only the numbers and names are committed, as the `TABLE` const. Rows with ABI `common` and `64` are kept; `x32` rows are not.
- **Table.** 383 entries, numbers 0 to 336 and 424 to 469. Maximum 469 (`file_setattr`). One gap, 337 to 423 (87 numbers).
- **Coalesced allow ranges** (inclusive): 0-100, 102-154, 156-164, 167-174, 177-245, 247-297, 299-312, 314-319, 322, 324-336, 424, 434-441, 443-466, 468-469. 14 ranges, 360 allowed numbers.
- **Deny set** (23 numbers, `EPERM`):
  - `ptrace` 101: cross-process memory and register access.
  - `perf_event_open` 298: hardware counters, a side-channel and a large kernel attack surface.
  - `bpf` 321: loads programs into the kernel.
  - `userfaultfd` 323: lets user space stall the kernel at a chosen page fault, an exploit primitive.
  - Mount family: 165 `mount`, 166 `umount2`, 155 `pivot_root`, 428 `open_tree`, 429 `move_mount`, 430 `fsopen`, 431 `fsconfig`, 432 `fsmount`, 433 `fspick`, 442 `mount_setattr`, 467 `open_tree_attr`. These change mount topology, which Landlock path rules do not model.
  - `kexec_load` 246, `kexec_file_load` 320: replace the running kernel.
  - `init_module` 175, `delete_module` 176, `finit_module` 313: load or unload kernel code.
  - io_uring: 425 `io_uring_setup`, 426 `io_uring_enter`, 427 `io_uring_register`. Operations submitted through a ring do not pass this filter as syscalls.
- **`clone3` (435) is allowed** (D-02). Its flags sit in a user struct the filter never reads (D-04).
- **`statmount` (457) and `listmount` (458) stay allowed** (discretion): read-only views of the caller's own mount namespace.
- **x32.** Bit `0x40000000` set returns `EPERM` (D-05). An x32 call arrives under `AUDIT_ARCH_X86_64`, so the arch check passes and the bit test catches it.
- **Other architectures.** `arch != AUDIT_ARCH_X86_64` (the i386 `int 0x80` path and any other) returns `EPERM`, never a kill (D-05). The program never returns `KILL_PROCESS` or `KILL_THREAD`.
- **Gap and above-table** (337-423 and anything over 469, non-x32) return `ENOSYS` (D-01), what an older kernel returns, so libc probes for newer syscalls degrade.
- **What the filter reads.** Only `seccomp_data.arch` (offset 4) and `.nr` (offset 0). Never `args`, never `instruction_pointer` (D-04, LNCH-06). That the kernel then serves it from its seccomp action cache (Linux 5.11+) is ASSUMED, not measured.
- **Program layout.** arch compare, x32 bit test, one JGE/JGE/RET triple (or JEQ/RET for a one-number range) per allow range, 23 JEQ compares to a shared `RET ERRNO|EPERM`, fall-through `RET ERRNO|ENOSYS`. Every jump offset is under 255 (checked by `u8::try_from`).

### A1: do libc probes degrade on ENOSYS?

- The launcher's own probes (`clone3`, `ioctl(PIDFD_GET_INFO)`, `landlock_create_ruleset` version) are all in allow ranges, so the filter never changes their result. MEASURED: every launch in the crate suite runs through the filter and the spawn path works.
- glibc, MEASURED: `strace -f` over the launcher with `sh -c true`, `python3 -I -B -c pass`, `cat /proc/self/status` and `sleep 0.01` shows the 71-instruction filter installed and then no syscall in any of the four processes returning `EPERM` or `ENOSYS` after the install. The only post-install failures are `ENOENT` and Landlock `EACCES` from the shell's `PATH` search.
- musl 1.2.6: NOT MEASURED. No musl toolchain is on this host. Reasoned only: its probes are also ordinary syscalls inside the allow ranges and a probe above 469 gets the same `ENOSYS` an older kernel gives. Open verification item for the first musl-linked agent (B3).
- No probe was found that branches on a non-`ENOSYS` errno, so no allow-list decision checkpoint was needed. The denied families return `EPERM`, which `io_uring_disabled=2` and an unprivileged `bpf` call also return on a stock kernel.

## Verification

Run once at the end: `systemd-run --user --scope -q -p Delegate=yes -- cargo +1.99.1 test --manifest-path tools/aok-launch/Cargo.toml -- --test-threads=1`.

- lib: 54 passed (11 landlock, 6 seccomp, 37 earlier)
- tests/errno_contract.rs 4, identity.rs 14, lifecycle.rs 3, registry_confinement.rs 1, shellout.rs 3, spawn.rs 6, subset.rs 5, xattr_forgery.rs 1: all passed
- every test named in the plan's run list is in the run: `spawn_into_cgroup`, `sub_is_descendant`, `exit_contract`, `fallback_reaches_cgroup`, `freeze_thaw_kill`, `zero_member_kill`, `identity_from_pidfd`, `fallback_equivalence`, `registry_root_from_inside_agent_is_eacces_a` and `_b`, `registry_malformed_line_skipped`, `xattr_forgery`, `nnp`, `subset_landlock`, `zero_grant_denies`, `registry_unreachable_from_agent`, `cannot_remove_or_widen`, `depth_cap`, `errno_contract`, `arch_mismatch_eperm`, `filter_no_ptr`, `subset_seccomp`, `shellout_inherits`, `floor_refusal`, `launch_cannot_widen`
- `cargo +1.99.1 fmt --check` and `cargo +1.99.1 clippy --all-targets -- -D warnings` clean
- `arch_mismatch_eperm` did not skip (host `cc` present, IA32 emulation on)
- host: WSL2 kernel 6.18.33.2, Landlock ABI 7, cgroup v2 delegated scope

### Verified
- Everything in the coverage list on this host.
- Each inside denial has an outside control that succeeds (`registry_unreachable_from_agent`, `errno_contract` for io_uring_setup, `cannot_remove_or_widen`, `zero_grant_denies`), so the denial is the layer and not DAC or the kernel.

### Not verified
- musl behaviour under the filter (see A1).
- Any real below-floor kernel: `floor_refusal` is SIMULATED, the shim makes `landlock_create_ruleset` return `ENOSYS` on a kernel that has ABI 7.
- That the kernel's seccomp action cache is engaged (D-04, ASSUMED).
- Intermediate floors: only ABI 0 (absent) is exercised end to end; `floor_check(5, 6)` and the other values are unit tests of the pure function.
- Behaviour on a kernel with `CONFIG_IA32_EMULATION` off (the i386 test asserts a positive pid outside the agent and would fail, not skip, there).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Test expectation] io_uring_enter and io_uring_register control is EINVAL, not EBADF**
- **Found during:** Task 2, first run of `errno_contract`
- **Issue:** I expected the unfiltered control to see `EBADF` for a bad ring fd; the kernel returns `EINVAL`.
- **Fix:** the control now asserts `ret == -1` and `errno != EPERM`, which is what separates the filter from the kernel. The inside assertion is unchanged (`EPERM`).
- **Files modified:** `tests/errno_contract.rs`. **Commit:** 2d8248b

**2. [Rule 1 - Test expectation] landlock_create_ruleset returns an fd, not 0**
- **Found during:** Task 3, first run of `cannot_remove_or_widen`
- **Fix:** `s4_create` is asserted `>= 0`. **Files modified:** `tests/subset.rs`. **Commit:** b7e2030

### Other

- **Manual rule masking instead of `path_beneath_rules`.** The plan said to use `path_beneath_rules`. `landlock.rs` builds each rule itself and masks the rights to what a non-directory accepts (`Execute | WriteFile | ReadFile | Truncate | IoctlDev`), because the grants have three different right sets and the D-12 launcher grant is a single file. Same effect, no behaviour difference found.
- **`Scratch` fixture added to `tests/common/mod.rs` in Task 2, not Task 3**, so `subset.rs` and `errno_contract.rs` share it. `common/mod.rs` is in the plan's `files_modified`. The probe-line parser (`parse_probe`, `probe_line`) was added there in Task 3 for the same reason.
- The unprivileged `landlock_restrict_self` in `cannot_remove_or_widen` needs NoNewPrivs, which the agent already has; step (0) is a no-op there and sets it in the control.

## Grant adjustments to earlier tests

none. No 09-01 or 09-02 test needed a `--grant`, a moved scratch path or a helper change. The plan's predictions held: `freeze_thaw_kill` witnesses progress through `/proc/PID/stat`, and `xattr_forgery` does not launch through `aok-launch`. `tests/lifecycle.rs` and `tests/xattr_forgery.rs` are unchanged.

## Out-of-scope changes

none. All eight changed files are in the plan's `files_modified`. `STATE.md` and `ROADMAP.md` are untouched, `08-GATE.md` is untouched, and no DEC mark or OQ answer was typed.

## Raised, not fixed

- **Operator grant covering the registry (T-09-15 residual).** `registry_unreachable_from_agent` proves the registry is unreachable under the default and D-12 implicit grants only. An operator `--grant rw:/run/user/UID` or `rx:/` would expose it, and `preflight` does not refuse such a grant. Refusing a grant whose path covers `/run/user/UID/aok-launch` is a small follow-up; it is not built here. Belongs in `09-REPORT.md` (09-06).
- **musl under the filter is unmeasured** (A1). Needs a musl toolchain; first real check is the B3 libc work.
- **`/proc` is granted read-only wholesale.** `/proc/PID/mem`, `/proc/PID/environ` of other same-uid processes are still protected by ptrace access mode checks and `ptrace` is denied, but this plan did not test reading a sibling agent's `/proc/PID/environ`. Worth a negative test in a later plan.
- **The D-14 floor of 6 is a proposal.** The DEC-03 mark is not made here. Lowering it needs the config value edited and a rebuild.

## Known Stubs

None.

## Threat Flags

None new. T-09-06 (widening through a sub launch) and T-09-15 (registry under an agent's layer) are mitigated by the tests above, with the T-09-15 residual recorded under "Raised, not fixed".

## Self-Check: PASSED

- Files: `aok-launch.conf`, `landlock.rs`, `seccomp.rs`, `subset.rs`, `registry_confinement.rs`, `errno_contract.rs`, `shellout.rs`, `common/mod.rs` all present.
- Commits: 98ab5e5, 2d8248b, b7e2030 are ancestors of HEAD; `git rev-list --count 6d34a4a..HEAD` is 3.
