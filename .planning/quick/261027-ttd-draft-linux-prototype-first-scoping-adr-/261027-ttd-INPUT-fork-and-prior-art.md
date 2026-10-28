# Linux fork of agent-aware kernel objects: maintenance cost and prior art

Date of research: 2026-10-27. Read-only on the repo. Repo context read: docs/architecture/scoping.md sections 2, 3, 6.

Evidence labels:
- VERIFIED: I fetched the page this session and read the stated fact in it.
- SNIPPET: seen only in a search-result summary; I did not fetch the page. Treat as a lead, not as confirmed.
- ASSUMED: my inference or memory, no source read.

Tool limits: WebSearch is US-only and returns summaries. I did not query lore.kernel.org directly. "Found nothing" below means nothing in about 30 searches and fetches, not a proof of absence.

---

## Part A. Fork maintenance cost

### A0. Kernel version facts as of 2026-10-27

| Fact | Value | Label | Source |
|---|---|---|---|
| Mainline | 7.3, released 2026-10-18 | VERIFIED | https://www.kernel.org/releases.json |
| Stable | 7.2.13 (2026-10-26) | VERIFIED | https://www.kernel.org/releases.json |
| LTS list | 6.18 (6.18.59), 6.12 (6.12.114), 6.6 (6.6.160), 6.1 (6.1.190), 5.15, 5.10 | VERIFIED | https://www.kernel.org/releases.json |
| LTS EOL | 6.18: Dec 2028. 6.12: Dec 2028. 6.6: Dec 2027. 6.1: Dec 2027. 5.15 and 5.10: Dec 2026 | VERIFIED | https://www.kernel.org/category/releases.html (page dated 2026-02-25) |
| LTS extension history | 6.12 moved from Dec 2026 to Dec 2028 on 2026-02-25. 6.18 was set to Dec 2027 then listed Dec 2028 on kernel.org. Dates have moved before and can move again | SNIPPET | https://phoronix.com/news/Linux-6.18-LTS-6.12-6.6-Extend |
| Mainline cadence | A release every 9-10 weeks, stable weekly | VERIFIED | https://www.kernel.org/category/releases.html |
| 7.3 cycle size | 18,765 non-merge commits, 3,021 developers (807 first-time). A record | VERIFIED | https://lwn.net/Articles/1101143/ |
| 7.2 cycle size | 13,412 non-merge commits | SNIPPET | search summary citing LWN and 7.2-rc1 announcement |
| 7.3 contents relevant to us | sched_ext subscheduler support "now complete"; new fchroot(2) syscall; working-set tracking for VMs | VERIFIED | https://lwn.net/Articles/1101143/ |
| Next LTS | 7.4 is the likely 2026 LTS (kernel convention is the last release of the year). 7.4 merge window opened 2026-10-19 | ASSUMED | no source states 7.4 will be LTS |

Derived: 6.12.114 means 114 stable releases in about 23 months (6.12 released 2024-11-17). Stable queue sizes seen: 6.12.64-rc1 had 567 patches, 6.12.67-rc1 had 139, 6.12.68-rc1 had 169, 6.12.2-rc1 had 826 (SNIPPET, https://lkml.iu.edu/2601.0/06263.html and https://lkml.iu.edu/2601.3/06557.html).

### A1. Precedents, with measured numbers

| Precedent | What was carried | Measured cost / numbers | Outcome | Label | Source |
|---|---|---|---|---|---|
| Asahi Linux (team, not single maintainer; one person, Janne, owns the rebase) | Downstream kernel patch set | 1250 patches, 90,000+ lines as of 6.13. Rebasing, "hours of work on an almost weekly basis" (porting to each stable point release). Then 1232 patches / 95,000 lines at 6.13.8 down to 858 patches / 83,000 lines at 6.18.8, through upstreaming. The article itself notes the 30% patch drop vs 13% line drop looks off. The 6.19 report gives no rebase-hours figure | Still carrying about 858 patches after 5+ years; only shrinks by upstreaming | VERIFIED | https://asahilinux.org/2025/03/progress-report-6-14/ and https://asahilinux.org/2026/02/progress-report-6-19/ |
| PREEMPT_RT | Out-of-tree patch set, rebased per kernel | 223 patches / 8,950 insertions at 3.0-rt (2011), 462 patches at 2.6.33. 57 patches at 5.19, 50 at 6.0-rc1 (5 trivial Kconfig, a few POWER, a few i915). Merged in 6.12 (Sept 2024) after "twenty years" | About 20 years out of tree, then merged; shrinking required upstreaming infrastructure first. The 2009 LWN piece shows the rebase pain was already a stated problem | VERIFIED for 57/50 (https://www.phoronix.com/news/Linux-6.0-RT-Patches). SNIPPET for 223/462 and 20 years (https://www.osadl.org/Single-View.111+M5aa4cd59136.0.html, https://en.wikipedia.org/wiki/PREEMPT_RT) | |
| sched_ext | BPF extensible scheduler class, out of tree | v4 posted July 2023, rejected by Zijlstra on maintenance-burden grounds. Another version November 2023, v6 early May 2024. Failed 6.11 merge, merged in 6.12. Heo argued out-of-tree status means "rebasing is painful" and no convenient distribution | About 10+ months of review as of the May 2024 LWN article; merged after roughly 15 months from v4. No hours figure published | VERIFIED for dates (https://lwn.net/Articles/972710). SNIPPET for the Heo quote (https://lkml.rescloud.iu.edu/2405.3/07388.html) | |
| Android GKI / ACK | Vendor kernels on top of LTS | Pre-GKI device kernels "as much as 50% of kernel code being out-of-tree". Up to 18 months from an upstream LTS fix to a device. GKI cut the out-of-tree delta but "somewhat artificially" because some patches became vendor hooks in modules. ACK policy: out-of-tree patches need an issue explaining why they cannot go upstream | Google, with a large team, keeps ACK on LTS and pushes vendors to modules plus a stable KMI. I found no absolute patch counts per ACK branch | VERIFIED for 50% and 18 months (https://source.android.com/docs/core/architecture/kernel/generic-kernel-image). SNIPPET for the vendor-hook comment (LPC 2023 abstract, https://lpc.events/event/17/contributions/1442/) | |
| Rust-for-Linux | Out-of-tree then linux-next | Initial merge for 6.1: 27 patches, about 12.5k lines (v10). In linux-next about 1.5 years first. Post-merge, Asahi describes dropping their downstream abstractions and rebasing each time one lands upstream as "grueling" work | Merged 6.1. "Experiment" concluded in the 7.0 cycle | SNIPPET | https://www.phoronix.com/news/Rust-Is-Merged-Linux-6.1 and https://phoronix.com/news/linux-7.0-rust (page returned 403 to my fetch) |
| grsecurity / PaX | Large hardening patch set | Stable series made paid-only in 2015. Test series ended with 4.9 in April 2017. Gentoo hardened-sources retired for lack of a regular patching schedule; NixOS dropped it. Current stable kernels supported "a minimum of three years" | A funded company carried it for about 15 years, then closed it. Downstream distros could not keep up with free patches | SNIPPET | https://www.theregister.com/2017/04/26/grsecurity_linux_kernel_freeloaders/ and https://grsecurity.net/stability_in_the_wake_of_linux_xlts_sunsetting |
| Clear Linux (Intel) | Distro with kernel tuning | Intel ended it 2025-07-18, no security patches, repo archived. No cause published. I found nothing on what happened to its kernel patches | Corporate sponsor withdrew; project ended abruptly | SNIPPET | https://phoronix.com/news/Intel-Ends-Clear-Linux |
| linux-hardened (single-maintainer lineage) | Hardening patches originated by Daniel Micay | Now maintained by Levente Polyak (Arch). The Debian fork's rationale: "Tracking a tree that plenty of other people already run beats maintaining a private fork only we ever look at". 2018 Arch forum thread: point-release rebases arriving weeks after the kernel | Handed from original author to a distro maintainer. No patch count or hours figure found | VERIFIED for maintainer and quote (https://unredacted.org/blog/2026/06/linux-hardened-unredacted-a-hardened-kernel-for-debian/). SNIPPET for 2018 lag (https://bbs.archlinux.org/viewtopic.php?id=236532) | |
| AgentCgroup (a 2026 research example of carried patches) | Uses `memcg_bpf_ops`, which is still an RFC series (v7, about May 2026, not merged as far as I found) | Demonstrates how a research artifact depends on an unmerged out-of-tree series | The README says Linux 6.12+ with sched_ext and does not say how memcg_bpf_ops is supplied | VERIFIED for README (https://github.com/eunomia-bpf/agentcgroup). SNIPPET for RFC status (https://patchew.org/linux/cover.1779760876.git.zhuhui@kylinos.cn/) | |

What I did not find: any published per-release rebase hours for a single maintainer, any conflict-rate statistic (how many hunks fail per release for a given patch count), Clear Linux's kernel delta size, grsecurity's patch size per release. Do not quote a number for these.

### A2. What the evidence supports (interpretation, not sourced)

1. The only quantified rebase cost I have is Asahi: one dedicated person, hours weekly, for 858-1250 patches and 83-95k lines. That is a large delta. A fork limited to a few thousand lines in new files should cost far less, but I have no measurement of that. ASSUMED.
2. Cost is driven by how many existing files the delta touches, not by line count. New files and new Kconfig/Makefile entries rebase nearly free; hooks in `task_struct`, `fork`, scheduler core, credentials, VFS rebase badly. RT's last 50 patches were mostly Kconfig and arch glue, which fits. ASSUMED.
3. Every precedent that survived long either shrank the delta by upstreaming (RT, sched_ext, Rust, Asahi) or had a funded team (Google ACK, grsecurity). Those that depended on one sponsor ended (Clear Linux, grsecurity public releases). A one-person-plus-agents project has no sponsor, so the shrink-by-upstreaming path or a very small delta are the two viable shapes. Interpretation of the rows above.
4. Cadence recommendation: pin one LTS (6.18 or 6.12, both EOL Dec 2028) and take stable point releases, which arrive roughly weekly with 130-570 patches each. Rebase the series onto a new LTS about once a year, not onto each mainline release. Tracking mainline means a rebase every 9-10 weeks against a record 18,765-commit cycle (7.3). The 7.3 cycle adds sched_ext subscheduler completion, so a fork based on 6.18 misses that. ASSUMED as a policy; the inputs are VERIFIED.
5. Agents can do mechanical rebases, but the 7.3 release notes tie record commit volume to AI-assisted growth (SNIPPET, https://www.warp2search.net/story/linux-kernel-73-released-record-commit-count-and-aidriven-growth/), so churn is rising. I found no data on agent-assisted rebase success rates. ASSUMED.

### A3. New in-tree Rust subsystem or driver for a new kernel object type

- Rust is permanent in mainline: the "Rust experiment" was formally concluded in the 7.0 cycle (SNIPPET, https://phoronix.com/news/linux-7.0-rust).
- The 7.3 Rust API docs list these modules in the `kernel` crate: cred, security, task, pid_namespace, fs, ioctl, uaccess, miscdevice, configfs, sync, xarray, maple_tree, workqueue, seq_file, tracepoint, net, mm, kunit, debugfs, drm, and others. Not present: bpf, cgroup. A module named `file` is not listed (the summarizer said so; `kernel::fs` exists and may hold File). VERIFIED, https://rust.docs.kernel.org/kernel/ (documents v7.3).
- Practical shape of a Rust "agent object" today: a misc character device with ioctls (`kernel::miscdevice`, `kernel::ioctl`, `kernel::uaccess`), per-open state in an `xarray`, reading credentials via `cred` and `task`, namespaces via `pid_namespace`. miscdevice abstractions were written for Rust Binder and Greg KH said to stick to the misc device interface for char devices (SNIPPET, https://lkml.iu.edu/hypermail/linux/kernel/2410.1/07697.html). Inference from the module list: an ioctl-driven device is the path of least resistance; it is not a new syscall.
- New syscalls: I found no sign that a syscall can be defined from Rust. `SYSCALL_DEFINEn` is a C macro (SNIPPET, https://lwn.net/Articles/604406/). Nothing found either way for Rust-defined syscalls, so assume a C shim is needed. ASSUMED.
- Hooking mediation: Rust LSM abstractions exist only as an RFC. v2 posted 2026-03-11 (`kernel::lsm`, three hooks: file_open, task_alloc, task_free; one Rust LSM per build; no security blobs yet). Paul Moore said upstream does not accept sample LSMs and the shim must come with a real LSM. Alice Ryhl asked who the end user is. Miguel Ojeda criticized the patches as heavily LLM-generated and insufficiently reviewed (https://kernsec.org/pipermail/linux-security-module-archive/2026-March/056267.html, VERIFIED for Moore and Ryhl; the Ojeda remark is SNIPPET from the search summary). I did not find whether it was later merged. `kernel::security` exists in 7.3 docs, but I did not read what it contains.
- Process: upstreaming a new kernel object type needs a real in-kernel user and a maintainer. The Rust-LSM thread is a direct example of reviewers rejecting abstractions with no consumer. Interpretation.
- Cost of in-tree Rust: Rust abstractions change quickly; Asahi reports the load rising as abstractions are upstreamed (VERIFIED, https://asahilinux.org/2025/03/progress-report-6-14/).
- Scale reality check: the 7.3 Rust pull requests were "Rust updates" and "Rust fixes" from Ojeda (SNIPPET, https://lwn.net/Articles/1091420/); I did not read their contents.

---

## Part B. Prior art, 2025-2026

### B0. Short answer

Nobody has built the project's idea as a whole. The pieces exist in separate places:
- Agent identity plus budgets plus capability-typed authority plus approvals: only in user-space runtimes (Agent libOS, AIOS) or architecture papers with no stated prototype (AgentKernel, AgenticOS).
- Kernel enforcement for agents: built on stock mainline mechanisms (Landlock, seccomp, bubblewrap, eBPF, sched_ext). None of what I read patches the kernel source.
- Inference scheduling in the kernel: gpu_ext (eBPF hooks in the GPU driver). No work found that makes inference requests a first-class kernel scheduled object with a per-agent budget.
- LKML: nothing found proposing agent identity, token budgets, or an agent LSM. The agent threads I found are about AI coding assistants contributing patches.

### B1. Table

| Work | What it does | Modifies the kernel? | Status | Label | Link |
|---|---|---|---|---|---|
| AIOS (Rutgers, agiresearch) | "AIOS kernel" providing scheduling, context, memory, storage, access control for LLM agents. FIFO and RR scheduling. Up to 2.1x faster execution | No. README: "an abstraction layer over the operating system kernel"; Python server (uvicorn); experimental Rust rewrite `aios-rs/` not complete. Access control not described in README | arXiv v1 2024-03-25, v5 2025-08-12, COLM 2025. 6.5k stars. Latest news item in README 2025-07-08 | VERIFIED | https://arxiv.org/abs/2403.16971 and https://github.com/agiresearch/AIOS |
| LiteCUA | Computer-use agent built on AIOS | No (built on AIOS) | arXiv 2505.18829, May 2025 | SNIPPET | https://arxiv.org/pdf/2505.18829 |
| AgentCgroup (Zheng et al., eunomia-bpf) | cgroup hierarchy aligned to tool calls; enforcement with sched_ext (CPU) and `memcg_bpf_ops` (memory). Measured 144 SWE-rebench tasks, two models: OS-level execution is 55-60% of end-to-end latency, memory spikes up to 15.4x peak-to-average | Not by source in the abstract; needs Linux 6.12+ with sched_ext; `memcg_bpf_ops` is an unmerged RFC series | arXiv 2602.09345, v1 2026-02-10, v3 2026-07-22. GPL-2.0, 83 stars, research artifact | VERIFIED | https://arxiv.org/abs/2602.09345 and https://github.com/eunomia-bpf/agentcgroup |
| ActPlane (Zheng et al.) | Agents declare policies in an IFC DSL; enforced in kernel via eBPF with semantic feedback. 1.9%-8.4% overhead; covers paths that tool-call interception misses | No kernel patches mentioned; eBPF | arXiv 2606.25189, 2026-06-23, v2 2026-06-30; code github.com/eunomia-bpf/ActPlane | VERIFIED (abstract) | https://arxiv.org/abs/2606.25189 |
| AgentSight (Zheng et al.) | eBPF "boundary tracing": TLS interception for intent plus kernel events, correlated. Under 3% overhead (authors' claim) | No. Needs root or CAP_BPF; kernel 5.0+ recommended | arXiv 2508.02736 (Aug 2025, PACMI 2025). MIT, about 757 stars, active | VERIFIED | https://arxiv.org/abs/2508.02736 and https://github.com/eunomia-bpf/agentsight |
| Sandlock (Wang, Zheng) | Unprivileged process sandbox for agent code. Landlock (fs, TCP ports, IPC scoping), seccomp-bpf, seccomp user notification; supervisor for runtime decisions. About 5 ms startup, Redis within noise of bare metal | No. Needs Linux 6.12+ for Landlock ABI v6 | arXiv 2605.26298, 2026-05-25. Apache-2.0, Rust, 587 stars, 1,363 commits | VERIFIED | https://arxiv.org/abs/2605.26298 and https://github.com/multikernel/sandlock |
| gpu_ext (Zheng et al.) | eBPF policies in the GPU driver and a device-side eBPF runtime; scheduling and memory placement hooks. Up to 4.8x throughput, up to 2x lower tail latency on inference, training, vector search | Extends GPU driver (an SNIPPET says built on Linux 6.15 and NVIDIA open modules; the abstract page does not say). So yes at driver level | arXiv 2512.12615, Dec 2025. No code link on the abstract page | VERIFIED for abstract/numbers. SNIPPET for 6.15 and NVIDIA modules (https://arxiv.org/html/2512.12615v2) | https://arxiv.org/abs/2512.12615 |
| Agent libOS (Zhang) | Library-OS runtime: process identity, authority ceilings, typed capabilities, approvals, budgets, information-flow labels, causal evidence log. States it provides no kernel-grade sandboxing and does not stop prompt injection | No; user space above a host OS | arXiv 2606.03895, v1 2026-06-02, v3 2026-08-18. 33/33 deterministic tasks pass. Code: github.com/yingqi-z20/Agent-libOS | VERIFIED | https://arxiv.org/abs/2606.03895 |
| AgentKernel (Zou et al.) | "Trust-native agentic OS" with Identity, Perception, Cognition, Execution pillars; mandatory enforcement boundary | Not stated | arXiv 2609.29647, 2026-08-29. 38 pages. No code link, no numbers on the page | VERIFIED (abstract only) | https://arxiv.org/abs/2609.29647 |
| AgenticOS (Zhao et al.) | Intent-oriented secure OS architecture: Ghost Kernel, Logic Shutter, Agent Capsule, Semantic Boundary Gateway, Intent ABI | Not stated | arXiv 2606.21129, 2026-06-19. 11 pages. No code link, no numbers | VERIFIED (abstract only) | https://arxiv.org/abs/2606.21129 |
| Ring-0 by Default: Agents as Operating Systems (Li et al.) | Maps 116 OS mechanisms across 19 subsystems to agent counterparts: 11 isomorphic, 65 missing. Tests Claude Code, Codex CLI, Gemini CLI: per-tool checks bypassable through the shell; all three load forged transcripts on resume | No prototype, no kernel work | arXiv 2610.33753, 2026-10-24 | VERIFIED | https://arxiv.org/abs/2610.33753 |
| Towards an Agent Operating System (Steinder, Franke) | Position paper: extend classical and cloud OS primitives to agent execution | Not stated; no prototype in abstract | arXiv 2607.25076, 2026-07-27 | VERIFIED (abstract) | https://arxiv.org/abs/2607.25076 |
| YoloFS, "Agent-native filesystems" (Zhong, Arpaci-Dusseau et al.) | Staged mutations, snapshots, progressive permissions for agents. 290 public failure reports analysed; 112 routine tasks at baseline success with less interaction | Not stated on abstract page | arXiv 2604.13536, v5 2026-09-16; github.com/YoloFS/YoloFS | VERIFIED (abstract) | https://arxiv.org/abs/2604.13536 |
| Anthropic sandbox-runtime (`srt`) | Filesystem and network restrictions for processes, behind Claude Code's sandbox. Linux: bubblewrap, seccomp (blocks unix sockets, new user/mount namespaces), HTTP/SOCKS proxy for domain allowlists | No | Beta research preview, Apache-2.0 | VERIFIED | https://github.com/anthropic-experimental/sandbox-runtime |
| OpenAI Codex CLI sandbox | Linux: "bwrap plus seccomp by default" per OpenAI's current docs. Landlock appears only as a `codex sandbox landlock` test helper on that page. Older reports (and a search summary of an earlier doc) say Landlock plus seccomp was the default | No | Shipping | VERIFIED for current default (https://learn.chatgpt.com/docs/agent-approvals-security). Earlier Landlock-default claim is SNIPPET and appears outdated | |
| Kubernetes agent-sandbox (SIG Apps) | Sandbox CRD for isolated stateful singleton agent workloads; isolation delegated to gVisor or Kata Containers. Firecracker only underneath Kata | No | Announced KubeCon Atlanta Nov 2025; GKE GA reported May 2026 | SNIPPET | https://kubernetes.io/blog/2026/03/20/running-agents-on-kubernetes-with-agent-sandbox |
| E2B / Modal | E2B: each sandbox a Firecracker microVM with its own kernel, snapshot/resume. Modal: gVisor | No (use VM or userspace-kernel isolation) | Commercial products | SNIPPET (vendor and comparison pages) | https://e2b.dev/index.md and https://www.morphllm.com/comparisons/e2b-vs-modal |
| Landlock upstream | 7.1: access right for pathname UNIX sockets. 7.2: UDP bind/connect/send control, audit "quiet" flag. LSS Europe 2026 talk on scoping and roadmap | This is mainline | In tree. The summaries found nothing agent-specific in Landlock | SNIPPET | https://kernsec.org/pipermail/linux-security-module-archive/2026-June/058131.html |
| Agent identity standards | Linux Foundation DNS-based Agent Name Service; NIST NCCoE concept paper on AI agent identity and authorization. Application layer | No | 2026 | SNIPPET | https://www.biometricupdate.com/202606/linux-foundation-unveils-dns-based-identity-standard-for-ai-agents |
| LKML "agent" threads | Sasha Levin, July 2025, config for coding agents contributing to the kernel; Assisted-by tag RFC March 2026. Concerns AI as contributor, not agent as kernel principal | n/a | Policy discussion | SNIPPET | https://lkml.org/lkml/2025/7/27/294 and https://lkml.org/lkml/2026/3/4/1747 |

### B2. Direct answers to the sub-questions

- Kernel-level agent identity or budget proposals on LKML: found none. Closest unrelated precedent for a new accounting controller is the misc cgroup (SNIPPET, https://lkml.iu.edu/hypermail/linux/kernel/2103.0/08049.html). A GPU cgroup controller was proposed in 2019-2022 (v7, May 2022) and not merged as far as I could tell (SNIPPET, https://lkml.iu.edu/hypermail/linux/kernel/2205.1/03943.html). I found no kernel token-budget proposal.
- Kernel-side LLM inference scheduling: gpu_ext is the only kernel-adjacent work found. The other 2026 results are user-space serving theory (KV-cache scheduling, e.g. https://arxiv.org/html/2601.22996, SNIPPET). Nothing found that treats inference requests as kernel-scheduled objects with per-principal accounting, which is scoping.md section 3.4.
- BPF-LSM / seccomp / Landlock agent sandboxes: well covered (Sandlock, srt, Codex, ActPlane, agent-lock mentioned in a search summary). All run on unmodified kernels.
- Firecracker/gVisor: used as the isolation boundary by commercial sandboxes; the agent concept does not exist inside those kernels.

### B3. What this means for the fork decision (interpretation)

- The same group (Yusheng Zheng and Andi Quinn, eunomia-bpf; Dan Williams co-authors gpu_ext and ActPlane) has built agent resource control, policy enforcement, observability and GPU policy entirely on eBPF, sched_ext and cgroups. That is a working demonstration that a large share of the scoping.md goals 2-4 (mediation, scheduling, accounting) can be prototyped without carrying a kernel patch series. ASSUMED that the same holds at the depth the project wants, because I did not read their evaluations beyond the abstracts.
- What the stock mechanisms plausibly cannot give (ASSUMED, not tested): a kernel-recognised agent credential inherited across fork/exec with subset-only delegation (scoping 3.1); a trusted escalation path (3.3); budgets enforced in the kernel for tokens and money (no kernel object counts tokens). Those are the candidates for a small fork or an out-of-tree LSM/module.
- Warning sign: AgentCgroup, the closest prior art, already depends on an unmerged kernel series (`memcg_bpf_ops`), so even "no fork" prototypes can end up carrying patches.
- Novelty risk is low for the combined design (agent as a kernel object with identity, capability set, budgets, mediation, and inference device), and high for any single piece in isolation (sandboxing, observability, GPU policy).

### B4. Unchecked leads

- Rust LSM series after March 2026: merged or dropped? Not checked.
- Contents of the 7.3 Rust pull requests.
- agent-lock (eBPF lsm/file_open agent tool): seen only in a search summary, no repo link.
- Whether memcg_bpf_ops merged after v7.
- Full texts of AgentKernel, AgenticOS, ActPlane implementation sections (only abstracts read).
- Android ACK absolute out-of-tree patch counts (try `git log` on android.googlesource.com/kernel/common for the android16-6.12 branch).
