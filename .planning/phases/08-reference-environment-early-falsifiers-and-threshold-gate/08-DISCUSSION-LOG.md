# Phase 8: Reference environment, early falsifiers and threshold gate - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-28
**Phase:** 8-Reference environment, early falsifiers and threshold gate
**Areas discussed:** Reference host, BPF-LSM harness, Adversarial stub, Threshold gate

All four offered areas were selected. Host probe run before the first question (VERIFIED, shell
output, 2026-10-27): WSL2 kernel 6.18.33.2, `CONFIG_BPF_LSM=y`, `CONFIG_CGROUP_BPF=y`, `CONFIG_LSM`
without `bpf`, no `lsm=` on the command line, no `/dev/kvm`, no passwordless sudo, no `bpftool`.

---

## Reference host

### Q1: Which environment is the reference kernel and host?

| Option | Description | Selected |
|--------|-------------|----------|
| This WSL2, edit .wslconfig | Add `lsm=...,bpf` via `kernelCommandLine`; free and fast; Microsoft's kernel, not a target distro; WSL2 honouring `lsm=` ASSUMED | ✓ (superseded by Q2 and Q3) |
| Cloud VM, distro kernel | Real target-distro kernel with root; costs money and an account | |
| Bare metal or dual-boot | Cleanest timing; needs spare hardware or a partition | |
| Local VM | Needs nested virtualization; no `/dev/kvm` here | |

**User's choice:** This WSL2, edit .wslconfig.
**Notes:** Claude objected once before Q2: the baseline run was on the same WSL2 host, so HARN-01's
transfer test would become a same-host repeatability check.

### Q2: If the .wslconfig edit does not take, what is the fallback?

| Option | Description | Selected |
|--------|-------------|----------|
| Cloud VM, capped | Rent Ubuntu 24.04 VM with root and a stated cap | |
| Block, run 9 and 10 | Mark Phase 8 blocked; run Phases 9 and 10 meanwhile | |
| Custom WSL2 kernel | Build a kernel with `bpf` in `CONFIG_LSM` | |

**User's choice:** "Dualboot" (free text).

### Q3: Time-box for the WSL2 route before switching to dual-boot?

| Option | Description | Selected |
|--------|-------------|----------|
| One attempt | Edit, shutdown, re-probe, one retry | |
| One session | A few hours of parameter variants | |
| Two days | Look for a newer Microsoft kernel or prep the partition | |

**User's choice:** "just switch to dual boot" (free text).
**Notes:** Read as: dual-boot is the reference host and the WSL2 attempt is dropped. Claude stated
that reading in the next question; the user did not correct it. That reading also removes the
objection from Q1.

### Q4: Which distro and kernel is the reference?

| Option | Description | Selected |
|--------|-------------|----------|
| Debian 13 (6.12.113) | LTS-shaped, `bpf` in `CONFIG_LSM`; lacks several newer floor rows | |
| Fedora 44 (7.2.13) | Meets every floor row; least representative of an LTS | |
| Ubuntu 24.04 HWE (7.0, -42+) | Common base; needs build -42 or later; misses only AUTOKILL and NNP | ✓ |
| Arch (7.2.11) | Rolling, near-mainline; kernel moves between runs | |

**User's choice:** Ubuntu 24.04 HWE (7.0, -42+).

### Q5 (after "More questions"): By when is the install up and probed?

| Option | Description | Selected |
|--------|-------------|----------|
| Within 1 week | Firm date; Phases 9 and 10 run meanwhile | ✓ |
| No date, hardware unsure | Open dependency | |
| Already have a partition | Install is hours of work | |

**User's choice:** Within 1 week (recorded as 2026-11-04).

### Q6: What must the probe show before the host is ready?

| Option | Description | Selected |
|--------|-------------|----------|
| LSM + attach + rows | `bpf` in active LSMs, BTF, ABI, floor rows, global `lsm/file_open` attaches; `lsm_cgroup` excluded | ✓ |
| Plus lsm_cgroup | Couples host readiness to the HARN-03 falsifier | |
| Env file only | No attach test | |

**User's choice:** LSM + attach + rows.

---

## BPF-LSM harness

### Q1: Where do the BPF-LSM variants live in the harness?

| Option | Description | Selected |
|--------|-------------|----------|
| External state + --tag | Root wrapper loads BPF and places the run; `micro.c`/`sandbox.c` unchanged; gcc and glibc only | ✓ |
| New configs in sandbox.c | Self-check inside micro via raw `bpf(2)` on pinned maps | |
| Separate bpf harness | Duplicates leg code; rows stop being comparable | |

**User's choice:** External state + --tag.

### Q2: How does a run prove the programs attached and fired?

| Option | Description | Selected |
|--------|-------------|----------|
| Counter map + denial probe | Per-hook counters read before/after each leg, plus a sentinel denial; counter cost charged to the BPF-LSM share | ✓ |
| Denial probe only | Proves attach and decision, not firing on every call | |
| bpftool prog show | Proves attach only | |

**User's choice:** Counter map + denial probe.
**Notes:** The counter increment biases the timed rows upward (conservative for a falsifier).

### Q3: Which unix-socket send hook?

| Option | Description | Selected |
|--------|-------------|----------|
| lsm/socket_sendmsg | Generic sendmsg hook; matches S3 wording | ✓ |
| lsm/unix_may_send | AF_UNIX datagram only | |
| Both, measured apart | Three rows; longer run | |

**User's choice:** lsm/socket_sendmsg. Attachability ASSUMED until first load.

### Q4: Which uid runs the timed micro process?

| Option | Description | Selected |
|--------|-------------|----------|
| Unprivileged, root only for setup | Same credentials as the baseline run | ✓ |
| Root for everything | Confounds HARN-01 transfer with uid | |

**User's choice:** Unprivileged, root only for setup.
**Notes:** Continue check answered "Next area". Open items left to the planner: sibling and root
cgroup rows, counter-bias reporting.

---

## Adversarial stub

### Q1: What are the cases and the stub broker written in?

| Option | Description | Selected |
|--------|-------------|----------|
| C cases + Python runner | Matches `tools/linux-baseline` precedent, no new toolchain (recommended) | |
| All C | One build; report assembly is awkward | |
| Rust | Cargo workspace in `tools/`; anticipates the real broker | ✓ |

**User's choice:** Rust (not the recommended option).
**Notes:** Claude objected once: raw-syscall code needs `unsafe` and a syscall crate;
`tools/unsafe_check.py` covers only `kernel/` and `userland/`; the choice must not decide Phase 10's
language. Q2 resolved it.

### Q2: Unsafe and toolchain policy, and does it bind Phase 10?

| Option | Description | Selected |
|--------|-------------|----------|
| libc crate, one sys module | Stable Rust, unsafe in one module with SAFETY comments, outside the ledger; Phase 10 language open | ✓ |
| Same, under the ledger | Extend `unsafe_check.py` roots and ledger | |
| rustix, minimal unsafe | Coverage of ptrace, `pidfd_getfd`, `io_uring_setup` ASSUMED | |

**User's choice:** libc crate, one sys module.

### Q3: What does the stub treat as identity (impersonation attempt)?

| Option | Description | Selected |
|--------|-------------|----------|
| Kernel-stamped cgroup id | `SO_PEERPIDFD` + `PIDFD_GET_INFO`; attempts: cgroup move, borrowed connected fd, payload claim as control | ✓ |
| Claimed id in payload | Impersonation always succeeds; count says nothing about trigger (c) | |
| Both, two stubs | Naive control plus kernel-stamped stub | |

**User's choice:** Kernel-stamped cgroup id.

### Q4: How fine is a route?

| Option | Description | Selected |
|--------|-------------|----------|
| Enumerated routes | Fixed route list per attempt type, one row each; invalid if the non-agent run fails it | ✓ |
| One route per type | Five rows; a 0 is weak evidence | |

**User's choice:** Enumerated routes.

### Q5: How many S1 cases at Phase 8's close?

| Option | Description | Selected |
|--------|-------------|----------|
| One per class (about 8) | Skeleton; later phases extend | ✓ |
| Three per class (about 24) | More variety before any mediation exists | |
| One per attachable hook family | Tied to a V-src list | |

**User's choice:** One per class (about 8).
**Notes:** Continue check answered "Next area". Open items left to the planner: the S5 deny rule,
stub restart between kill attempts.

---

## Threshold gate

### Q1: DEC-02: commit to a decision rule now, or decide at the gate?

| Option | Description | Selected |
|--------|-------------|----------|
| Pre-commit: keep bound | Option 1; trigger (b) reads as fired if the reference host reproduces the openat miss and a profile attributes it to the kernel path | ✓ |
| Pre-commit: R1 mechanics | Option 2 applied mechanically; cannot fail on the legs it revises | |
| Decide at the gate | Nothing committed; can be fitted to the result | |

**User's choice:** Pre-commit: keep bound.
**Notes:** Claude objected once before the question: R1 sets each bound from the data it is tested
against, so trigger (b) cannot fire on revised legs; the unrevised bound already fails on
`openat_tmpfs` (25.01% p50, 30.09% p99 on WSL2, no BPF-LSM).

### Q2: Which rows are decided at the gate and which deferred?

| Option | Description | Selected |
|--------|-------------|----------|
| Defer S7, S8 only | Roadmap default; S1-S6, S9, POSX-02 decided | ✓ |
| Also defer S1, S9, POSX-02 | Five rows open; trigger (a) cannot fire until S1 is accepted | |
| Decide all ten now | S7 and S8 accepted with no backend and ungrounded numbers | |

**User's choice:** Defer S7, S8 only.

### Q3: How is the gate run?

| Option | Description | Selected |
|--------|-------------|----------|
| Gate sheet, then transcribe | `08-GATE.md` per row with evidence; user marks; plan transcribes to section 4 and the ADR | ✓ |
| Live walk-through | Rows one at a time in a session | |
| You edit the cells | A check script verifies format | |

**User's choice:** Gate sheet, then transcribe.

### Q4: How is DEC-03 decided?

| Option | Description | Selected |
|--------|-------------|----------|
| On the gate sheet | Own block with the reference-host probe and the distro-kernels table | ✓ |
| Pre-accept as proposed | Locks the ABI 6 minimum and the HWE reading before the first probe | |

**User's choice:** On the gate sheet.

**Done check:** "I'm ready for context" (offered: explore more gray areas; more on threshold gate).

---

## Claude's Discretion

S5 deny rule and its hooks; stub restart between kill attempts; sibling and root cgroup rows in the
per-cgroup config; counter-bias reporting; the numeric test for "reproduces the openat miss";
names and locations of the wrapper, crate, report and gate sheet; whether the Rust suite and the
non-agent baseline can be built before the dual-boot is up; whether the R1 arithmetic is scripted
(`summarize.py` has none).

## Deferred Ideas

Phase 10 broker language; a naive claimed-id second stub; measuring `lsm/unix_may_send`; three S1
cases per class or one per hook family; Debian, Fedora, RHEL-family and Arch verdicts (Phase 12).
