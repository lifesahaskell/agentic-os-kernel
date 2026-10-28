# Mainline Linux vs. agentic-OS goals 1-4: what exists, what is left

Date: 2026-10-27. Baseline: mainline 7.3 (released 2026-10-18; stable 7.2.13, LTS 6.18.59) [V-doc: https://www.kernel.org/].
Scope: goals 1-4 of docs/architecture/scoping.md section 2 (read first, lines 30-61, plus primitives 3.1-3.5).

## Evidence tags

- V-src: I downloaded the file at tag v7.3 and grepped/read it this session. URL pattern: `https://raw.githubusercontent.com/torvalds/linux/v7.3/<path>`. Local copies are in `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/551cda72-80af-415e-95df-8a64bb7a2418/scratchpad/src/`.
- V-doc: read via WebFetch (a small model summarised the page; quotes are the summary's, not mine). Treat as second-hand.
- V-search: only a search-result snippet or a fetched mailing-list/news page; not kernel source.
- A: ASSUMED, from memory, not checked this session.

Gap labels: **US** closable in userspace; **BPF** closable with BPF (incl. BPF-LSM, cgroup-BPF, sched_ext, seccomp cBPF); **PATCH** needs a kernel patch (file/subsystem named).

## Headline

Across goals 1-4 I found no gap that *must* be a kernel patch to meet the stated goal text. Everything that is "needs PATCH" below is a tamper-resistance, atomicity, or overhead improvement over a userspace/BPF design, or a hardware-scheduling feature that only matters if agents each hold their own GPU context. The one place a patch changes what is achievable (rather than how cheaply) is per-agent GPU time sharing between processes (goal 3), and that only applies to a local GPU, not to a remote API.
Not found / not verified: any number for mediation overhead (success criterion in scoping.md section 2 is TBD); I measured nothing.

---

## Cross-cutting: building blocks that are present in 7.3

| Block | What it gives | Evidence |
|---|---|---|
| `clone3` flags `CLONE_INTO_CGROUP`, `CLONE_PIDFD`, `CLONE_PIDFD_AUTOKILL`, `CLONE_NNP`, `CLONE_EMPTY_MNTNS` | Spawn a child directly into a named cgroup (no window outside it), get a pidfd, kill the child when the pidfd closes, force no_new_privs, empty mount ns | V-src `include/uapi/linux/sched.h` lines 15, 38, 40-42 |
| pidfd: `PIDFD_GET_INFO` with `PIDFD_INFO_CGROUPID`, `PIDFD_INFO_CREDS`, `PIDFD_INFO_EXIT`; ioctls to fetch the target's ipc/mnt/net/pid/time/user/uts/cgroup namespaces; pidfs gives a 64-bit non-recycled id | Race-free handle to an agent process; its cgroup id is readable from the handle | V-src `include/uapi/linux/pidfd.h`, `fs/pidfs.c` (64-bit `pidfs_ino_nr`, lines 77-112) |
| `pidfd_getfd(2)` | Take an fd out of another process; gated by `PTRACE_MODE_ATTACH_REALCREDS`; no cooperation needed | V-doc https://man7.org/linux/man-pages/man2/pidfd_getfd.2.html (since 5.6) |
| Unix-socket peer identity: `SO_PEERCRED`, `SO_PEERPIDFD`, `SO_PASSPIDFD` | Receiver gets a kernel-stamped pidfd of the sender (per message with SO_PASSPIDFD) | V-src `include/uapi/asm-generic/socket.h` lines 30, 135, 136 |
| `SO_PEERCGROUPID` or similar | Not present in v7.3 | V-src same file (absent from PEER* list) |
| cgroup v2 `cgroup.kill`, `cgroup.freeze`, `cgroup.events`, delegation, cgroup namespaces, `cgroup.max.descendants/depth` | Lifecycle (kill/freeze/populated notifications) and containment of a subtree | V-doc https://docs.kernel.org/admin-guide/cgroup-v2.html |
| BPF helpers `bpf_get_current_cgroup_id`, `bpf_get_current_ancestor_cgroup_id`, `bpf_task_storage_get`, `bpf_cgrp_storage_get` in the base proto set | A BPF program can key policy by cgroup (ancestor level = agent) and keep per-task state | V-src `kernel/bpf/helpers.c` lines 2168-2186 |

Design implication (interpretation): "agent_id" can be a cgroup (one cgroup per agent, sub-agents are child cgroups). The cgroup id is kernel-stamped on every unix-socket peer via pidfd. That covers goal 1 identity without a new cred field.

---

## Goal 1: agents as first-class principals (identity, capabilities, budgets, lifecycle)

### What mainline provides

| Item | Provides | Limits | Evidence |
|---|---|---|---|
| cgroup v2 `cpu` | `cpu.weight` (1-10000), `cpu.max` ("$MAX $PERIOD"), `cpu.max.burst`, `cpu.uclamp.*`, `cpu.idle`, `cpu.stat` | Time budget as bandwidth, not as a total CPU-seconds quota | V-doc cgroup-v2 |
| `memory` | `memory.min/low/high/max`, `memory.swap.max`, `memory.zswap.max`, `memory.reclaim`, `memory.pressure`, `memory.events`, `memory.oom.group`; tracks user memory, kernel structures, TCP buffers | See goal 4 | V-doc cgroup-v2 |
| `io` | `io.max` (bps/iops), `io.weight`, `io.latency`, `io.cost.*` | Block IO only | V-doc cgroup-v2 |
| `pids` | `pids.max`; fork/clone returns -EAGAIN at the limit | Counts tasks, nothing else | V-doc cgroup-v2 |
| `misc` | `misc.max/current/peak/events` for a *compile-time* enum of resources (SEV, SEV_ES, TDX only) | Not extensible at runtime; charge API is in-kernel only (`misc_cg_try_charge` is `EXPORT_SYMBOL_GPL`, no uAPI to charge) | V-src `include/linux/misc_cgroup.h` lines 15-25; `kernel/cgroup/misc.c` lines 20, 75-94, 186, 206 |
| `dmem` | Device (VRAM) memory limits `dmem.max/min/low/current`; TTM charges BOs; xe and amdgpu implement `dmem_cgroup_ops` with reclaim | Needs driver registration (`dmem_cgroup_register_region`) | V-doc cgroup-v2; V-src `drivers/gpu/drm/xe/xe_ttm_vram_mgr.c` line 304, `drivers/gpu/drm/amd/amdgpu/amdgpu_vram_mgr.c` line 938, `drivers/gpu/drm/ttm/ttm_resource.c` line 417 |
| `devices`, `rdma`, `hugetlb`, `cpuset`, `perf_event`, `freezer` | Unrelated to agents except `devices` (BPF device cgroup) as a deny-list for `/dev/accel*` and GPU nodes | | V-src `include/linux/cgroup_subsys.h` (full list: cpuset, cpu, cpuacct, io, memory, devices, freezer, net_cls, perf_event, net_prio, hugetlb, pids, rdma, misc, dmem, debug) |
| Landlock (ABI 11 in 7.3 docs) | Unprivileged, stackable, monotonic-narrowing policy: FS access rights, TCP and UDP bind/connect by port, abstract-unix-socket and signal scopes, pathname unix sockets, ioctl on devices; max 16 layers; inherited by children; cannot be removed | Policy is per thread/domain, not a named capability object; cannot restrict `chdir`, `stat`, `chmod`, `chown`, `setxattr`, mount; net rules are port-only, no IP/host | V-doc https://docs.kernel.org/userspace-api/landlock.html |
| `seccomp` cBPF + `SECCOMP_RET_USER_NOTIF` | Syscall allowlist; supervisor can answer a syscall and inject fds (`SECCOMP_IOCTL_NOTIF_ADDFD`) | Cannot dereference pointers; docs: "System call filtering isn't a sandbox"; TOCTOU on tracee memory; notification can be aborted by preemption | V-doc https://docs.kernel.org/userspace-api/seccomp_filter.html |
| BPF-LSM | Attach to ~all LSM hooks incl. `task_alloc`, `task_free`, `cred_prepare`, `bprm_check_security`, `file_open`, `path_*`, `socket_connect`, `socket_sendmsg`, `unix_stream_connect`, `task_kill`, `ptrace_access_check`, `uring_*`; per-cgroup attach via `BPF_LSM_CGROUP`; sleepable on a defined subset | Needs privileged loader (doc: "by privileged users"); a small set of hooks is attach-disabled (`vm_enough_memory`, `file_alloc_security`, `getprocattr`, ...) | V-src `include/linux/lsm_hook_defs.h`; `kernel/bpf/bpf_lsm.c` lines 39-57 (disabled set), 232 (BPF_LSM_CGROUP), 291+ (sleepable set); V-doc https://docs.kernel.org/bpf/prog_lsm.html |
| sched_ext | CPU scheduler as a BPF struct_ops, with `cgroup_init/cgroup_set_weight/cgroup_set_bandwidth/cgroup_set_idle/cgroup_move` callbacks; hierarchical per-cgroup sub-schedulers (`CONFIG_EXT_SUB_SCHED`, `kernel/sched/ext/sub.c`); falls back to fair on error or stall; no ABI stability | CPU only. No ABI stability means a BPF scheduler tracks kernel versions | V-src `kernel/sched/ext/sub.c` lines 1-15, `kernel/sched/ext/ext.c` lines 4882-5076; V-doc https://docs.kernel.org/scheduler/sched-ext.html |
| Namespaces | cgroup, ipc, mnt, net, pid, time (boot and monotonic clocks only), user, uts; user ns needs no privilege; per-user count limits in `/proc/sys/user/*` | Namespaces isolate views, not authority; time ns does not virtualise wall clock | V-doc https://man7.org/linux/man-pages/man7/namespaces.7.html |
| Process lifecycle | `cgroup.kill`, `cgroup.freeze`, `PIDFD_AUTOKILL`/`CLONE_PIDFD_AUTOKILL`, pidfd poll for exit | Freezing a cgroup does not stop work already in flight on a device (A) | see cross-cutting table |

### Residual gaps for goal 1

| # | Gap | Label | Detail |
|---|---|---|---|
| 1a | Agent identity (agent_id, on_behalf_of, parent) bound to the process tree and surviving fork/exec/setuid | **BPF** (or US + cgroup) | Use one cgroup per agent. Store `{agent_id, on_behalf_of, caps}` in cgroup storage or task storage; propagate via BPF-LSM `task_alloc`. Both helpers present (V-src helpers.c); `task_alloc` attachable (V-src lsm_hook_defs.h line 218, not in disabled set). Escape hatch: a process with `CAP_SYS_ADMIN` can move itself; keep agents unprivileged and in a delegated cgroup subtree with `nsdelegate` (V-doc). `CLONE_INTO_CGROUP` removes the spawn-time window. |
| 1b | Identity visible to non-BPF kernel code (audit records, `/proc`, LSM blobs of other LSMs) | **PATCH** (optional) | A small in-tree LSM (`security/agentos/`, blob in `lsm_blob_sizes`) would hold the agent credential in `struct cred`. Only needed if something other than BPF must read it. LSM acceptance rule is documentation of intent in `Documentation/admin-guide/LSM/` (V-src `Documentation/security/lsm-development.rst`); for a private fork that rule is moot. |
| 1c | Sub-agents get a *subset* of parent capabilities | **US + existing kernel** | Landlock stacking is monotone narrowing (V-doc). Seccomp filters stack and cannot be removed (A for the "cannot be removed" part; doc fetched did not say it). `CLONE_NNP` plus user ns for POSIX capabilities. Anything above syscalls ("tool invocation", "IPC to other agents") has no kernel object; keep in the broker (US). |
| 1d | Token and money budgets | **US** | The kernel never sees tokens or dollars. A broker/gateway counts them and enforces by `cgroup.kill`, `cgroup.freeze`, or revoking the handle. A kernel counter would be a new cgroup controller (below) or a `misc` enum entry; neither adds enforcement the broker lacks, since the agent cannot reach the broker's memory anyway. |
| 1e | Wall-time budget | **US** | Timer in the broker plus `cgroup.kill`/`PIDFD_AUTOKILL`. `cpu.max` bounds rate, not total. |
| 1f | New cgroup controller for agent budgets (feasibility) | **PATCH**, feasible but C-only | Controllers are declared in `include/linux/cgroup_subsys.h` (16 present; build asserts `CGROUP_SUBSYS_COUNT <= 32` at `kernel/cgroup/cgroup.c` line 6527, so 16 slots free). Template: `kernel/cgroup/pids.c` (counter, max, events) or `kernel/cgroup/dmem.c`. Header says "DO NOT ADD ANY SUBSYSTEM WITHOUT EXPLICIT ACKS FROM CGROUP MAINTAINERS" (V-src) : irrelevant to a fork, fatal to upstreaming. All V-src. |
| 1g | Rust for the above | n/a | Rust is no longer experimental: patch "rust: conclude the Rust experiment" (2025-12-13) says "the experiment is done, i.e. Rust is here to stay" (V-search https://patchew.org/linux/20251213000042.23072-1-ojeda@kernel.org). But abstractions are driver-oriented. `rust/kernel/` in v7.3 has `cred.rs`, `security.rs`, `task.rs`, `pid_namespace.rs`, `drm/`, `gpu/`, `block/`, `net/`, `fs/`, `miscdevice.rs`, `workqueue.rs`; there is no `cgroup.rs`, no sched, no BPF abstraction (V-src, GitHub contents listing of `rust/kernel` at v7.3). `kernel/cgroup/` has only C (V-src listing). x86 support is `x86_64` only, "Maintained" (V-src `Documentation/rust/arch-support.rst`). A new cgroup controller or LSM in Rust means writing the missing abstractions first. A leaf driver (`miscdevice.rs`) in Rust is ready. |

---

## Goal 2: every side effect mediated and attributable

### What mainline provides

| Item | Provides | Limits | Evidence |
|---|---|---|---|
| BPF-LSM hook set | Path-level hooks receive resolved `struct path`/dentry, so no pathname TOCTOU: `path_unlink/mkdir/rmdir/mknod/truncate/symlink/link/rename/chmod/chown/chroot`, `inode_create`, `file_open`, `file_permission`, `file_ioctl`, `mmap_file`, `sb_mount`, `bprm_check_security` (spawn), `socket_create/connect/sendmsg`, `unix_stream_connect`, `unix_may_send`, `task_kill`, `ptrace_access_check`, `bpf`, `bpf_prog_load` | `socket_connect` gets a `sockaddr` (IP/port), not a hostname; no semantic view of payloads | V-src `include/linux/lsm_hook_defs.h` |
| cgroup-BPF | `BPF_CGROUP_INET4_CONNECT` and siblings (rewrite/deny connect per cgroup), `BPF_PROG_TYPE_CGROUP_DEVICE` (deny device node open) | IP-level | V-src `include/uapi/linux/bpf.h` lines 1103-1143; V-doc cgroup-v2 (Device section) |
| seccomp user-notif | Synchronous supervisor decision per syscall | Documented TOCTOU on pointer args; notification abortable; not a sandbox | V-doc seccomp_filter |
| Landlock | Self-restriction, unprivileged; scopes for abstract unix sockets and signals | Port-only net; not a process-spawn control beyond `EXECUTE` on files; `/proc/<pid>/fd` pipes/sockets and `/proc/<pid>/ns/*` not covered; fds opened before enforcement unaffected | V-doc landlock |
| io_uring | Bypasses seccomp (ops are not syscalls) [V-search https://lwn.net/Articles/1054225/]. Mitigations in 7.3: `sysctl kernel.io_uring_disabled=2` (V-doc https://docs.kernel.org/admin-guide/sysctl/kernel.html; the URL I tried first, `userspace-api/sysctl/kernel.html`, 404'd); per-ring restrictions; **task-level** restrictions via `IORING_REGISTER_RESTRICTIONS` with `ring_fd == -1`, inherited by children (V-src `io_uring/register.c` line 201 `io_register_restrictions_task`; the "inherited by children, only narrowing" wording is V-search from LWN); cBPF opcode filters (V-src `io_uring/bpf_filter.c`); LSM hooks `uring_override_creds`, `uring_sqpoll`, `uring_cmd`, `uring_allowed` (V-src lsm_hook_defs.h lines 467-470) | The BPF filter handles SQE opcodes "for now" (V-src header comment); it allows/denies, cannot rewrite | as cited |
| fanotify | Permission events `FAN_OPEN_PERM`, `FAN_ACCESS_PERM`, `FAN_OPEN_EXEC_PERM` (a userspace process holds the open until it answers); `FAN_REPORT_PIDFD` returns a pidfd of the causer | Files only; no mmap/msync/munmap events; no recursive dir watch; queue can overflow and drop events; no permission events for create/delete (inferred by the summarizer) | V-doc https://man7.org/linux/man-pages/man7/fanotify.7.html |
| audit | Syscall and LSM event records with `auid` (loginuid) and `ses`; `FAN_ENABLE_AUDIT` for fanotify decisions | Netlink control is refused outside the init user namespace (`audit_netlink_ok`: "if (current_user_ns() != &init_user_ns) return -ECONNREFUSED", comment: "This should be removed when we support non init namespaces"); no container/agent identifier in the uAPI (no `CONTAINER`/`CGROUP` constants in `audit.h`); identity is a uid, not a cgroup | V-src `kernel/audit.c` lines 1085-1088, `include/uapi/linux/audit.h` |
| `ptrace`/`pidfd_getfd`/`/proc/<pid>/mem` | Cross-process reach is mediated by `ptrace_access_check` (BPF-attachable) | | V-src lsm_hook_defs.h line 36 |

### Residual gaps for goal 2

| # | Gap | Label | Detail |
|---|---|---|---|
| 2a | Kernel-level mediation of all agent-initiated FS/exec/socket/signal/ptrace effects, deny-by-default | **BPF** | BPF-LSM programs keyed on `bpf_get_current_ancestor_cgroup_id` or task storage; per-agent attach with `BPF_LSM_CGROUP`. Combine with Landlock and seccomp allowlist for defence in depth. Completeness ("0 bypasses" in scoping.md) is not provable from the kernel docs; it is an adversarial-test result. I did not find any document claiming LSM hook coverage is complete. |
| 2b | Pathname/argument TOCTOU in the seccomp-notify path | **BPF** | Use BPF-LSM `path_*`/`file_open` hooks (resolved objects) instead of seccomp-notify for FS decisions. Keep seccomp-notify only for effects with no LSM hook. |
| 2c | Hostname-level or URL-level network policy; "which tool/API did this call hit" | **US** | Not visible to `socket_connect`. Force egress through a broker (UDS only, via Landlock `RESOLVE_UNIX` or cgroup-BPF connect deny) that terminates TLS and holds credentials. Matches scoping.md 3.7. |
| 2d | io_uring as a bypass | **No patch on >=7.0** | Disable (`io_uring_disabled=2`), or install task-level restrictions/BPF filter before exec. On 6.18 LTS the task-level restriction is not available (V-search: LWN says merged for 7.0) so a 6.18-based fork needs a backport (**PATCH**, `io_uring/register.c`, `io_uring/bpf_filter.c`). |
| 2e | Attribution: records carrying agent_id + on_behalf_of for every mediated event | **BPF** (ring buffer) or **US** (fanotify pidfd + cgroup id from `PIDFD_GET_INFO`) | Audit cannot do it without a patch. Without patching audit, ship your own event stream from BPF-LSM programs: they already hold cgroup id and task storage. If you want attribution in `auditd` itself: **PATCH** `kernel/audit.c`, `kernel/auditsc.c`, `include/uapi/linux/audit.h` (new record type or field), plus lifting the init-userns restriction at `kernel/audit.c:1087` only if agents run in user namespaces. |
| 2f | Identity on IPC messages | **US** | Unix sockets: `SO_PASSPIDFD` + `PIDFD_GET_INFO(PIDFD_INFO_CGROUPID)`; one extra syscall per peer, not per message if the pidfd is cached by pidfs id. Pipes, shared memory, and regular files carry no sender identity; route agent IPC via sockets. Adding `SO_PEERCGROUPID` would be a small **PATCH** to `net/core/sock.c` and `net/unix/af_unix.c` (A that those are the files). |
| 2g | Trusted path for human confirmation (scoping 3.3) | **US** | Broker owns a channel the agent has no fd for (separate pty/socket the agent's Landlock/cgroup-BPF deny). No kernel feature needed. I did not verify secure-attention-key behaviour. |
| 2h | Non-kernel side effects (agent holds an API key and calls a remote service directly) | **US** | Out of kernel scope entirely; solved by credential-holding broker, not by any LSM. This is the largest practical gap in "every side effect" and no kernel patch closes it. |
| 2i | Non-agent child inherits mediation ("shelling out does not escape") | **No patch** | cgroup membership and Landlock/seccomp domains are inherited across fork/exec (V-doc landlock; seccomp A). |

---

## Goal 3: inference as a shared, scheduled, accounted device

Key observation (interpretation): the scheduling granularity that matters for LLM serving (continuous batching, per-token, KV-cache paging) lives inside the inference runtime and GPU firmware. The kernel schedules at process/context granularity. A kernel patch does not reach that granularity.

### What mainline provides

| Item | Provides | Limits | Evidence |
|---|---|---|---|
| accel subsystem | `/dev/accel/accel*`, `DRIVER_COMPUTE_ACCEL`, lives inside DRM; in-tree drivers: `amdxdna`, `ethosu`, `habanalabs`, `ivpu`, `qaic`, `rocket` | Common interface only; page names no scheduling, memory-management or cgroup facility | V-doc https://docs.kernel.org/accel/introduction.html; V-src listing of `drivers/accel/` at v7.3 |
| DRM GPU scheduler (`drm_sched`) | Entities -> run queues -> hardware; priorities; credit-based flow control; dependencies via dma_fence; policy `FIFO` (default), `RR`, `FAIR` ("experimental") via module param `sched_policy` | **Zero references to cgroups** in `sched_main.c` (grep count 0). Used by amdxdna (`aie2_ctx.c`), rocket (`rocket_job.c`), ethosu (`ethosu_job.c`); no `drm_sched` match in `ivpu_job.c` | V-src `drivers/gpu/drm/scheduler/sched_main.c` lines 28-62, 87-94; V-src driver greps |
| Per-client GPU accounting | fdinfo keys `drm-engine-<name>` (ns), `drm-cycles-*`, `drm-total-cycles-*`, memory keys | Only for drivers that implement it; per open DRM fd, so only useful if each agent (or worker) holds its own fd | V-src `Documentation/gpu/drm-usage-stats.rst` |
| dmem cgroup | VRAM limits per cgroup; xe and amdgpu implement reclaim ops in 7.3 | Only drivers that register regions; nouveau wiring was still in review in 2026-04 (V-search) | see goal 1 table; V-search https://lore.gitlab.freedesktop.org/drm-ai-reviews/review-overall-20260410081322.5577-1-natalie.vock@gmx.de/raw |
| DRM weight-based GPU-time cgroup controller | **Not merged.** `drm` is absent from `cgroup_subsys.h` at v7.3 (V-src). Last revision I found: RFC v8, September 2025, `drm.weight` 1-10000, soft and best-effort (V-search https://lkml.iu.edu/hypermail/linux/kernel/2509.3/04803.html). No 2026 status found. | | |
| dma-buf / PRIME | fd-based buffer sharing between processes and devices; `dma_resv` fences | I did not verify memcg or dmem charging of dma-buf heaps. Not found. | V-doc https://docs.kernel.org/gpu/drm-mm.html (PRIME section) |
| PSI | `/proc/pressure/*` and per-cgroup `*.pressure` with poll()able triggers | Measures CPU/mem/IO stalls, not inference-queue stalls | V-src `Documentation/accounting/psi.rst` |
| sched_ext / cgroup `cpu` | CPU-side scheduling of the serving threads | Irrelevant to GPU time | above |

### Residual gaps for goal 3

| # | Gap | Label | Detail |
|---|---|---|---|
| 3a | A request queue with FIFO/RR/MLFQ per scoping 3.4, attribution to agent and human principal | **US** | A single inference-server daemon, one queue, identity from `SO_PASSPIDFD` + cgroup id (2f). Works for local models and remote APIs alike (scoping non-goal says both sit behind one interface). |
| 3b | Accounting per agent and per human principal "the way cgroups account CPU" | **US** | Aggregate in the daemon; expose as files if desired (FUSE or a socket). A real cgroup file needs the 1f patch and buys nothing the daemon lacks, because only the daemon can count tokens. |
| 3c | Fair GPU time sharing among *processes* that each hold a GPU context | **PATCH** | Revive the DRM scheduling cgroup controller: new `kernel/cgroup/drm.c`, `include/linux/cgroup_subsys.h`, and hooks in `drivers/gpu/drm/scheduler/sched_main.c` (+ `sched_entity.c`, `sched_rq.c` if present: A on file names) and each driver. Upstream status: RFC, unmerged. Only relevant if agents are separate processes on a local GPU. If one inference server owns the GPU (the usual case), skip. |
| 3d | Preempting an in-flight request (needs snapshot/restore per scoping 3.4) | **US** (runtime) / driver-specific **PATCH** | Kernel offers `cgroup.freeze` on the CPU side only (A that it does not stop GPU work already submitted). GPU-context preemption is per-driver/hardware; `drm_sched` has timeout/reset (`drm_sched_stop/start` used in `aie2_ctx.c` line 102) but not request-level preemption. Preemption at token boundaries is an inference-runtime feature. |
| 3e | Accounting remote-API inference | **US** | No kernel involvement; broker counts tokens from API responses. |
| 3f | Dedicated agent-aware accel driver | **PATCH** (new driver, Rust feasible) | If a custom NPU is ever targeted: new `drivers/accel/<name>/`. Rust has `rust/kernel/drm/`, `rust/kernel/gpu/`, and an in-tree Rust driver exists (`drivers/gpu/drm/nova/`) (V-src listings). Not relevant for GPU/remote inference today. |

---

## Goal 4: agent context as accounted memory with admission control

### What mainline provides

| Item | Provides | Limits | Evidence |
|---|---|---|---|
| memcg | `memory.high` (throttle, never OOM-kills), `memory.max` (hard), `memory.min/low` protection, `memory.reclaim` (proactive, `-EAGAIN` on short reclaim), `memory.swap.*`, `memory.zswap.*`, `memory.events`, `memory.oom.group` | Charges the *allocating task's* cgroup (A). A shared server process allocating on behalf of many agents charges itself | V-doc cgroup-v2 |
| PSI with triggers | `memory.pressure` per cgroup, `poll()`/`epoll()` triggers on cumulative stall time | Trigger registration needs a separate open() per trigger (V-src psi.rst) | V-src `Documentation/accounting/psi.rst` |
| dmem | VRAM accounting and limit, with reclaim in xe and amdgpu | Per-cgroup of the allocating process (A) | goal 1 table |
| BPF memcg read access | kfuncs: `bpf_get_mem_cgroup`, `bpf_mem_cgroup_usage`, `bpf_mem_cgroup_memory_events`, `bpf_mem_cgroup_page_state`, `bpf_mem_cgroup_vm_events`, `bpf_mem_cgroup_flush_stats` | Read-only. I found no BPF struct_ops that changes memcg limits or OOM decisions: `mm/oom_kill.c` at v7.3 has no `bpf_oom`/`struct_ops` match | V-src `mm/bpf_memcontrol.c`, `mm/oom_kill.c` |
| `shmem`/`memfd`/page cache | A context store as memfd or tmpfs file is memcg-charged (A) | | |

### Residual gaps for goal 4

| # | Gap | Label | Detail |
|---|---|---|---|
| 4a | Context measured in tokens, not bytes; "working set" of a context | **US** | Kernel has no notion of tokens. Runtime knows bytes-per-token for the KV cache and converts. The OSTEP working-set analogy (scoping 3.5) is a userspace policy. |
| 4b | Admission control when active contexts exceed capacity | **US** | Daemon admits N agents using `memory.current`, `memory.pressure` triggers, `dmem.current`, and per-agent `memory.max`/`dmem.max`. Reaction: `cgroup.freeze` the lowest-priority agent, `memory.reclaim`, or evict KV to disk. All present. |
| 4c | Charging memory allocated by a shared server to the *agent's* cgroup | **US** (worker per agent or accounting in daemon) / **PATCH** (remote charge uAPI) | No uAPI found to charge an allocation to a different cgroup. Worker-per-agent processes inside the agent's cgroup avoids it. A remote-charge syscall/ioctl would touch `mm/memcontrol.c`. I did not search for existing precedent beyond the BPF kfunc file. |
| 4d | Eviction with exact reload (paging analogue) | **US** | File store plus memcg limits; the kernel's own swap is page-granular and agent-oblivious. |
| 4e | Memory-pressure *policy* in the kernel (e.g. choose which agent's context is reclaimed first) | **PATCH** (memcg) or **US** | memcg `memory.low/min` protection plus userspace oomd-style daemon covers it. A BPF hook for memcg policy is not in 7.3 (V-src absence above). Would be **PATCH** `mm/memcontrol.c`, `mm/oom_kill.c`. |

---

## Where a fork buys something real, ranked

1. Backports for a LTS base: if the fork tracks 6.18 LTS rather than 7.x, io_uring task-level restrictions and BPF filters (merged for 7.0 per V-search) and recent sched_ext sub-schedulers must be carried. Pick the base deliberately (decision, not a gap).
2. Audit with agent identity (2e): only if auditd output specifically is a requirement; BPF event stream otherwise.
3. DRM scheduling cgroup controller (3c): only for multi-process local GPU sharing.
4. In-kernel agent credential / budget controller (1b, 1f): hardening and uniformity, not capability.
5. `SO_PEERCGROUPID` (2f): ergonomics.

Everything else is userspace or BPF on stock 7.3. Counter-argument to check before committing to a fork: scoping.md section 2 lists "mediation overhead" and "POSIX layer cost" as TBD with no targets. A stock-kernel prototype (cgroup per agent + BPF-LSM + Landlock + seccomp + broker) yields those baselines at low cost; a fork should be justified by a measured miss, not an assumed one. That is also the cheapest test of whether the fork-versus-microkernel decision (ADR 0002) is reversible.

## Not found / not verified

- No source showing LSM hook coverage is complete; "0 bypasses" is an empirical claim.
- Mediation overhead numbers for BPF-LSM or Landlock on 7.3: not looked up.
- Any 2026 status of the DRM cgroup controller beyond RFC v8 (Sept 2025).
- Whether dma-buf heaps are memcg- or dmem-charged.
- Whether `memory.high` etc. charge dma-buf/GPU system memory (A).
- Seccomp filter immutability, memcg charge-to-allocator rule, pid-ns kill-on-init-exit, `cgroup.freeze` vs in-flight device work, secure attention key: all A.
- LWN article 1054225 (io_uring task restrictions) is subscriber-only; I relied on the search summary and read the code in `io_uring/register.c` instead. The exact release in which it landed (7.0) comes from the search summary.
- docs.kernel.org pages for drm_sched body and cgroup-v2 were truncated by the fetcher; I used source files instead for those.
- The `kernel.org/finger_banner` fetch and `kernel.org` fetch agree: mainline 7.3.
- `git.kernel.org` returned 403; all source reads came from the GitHub mirror at tag `v7.3`, which I assume is identical to the kernel.org tag.
