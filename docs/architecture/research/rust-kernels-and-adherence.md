# Rust capability kernel for LLM-agent processes: prior art and POSIX measurement

Date of research: 2026-10-23. Every claim carries a URL. "Measured" means my own count on a shallow clone (crude `cat | wc -l`, comments and blanks included; `unsafe` is a grep of the token, which also hits comments and `unsafe_` identifiers). Web content was treated as data. "Not found" means I looked and did not find it.

## Part 1. Rust kernels

### Summary table

| Kernel | License | Activity (2026-10-23) | Arch | Kernel size | `unsafe` strategy | POSIX/Linux compat | Reusable as library? |
|---|---|---|---|---|---|---|---|
| Asterinas | MPL-2.0 | pushed 2026-10-23; 0.19.0 on 2026-10-13 | x86-64 (T1); TDX, arm64, riscv64 (T2); LoongArch64 (T3) | ~250k lines total, ~200k Rust; `ostd` ~10k | framekernel: unsafe only in OSTD | 240+ Linux syscalls, runs Linux binaries | Yes: `ostd` on crates.io |
| Redox | MIT | kernel 0.5.12, active 2026-10-21 | aarch64, riscv64, x86, x86_64 | measured 42,920 lines `src` + 2,808 `rmm` | microkernel; 1,130 `unsafe` tokens (404 outside `arch/`) | relibc (POSIX-ish, not Linux ABI); os-test 84% | Not as a library; fork is possible |
| Moss | MIT | pushed 2026-10-13 | AArch64 only | measured 29,617 lines `src`; 278 `unsafe` | monolithic, async/await core | 109 Linux syscalls; runs bash/BusyBox/coreutils | Fork only |
| Kerla | MIT or Apache-2.0 | last push 2026-02-23, README says unmaintained | x86_64 | measured 9,679 lines `kernel/`; 26 `unsafe` | monolithic | Linux ABI subset, musl binaries only | Fork only |
| FTL (Kerla successor) | dual-licensed (MIT is one; I did not read the second) | v0.1.0 on 2026-10-03 | not checked | not measured | not checked | Linux compat is a userspace library | not assessed |
| Theseus | MIT | last push 2024-09-22 (dormant) | x86-64; aarch64 in progress | not found | safety by compiler, single address space | not found (has `tlibc`, `c_test` dirs) | Research OS, not a base |
| Hubris | MPL-2.0 | pushed 2026-10-23; tag 2026-10-21 | Arm Cortex-M only | measured 6,504 lines `sys/kern`; 108 `unsafe` | static task set, MPU | none | Wrong target class |
| Tock | Apache-2.0 / MIT | pushed 2026-10-23; 2.3.0 on 2026-10-08 | cortex-m, riscv, x86 dirs | measured `kernel/` 32,749 lines, 354 `unsafe` | capsules in safe Rust, apps MPU-isolated | none | Wrong target class |
| Maestro | AGPL-3.0 | pushed 2026-10-10 | not checked | not measured | not checked | "lightweight Unix-like", early stage | AGPL blocks reuse in most plans |
| rust-sel4 (userspace only) | mixed, incl. MIT, Apache-2.0, BSD-2, GPL-2.0 | v5.0.0 on 2026-07-22 | follows seL4 | n/a | Rust userspace on a C kernel | none | Yes, for a Rust userspace |

### Asterinas (the main candidate)

- Repo, README: https://github.com/asterinas/asterinas , https://raw.githubusercontent.com/asterinas/asterinas/main/README.md . API (license MPL-2.0, last push 2026-10-23, about 5.0k stars): https://api.github.com/repos/asterinas/asterinas
- 0.19.0 released 2026-10-13. The announcement says the codebase passed 250,000 lines (about 200,000 Rust), 100+ contributors, AArch64 became the fourth architecture, and the OSTD virtualization module is about 2,000 lines of unsafe: https://asterinas.github.io/2026/10/13/announcing-asterinas-0.19.0.html
- Architecture: framekernel. Only OSTD (the framework) may contain unsafe Rust; all kernel services are safe Rust in one address space. The page says the framework "enables others to create their own framekernels": https://raw.githubusercontent.com/asterinas/asterinas/main/book/src/kernel/the-framekernel-architecture.md
- OSTD size is stated as about 10K lines and the book includes a "100-line kernel in safe Rust" example: https://raw.githubusercontent.com/asterinas/asterinas/main/book/src/ostd/README.md
- Library use: `ostd` is on crates.io, 0.19.0 published 2026-10-12, MPL-2.0, about 33.8k downloads; 0.17.1 through 0.19.0 all published in 2026, so a release every 2-3 months: https://crates.io/api/v1/crates/ostd . MPL-2.0 is file-level copyleft: modified MPL files must stay MPL, new files may be any license (standard MPL-2.0 property; license text https://www.mozilla.org/en-US/MPL/2.0/).
- Linux compatibility: "240+ Linux system calls"; unsupported include shm*, msg*, mincore; kernel modules never supported: https://github.com/asterinas/asterinas/blob/main/book/src/kernel/linux-compatibility/README.md
- Architecture tiers (x86-64 T1; x86-64 TDX, ARM64, RISC-V 64 T2; LoongArch64 T3): README above.
- Conformance harness lives in `test/initramfs/src/conformance/{gvisor,kselftest,ltp,xfstests}`: https://raw.githubusercontent.com/asterinas/asterinas/main/test/initramfs/README.md . LTP runs `--run-suite syscalls` through kirk and CI requires failed=0, broken=0, warnings=0 within the allowlist: https://raw.githubusercontent.com/asterinas/asterinas/main/test/initramfs/src/conformance/ltp/run_ltp_test.sh
- My count of `testcases/all.txt` in that LTP dir: about 564 enabled and about 966 commented-out entries. Heuristic, not a published figure. Asterinas publishes no headline pass-rate I could find.
- Fit for this project: a capability microkernel does not match Asterinas' monolithic Linux-ABI services, but OSTD (memory frames, page tables, tasks, interrupts, arch support, with the unsafe quarantined) is the layer you would otherwise write first. Whether OSTD's API is stable enough to build a different kernel structure on: not found (no stability statement read).

### Redox

- Repos: https://gitlab.redox-os.org/redox-os/redox , GitHub mirror (MIT): https://api.github.com/repos/redox-os/redox . Kernel 0.5.12, last activity 2026-10-23: https://gitlab.redox-os.org/redox-os/kernel . Arch dirs aarch64, riscv64, x86, x86_64, x86_shared (GitLab API for that repo).
- Measured: `src` 42,920 lines Rust, `rmm` 2,808; 1,130 `unsafe` tokens in `src`, 404 outside `arch/`. The book says "less than 40,000 Rust lines of kernel code" and "any component which can run in user-space should": https://doc.redox-os.org/book/microkernels.html
- Object model: schemes are userspace resource providers, paths `/scheme/{name}/{resource}`. Capability work funded by NLnet: namespace and CWD become file descriptors, userspace namespace manager `nsmgr`, openat with O_RESOLVE_BENEATH as the sandbox idea: https://www.redox-os.org/news/nlnet-cap-nsmgr-cwd/ (2026-03-17).
- libc: relibc, 77.7% Rust / 20% C: https://gitlab.redox-os.org/redox-os/relibc . `redox-rt` does fork/exec/signals in userspace.
- Latest named release 0.9.0. The site dates it 2024-09-09: https://www.redox-os.org/news/release-0.9.0/ ; the GitHub mirror lists a 0.9.0 release dated 2025-09-22. Unexplained discrepancy; I did not resolve it. No 1.0 post found.
- Reuse: closest in spirit (microkernel, capability direction, Rust), but its capability model is still being retrofitted onto a Unix-like fd/scheme design. Forking it means inheriting scheme semantics.

### Moss

- https://github.com/hexagonal-sun/moss-kernel (MIT, last push 2026-10-13, 1.96k stars). README: https://raw.githubusercontent.com/hexagonal-sun/moss-kernel/master/README.md
- AArch64 only, 109 Linux syscalls, async/await kernel core, EEVDF scheduler, runs a dynamically linked Arch Linux AArch64 userspace including bash, BusyBox, coreutils; ptrace enough for strace; `usertest` suite and 230+ `libkernel` tests (README).
- Measured 29,617 lines in `src`, 278 `unsafe`. Started about Nov 2025 (git history). Evidence that a Rust kernel can reach bash/coreutils on 109 syscalls in under a year.

### Kerla and FTL

- Kerla: https://github.com/nuta/kerla , MIT or Apache-2.0, README says no longer maintained, last push 2026-02-23. x86_64, musl-linked binaries only, glibc not supported. Many calls (lseek, mprotect, munmap) are listed unimplemented: https://raw.githubusercontent.com/nuta/kerla/main/Documentation/compatibility.md . Measured 9,679 lines in `kernel/`, 26 `unsafe`.
- FTL: https://github.com/nuta/ftl , v0.1.0 on 2026-10-03. Design: small kernel with "minimalistic hypervisor-shaped system calls"; Linux compatibility implemented as a userspace "userspace OS" library; each container has its own VM space: https://raw.githubusercontent.com/nuta/ftl/main/README.md . Directly relevant shape (small kernel, personality in userspace), but v0.1.0 and its author's previous kernel was abandoned. I did not measure it.

### Theseus, Hubris, Tock, Maestro

- Theseus: https://github.com/theseus-os/Theseus , MIT, last push 2024-09-22, no releases, 1,263 commits. "Cells" (one per crate), single address space and privilege level, compiler-enforced safety: https://www.theseus-os.com/Theseus/book/design/design.html , https://www.usenix.org/conference/osdi20/presentation/boos . LOC, unsafe stats, POSIX level: not found.
- Hubris: https://github.com/oxidecomputer/hubris , MPL-2.0. Only `sys/kern/src/arch/arm_m.rs` as arch file. Measured `sys/kern` 6,504 lines, 108 `unsafe`. Tasks are static, declared in `app.toml` with flash/RAM budgets. Good model for a tiny, auditable kernel; microcontroller only, no POSIX.
- Tock: https://github.com/tock/tock , Apache-2.0/MIT, 2.3.0 on 2026-10-08. Measured `kernel/` 32,749 lines (354 `unsafe`), `arch/` 18,405, `capsules/` 89,061. Capsules are untrusted-by-Rust, apps isolated by MPU. Microcontroller class, no POSIX. It has an `AGENTS.md` in the repo.
- Maestro: https://github.com/maestro-os/maestro , AGPL-3.0, last push 2026-10-10, README: "early stage ... highly unstable ... Do not use it in production": https://raw.githubusercontent.com/maestro-os/maestro/master/README.md . Not examined further.

### rust-sel4 / seL4

- rust-sel4 v5.0.0 on 2026-07-22 supports seL4 16.0.0 and Microkit 2.3.0; provides `sel4`, `sel4-sys`, root-task and Microkit runtimes, capDL initializer, kernel loader; not yet on crates.io: https://github.com/seL4/rust-sel4
- seL4 kernel is C, GPL-2.0-only, 16.0.0 on 2026-07-22: https://github.com/seL4/seL4 . Verified configurations: AArch32 (functional correctness, integrity/availability, confidentiality, binary), AARCH64 (first three), RISCV64 non-MCS (first three, no fast path), X64 only C-level functional correctness; MCS proofs in progress; no IOMMU/boot/debug coverage: https://docs.sel4.systems/projects/sel4/verified-configurations.html
- Using seL4 means Rust userspace on a C kernel, not a Rust kernel. It is the only option here with a proof story.

### Part 1 read-out

- Nobody has a Rust kernel with a first-class capability object model plus Linux/POSIX compatibility. Closest: Redox (capabilities retrofitted), FTL (small kernel + userspace Linux library, too young), seL4 + rust-sel4 (best capability model, C kernel, POSIX is on you).
- Cheapest way not to start at zero: Asterinas OSTD as the unsafe-confined hardware layer (~10k lines, crates.io, MPL-2.0), with your own capability-based services above it.
- Size reference for "small": Hubris 6.5k, Kerla 9.7k, Moss 29.6k, Redox 43k+2.8k, Asterinas 200k Rust.

## Part 2. Capability models for delegating authority

### seL4

- Source: seL4 Reference Manual v16.0.0 (22 July 2026): https://sel4.systems/Info/Docs/seL4-manual-latest.pdf
- Attenuation: capabilities are minted with a subset of rights, "never with more rights". Rights are Read, Write, Grant, GrantReply, on Endpoints, Notifications, Pages, Reply. Badged endpoint caps via mint; a badge cannot be removed or changed afterwards. Propagation follows a take-grant model.
- Revocation: the kernel tracks derivations in a capability derivation tree (CDT), stored as part of the CNode object. `seL4_CNode_Revoke` deletes all CDT children of a cap and leaves the cap itself, across all CSpaces. It can partially complete (revoking thread destroyed, or authority removed mid-operation). Deleting the last typed capability destroys the object.

### Zircon (Fuchsia)

- Rights belong to the handle, not the object; `zx_handle_duplicate` requires ZX_RIGHT_DUPLICATE; `zx_handle_replace` makes an equivalent handle, possibly with fewer rights, and invalidates the original: https://fuchsia.dev/fuchsia-src/concepts/kernel/handles
- Attenuation on duplicate: requested rights must be "strictly lesser" than the source's (ZX_RIGHT_SAME_RIGHTS and ZX_RIGHT_NONE are the special cases): https://fuchsia.dev/reference/syscalls/handle_duplicate
- Revocation: not described in either page. Closing a handle does not destroy the object. No CDT-style transitive revoke found.

### Capsicum (FreeBSD)

- Source: Watson et al., USENIX Security 2010, summarized at https://research.google/pubs/capsicum-practical-capabilities-for-unix/ , slides https://www.cl.cam.ac.uk/research/security/capsicum/slides/20100811-usenix-capsicum.pdf , docs https://www.cl.cam.ac.uk/research/security/capsicum/documentation.html . I read search-result excerpts of these, not the full paper.
- Capabilities are file descriptors with extra rights constraints. A rights-limited descriptor can only be narrowed further; directory capabilities allow subtree delegation. Rights propagate by inheritance or message passing.
- Capability mode: set by `cap_enter`, inherited by descendants, cannot be cleared; denies global namespaces (filesystem, PIDs, SysV IPC).
- The 2010 paper names the call `cap_new`; the later FreeBSD name `cap_rights_limit` is not confirmed by a source I read.
- Revocation: not found in the sources read. Fd model implies you close your own descriptors, but no transitive revoke is documented there.

### KeyKOS / EROS / Coyotos

- Sources are 2005-2006 mailing-list posts by Jonathan Shapiro, not papers; treat as secondary: https://lists.nongnu.org/archive/html/l4-hurd/2005-10/msg00044.html , https://lists.endsoftwarepatents.org/archive/html/l4-hurd/2005-10/msg00040.html , https://lists.endsoftwarepatents.org/archive/html/l4-hurd/2006-04/msg00055.html . I did not locate the EROS SOSP'99 paper or the Coyotos spec pages.
- Object-level revocation: destroying an object increments its allocation count, which invalidates all keys to it (on disk); in-memory (prepared) keys go through an indirection table entry that is invalidated.
- Selective revocation: EROS puts the real capability in a space-bank-allocated node and hands out a wrapper to it; revoke by deallocating the node. Coyotos uses a kernel-implemented transparent forwarding object. Shapiro states the wrapper design costs extra IPCs, and that real-world depth never exceeded 3 across Gnosis/KeyKOS/EROS (limit in EROS: 20).
- Design principle quoted there: the party who pays for revocation bookkeeping must be able to deallocate it.

### CHERI

- Source: CHERI ISA technical report UCAM-CL-TR-987: https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-987.pdf (downloaded and grepped).
- Attenuation: capabilities carry bounds and permissions. "Guarded manipulation" (section 2.3.4) means capability instructions cannot increase rights; in CHERI-RISC-V a non-monotonic modification clears the tag instead of raising an exception. Tags make forging impossible; the report calls the result "non-bypassable capability monotonicity".
- Revocation: not a delegation-tree revoke. Revocation is a temporal-safety sweep that finds and invalidates capabilities to freed memory (section 2.3.16; appendix C.3 "Hierarchical Revocation From Ephemeral Capabilities" exists, not read in detail). Cornucopia: average overhead under 2%, worst case 8.9% on compatible SPEC CPU2006: https://www.repository.cam.ac.uk/handle/1810/304040 , https://www.cl.cam.ac.uk/~tmj32/papers/docs/filardo20-sp.pdf . Cornucopia Reloaded (load barrier, near-zero pauses): https://microsoft.com/en-us/research/uploads/prod/2024/02/revocation3-paper.pdf .
- Hardware: CHERI needs CHERI silicon or emulation (Morello, CHERI-RISC-V); not available on the generic x86-64/QEMU targets Asterinas supports. Availability of CHERI hardware in 2026 beyond that: not checked.

### Capabilities for LLM agents, 2025-2026

- CaMeL, "Defeating Prompt Injections by Design", Debenedetti, Shumailov et al. (Google, Google DeepMind, ETH Zurich), arXiv 2503.18813, 2025-03-24: https://arxiv.org/abs/2503.18813 , PDF https://arxiv.org/pdf/2503.18813 . Mechanism: extracts control and data flow from the trusted user query so untrusted data cannot change program flow; values carry a "capability" (provenance/permission metadata) to block exfiltration over unauthorized flows; privileged LLM plans, quarantined LLM reads untrusted data. Result on AgentDojo: 77% tasks with provable security vs 84% undefended in the latest version (https://sotaverified.org/papers/defeating-prompt-injections-by-design ; v1 cited 67%, https://liner.com/review/defeating-prompt-injections-by-design ). Code: github.com/google-research/camel-prompt-injection (per sotaverified, not opened). Caveat: these "capabilities" are interpreter-level taint/policy tags, not kernel capabilities. They enforce at tool-call time in a Python interpreter. A kernel capability system would sit underneath this, not replace it.
- AgenticOS, arXiv 2606.21129 (2026-06-19): argues POSIX "resource exposure plus permission checks" is the wrong model for agents, proposes an intent-oriented secure OS architecture with delegable, auditable OS-native capabilities: https://arxiv.org/abs/2606.21129 . I read the abstract only; whether it has an implementation: not checked.
- Agent libOS, arXiv 2606.03895 (2026-06-02): library-OS substrate for long-running agents with process identity, Task Authority ceilings, typed Capabilities, policy or human approval, budgets, information-flow labels: https://arxiv.org/abs/2606.03895 . Runs above a host OS, so no kernel isolation (per search summary; abstract read).
- Formal analysis and supply chain security for agentic AI skills, arXiv 2603.00195 (2026-02-27): ocap-grounded skill sandbox, a skill confined to its declared capability set; 5 theorems, open-source tool "skillfortify": https://arxiv.org/abs/2603.00195 . Abstract read; the ocap framing is from search snippet, not verified in the paper body.
- Aethelgard, arXiv 2604.11839 (2026-04-12): learned least-privilege capability sets per task, enforced at tool-call layer, motivated by a measured "15x overprovision ratio": https://arxiv.org/abs/2604.11839
- Container-escape benchmark, arXiv 2603.02277: frontier models can exploit sandbox weaknesses when vulnerabilities exist (search snippet only, paper not opened): https://arxiv.org/abs/2603.02277
- Not found: any published kernel-level capability system (seL4-style or Zircon-style) evaluated with LLM agents as the processes. The 2026 work I found is application- or library-OS-level. That gap is the novelty claim for this project, with the caveat that I did one search pass.

### Part 2 read-out for design

- Attenuation is solved identically everywhere (derive with subset of rights, never more). Pick seL4/Zircon style: rights on the reference, mint/duplicate with a mask.
- Revocation is the real design choice. Options with evidence: seL4 CDT (transitive, kernel-tracked, partial-completion hazards), EROS/Coyotos forwarders (revoke per delegate, cost in IPC hops), Zircon (none found), CHERI (memory-lifetime sweep, not delegation). For agents, per-delegation revoke (forwarder style) maps to "end this tool grant" better than a global tree.

## Part 3. Measuring POSIX adherence without certification

### Open POSIX Test Suite (OPTS), inside LTP

- Path `testcases/open_posix_testsuite` in https://github.com/linux-test-project/ltp , GPL-2.0, LTP release 20260930. README: targets IEEE 1003.1-2001 System Interfaces, "implementation-agnostic", usable on any POSIX OS. Categories conformance, functional, stress, performance, speculative.
- Measured in a sparse clone: `conformance/interfaces` 191 interface dirs with 1,753 .c/.sh files; `conformance/definitions` 261 files; `behavior` 5; `functional` 15; `stress` 51.
- Effectively dormant: one commit touching it since 2025-10-01 (a mailmap fix).
- Running: `./configure && make all`; per area `make generate-makefiles; cd conformance/interfaces/X; make all test`; option groups AIO, MEM, MSG, SEM, SIG, THR, TMR, TPS via `bin/run-posix-option-group-test.sh`. `BUILD` has FreeBSD notes; CFLAGS/LDLIBS come from the environment. Each test is its own compile/link/run, so build and run separate cleanly for cross-compiling to a new OS.
- Result codes (PASS 0, FAIL 1, UNRESOLVED 2, UNSUPPORTED 4, UNTESTED 5) from IEEE 1003.3-1991 methods; a definitions test passes if it compiles (HOWTO_ResultCodes in that dir).
- Redox's assessment: GPL-2, unmaintained since about 2006, POSIX.1-2001 only: https://www.redox-os.org/news/posix-conformance-testing/

### Sortix os-test

- https://sortix.org/os-test/ ; source https://gitlab.com/sortix/os-test (ISC, last activity 2026-10-19; NLnet NGI Zero Commons funded). Targets POSIX.1-2024.
- Suites: basic, fnmatch, include, io, limits, malloc, namespace, paths, process, pty, regex, signal, stdio, udp (plus posix-parse in the repo). A suite is a directory plus a `.expect` directory.
- Publication: a matrix across OSes; each cell is a percentage and (good/total non-neutral), split by mandatory POSIX and each option; "extension" and "previous_posix" outcomes shown separately.
- Reported systems: AIX 7.3, DragonFly 6.4, FreeBSD 15.0, Haiku, Hurd, Linux 7.1.12, macOS 26.5.2, Managarm, Minix 3.4.0, musl on Linux 6.18, NetBSD 10.1, OmniOS, OpenBSD 7.8, Redox 0.5.12, Solaris 11.4, Sortix 1.1.0-dev.
- Overall scores as read from the page on 2026-10-23:

| OS | Score | Good/total |
|---|---|---|
| musl (on Linux) | 94% | 4296/4566 |
| Managarm | 94% | 4143/4380 |
| Linux | 92% | 4226/4565 |
| Hurd | 91% | 4169/4575 |
| FreeBSD | 90% | 4139/4582 |
| Sortix | 85% | 3891/4568 |
| Redox | 84% | 3842/4557 |
| macOS | 83% | 3825/4565 |
| Minix | 73% | 3357/4560 |

- Redox `basic` suite 76% (746/979); Linux `basic` 91% (892/979).
- Running: `make test`, `make html`, `make json`, `make jsonl`; needs only make and a C compiler; tests cross-compile and run over ssh; per-OS scripts in `os/`. Redox and Managarm results are pulled from their CI.
- Denominators differ by OS (4380 to 4582) because tests that don't apply are neutral. Compare percentages, not counts.

### musl libc-test

- Cloned from https://repo.or.cz/libc-test.git (git.musl-libc.org returned 502). MIT. Last commits Aug 2026.
- Measured dirs: src/api 80, functional 85, math 206, regression 72, common 14, musl 3. Each test is standalone and returns 0 on success; the build produces a `REPORT` file and continues past build failures; run via `runtest` with timeouts; needs only libc, POSIX sh, GNU make.
- Mostly tests the libc, not kernel services; not updated for POSIX.1-2024 (Redox post above).

### The Open Group VSX-PCTS2016 / VSC-PCTS2016

- Catalog (updated 4 September 2026): https://posix.opengroup.org/testsuites.html . VSX-PCTS2016 v1.15 (GA 12 Feb 2025), 1003.1-2016 System Interfaces. VSC-PCTS2016 v3.1 (GA 11 Nov 2025), 1003.1-2016 Shell and Utilities. Older VSX/VSC-PCTS2003 3.1. PSE52/54 realtime suites. No POSIX.1-2024 suite listed.
- Fee: free for certification-track users; open-source projects get "a twelve month free license" via obconformance at opengroup.org (same page).
- Open-source licence text: https://posix.opengroup.org/docs/VSC-PCTS2016-TestSuite-TimeLimited-License-Agreement-OSS-v1.4.pdf (VSX version identical by my grep). Terms: time-limited 12 months from the Open Group's email; non-exclusive, non-transferable; assess conformance to IEEE 1003.1-2016 only; must submit feedback; may not publicly disclose results or claim a product "passed" without prior written consent; destroy the suites within 10 days of expiry; no sublicense or redistribution; data and written materials related to use assigned to the Open Group; English law; may contain a disabling device.
- Net: free, not open source, not publishable, expires, 2016 not 2024. Redox says the suites need TET/VSXgen, are complex, and have cross-build limits: https://www.redox-os.org/news/posix-conformance-testing/ . Commercial path quoted there as "small number of thousands of dollars".

### Application suites as functional checks (measured on shallow clones)

- BusyBox testsuite: README says run `./runtest` in `testsuite/`. Mirror https://github.com/mirror/busybox (commit date of my clone: 2024-07-14, so the mirror may be stale; upstream not checked). 66 `*.tests` files plus per-applet directories, 327 files in `testsuite/`. Upstream site git.busybox.net returned 404 for the raw paths I tried.
- GNU coreutils: https://github.com/coreutils/coreutils `tests/` has 970 files (665 `.sh`, 64 `.pl`, 237 without extension) per the GitHub tree API; run through `make check`. Needs the full autotools build and many tools on the target; savannah cgit returned 502.
- bash: https://git.savannah.gnu.org/git/bash.git (clone date 2026-09-15 HEAD) `tests/` has 83 `*.tests` files; README: "Type `sh run-all'". Output is diffed against expected files.
- These check that real programs work (fork/exec/pipes/signals/termios/fs) and are a different signal from conformance suites. They fail for reasons unrelated to POSIX (GNU extensions, locales, missing /proc), so a pass/fail count needs a triaged baseline file.
- Moss claims bash/BusyBox/coreutils run: https://raw.githubusercontent.com/hexagonal-sun/moss-kernel/master/README.md

### How Redox and Asterinas report numbers

- Redox: os-test matrix pulled from CI (os-test page above), relibc `tests/` with expected outputs (about 97 entries; "each MR needs to give success in all of them"), bespoke POSIX.1-2024 signal tests built under NLnet: https://www.redox-os.org/news/posix-conformance-testing/ , https://gitlab.redox-os.org/redox-os/relibc
- Asterinas: syscall-support table and per-syscall flag coverage pages, plus allowlist/blocklist runs of LTP, gVisor, kselftest (pinned to Linux v6.18), xfstests gated in CI: Asterinas links in Part 1. No headline percentage.

### Recommended "adherence bar" (my interpretation; none of this is a published standard)

Constraint from the above: nothing free is both current (POSIX.1-2024) and kernel-level. os-test is the only current free one; OPTS is old but broad on interfaces; the Open Group suites are unpublishable.

Option A, single headline number (cheapest): os-test overall % (mandatory POSIX only), generated by `make json` in CI against your libc.
- Baselines: Redox 84%, Sortix 85%, Linux 92%, musl 94%, Minix 73%.
- Suggested tiers: 70% (Minix-class, "boots and mostly works"), 84% (Redox-class), 90% (FreeBSD/Linux-class).
- Weakness: libc and kernel are both in the number. If you use musl, the libc ceiling is 94% and shortfalls are yours.

Option B, per-subsystem gate (what Asterinas does): an allowlist file per suite (os-test suites, OPTS `conformance/interfaces` areas, libc-test `src/api`+`functional`+`regression`) with CI failing on any regression and the allowlist only growing.
- Number to report: enabled/total per suite.
- Strongest regression control; weakest as a single claim.

Option C, application bar: BusyBox testsuite + bash `run-all` + coreutils `make check` pass with a triaged expected-fail list; report passes/total.
- Proves real software runs. Do after A, as bash/coreutils need a lot of surface.

Option D, certification-grade, internal only: 12-month Open Group VSX/VSC-PCTS2016 licence for a private gap analysis. You cannot publish results or say "passed", and it tests 2016 not 2024. Worth it only once A reaches the 84% tier.

My recommendation: A as the public headline, B as the CI mechanism that enforces it, C as the milestone gate for "usable shell", D skipped until there is a reason to pay for it. Per your own rigor rule a bar needs a number, a deadline, and a baseline: the numbers above are baselines; the deadline and target tier are yours to set and I have not set them.

What would show the bar is wrong: if the os-test number is dominated by libc-only or header tests (the `include`, `namespace`, `limits` suites) that a kernel never touches, then the percentage rises without kernel progress. Check per-suite before committing; I did not.

## Not checked / limits

- Did not read the full Capsicum, EROS, seL4 papers beyond the manual and excerpts; EROS/Coyotos from list posts only.
- Did not run any suite; all numbers are read from published pages or counted from clones.
- Did not measure FTL, Maestro, Theseus LOC/unsafe; Theseus POSIX level not found.
- CHERI hardware availability and the AgenticOS implementation status not checked.
