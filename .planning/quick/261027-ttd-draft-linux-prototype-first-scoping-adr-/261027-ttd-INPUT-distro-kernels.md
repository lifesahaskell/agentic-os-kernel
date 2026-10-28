# Distro kernel feature floor (2026-10-27)

Scope: which kernel features a Linux-prototype-first design can assume on mainstream distro kernels.
Evidence gathered 2026-10-27. Every cell is VERIFIED (primary source read this session) or
TO-VERIFY (reason given). Nothing is guessed. "Present" means in the source tree of that
distro kernel; "enabled" means the shipped config turns it on by default.

Source URL shorthand (all read this session):

- `KO/<tag>/<path>` = `https://git.kernel.org/pub/scm/linux/kernel/git/{torvalds|stable}/linux.git/plain/<path>?h=<tag>`
- `UBU/<tag>/<path>` = `https://git.launchpad.net/~ubuntu-kernel/ubuntu/+source/linux/+git/noble/plain/<path>?h=<tag>`
- `CS10/<path>` = `https://gitlab.com/redhat/centos-stream/src/kernel/centos-stream-10/-/raw/kernel-6.12.0-211.el10/<path>`
- Debian config: `http://security.debian.org/debian-security/pool/updates/main/l/linux-signed-amd64/linux-image-6.12.113+deb13-amd64_6.12.113-1_amd64.deb` (`./boot/config-6.12.113+deb13-amd64`)
- Fedora config: `https://src.fedoraproject.org/rpms/kernel/raw/f44/f/kernel-x86_64-fedora.config`
- Alma config: `https://repo.almalinux.org/almalinux/10/BaseOS/x86_64/os/Packages/kernel-core-6.12.0-211.68.1.el10_2.x86_64.rpm` (`/lib/modules/.../config`); Rocky: same file name under `https://dl.rockylinux.org/pub/rocky/10/BaseOS/x86_64/os/Packages/k/` (differs from Alma only in BE2NET, MLX4, FIPS name)
- Arch config: `https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/7.2.11.arch1-1/config.x86_64`; version from `https://archlinux.org/packages/core/x86_64/linux/json/`

## 1. Shipped kernels

| Distro | Kernel | Basis |
|---|---|---|
| Ubuntu 24.04 GA | 6.8.0-147.147 (Updates and Security) | VERIFIED, Launchpad publishing history; source tag `Ubuntu-6.8.0-147.147`. 6.8.0-149 is in Proposed only. |
| Ubuntu 24.04 HWE | linux-hwe-7.0: 7.0.0-42.42~24.04.1 in Updates (base 7.0.14); -38 was in Security | VERIFIED, Launchpad; tag `Ubuntu-hwe-7.0-7.0.0-42.42_24.04.1` |
| Debian 13 (trixie) | 6.12.113-1 (trixie-security) | VERIFIED, packages.debian.org plus the deb itself. Trixie-backports kernels excluded (not default). |
| Fedora current stable (44) | 7.2.13-200.fc44 (stable 2026-10-28); 7.2.12 on 2026-10-27 | VERIFIED, Bodhi; source read at stable `v7.2.13` and the `f44` dist-git branch. F43 also supported, F45 in Beta. |
| RHEL 10.2 / Alma 10.2 / Rocky 10.2 | 6.12.0-211.68.1.el10_2 (heavily backported "6.12") | VERIFIED, rpm headers and configs. Source checks used base tag `kernel-6.12.0-211.el10`; the z-stream source was not read. |
| Arch | 7.2.11.arch1-1 | VERIFIED, archlinux.org package JSON |

Mainline reference: 7.3 released 2026-10-18; stable 7.2.13; longterm 6.18, 6.12, 6.6, 6.1, 5.15, 5.10.

## 2. First mainline version per feature

All "first" versions come from adjacent-tag presence checks (absent in N-1, present in N) on the
named file at `KO/<tag>/<path>`.

| Feature | First mainline | Evidence |
|---|---|---|
| CLONE_INTO_CGROUP | 5.7 | VERIFIED. `include/uapi/linux/sched.h`: absent v5.6, present v5.7. |
| CLONE_PIDFD_AUTOKILL | 7.1 | VERIFIED. Header absent v7.0, present v7.1. Commit c8134b5f13ae959de2b3c8cc278e2602b0857345 "pidfd: add CLONE_PIDFD_AUTOKILL"; merge a2900f5aa4e7724a2bf56ddb1a4f816d4fcc0598; pull tag vfs-7.1-rc1.pidfs. |
| CLONE_NNP | 7.1 | VERIFIED. Same series as AUTOKILL. Implemented in `kernel/fork.c` at v7.2.13 (`if (clone_flags & CLONE_NNP)`). Commit id not located. |
| PIDFD_GET_INFO ioctl | 6.13 | VERIFIED. Commit cdda1f26e74bac732eca537a69f19f6a37b641be "pidfd: add ioctl to retrieve pid info"; merge 909d3b571e5a77aef0949818de1efda129dcddbd (vfs-6.13.pidfs). |
| PIDFD_INFO_CGROUPID (cgroup id in that ioctl) | 6.13 | VERIFIED. In v6.13 `include/uapi/linux/pidfd.h`, absent in v6.12. |
| BPF_LSM_CGROUP | 6.0 | VERIFIED. `include/uapi/linux/bpf.h`: absent v5.19, present v6.0. |
| BPF LSM | 5.7 | VERIFIED. `kernel/bpf/bpf_lsm.c` first at v5.7. |
| Landlock | 5.13 | VERIFIED. `security/landlock/setup.c` first at v5.13. |
| Landlock ABI by kernel | 6.6=3, 6.7=4, 6.10=5, 6.12=6, 6.15=7, 7.0=8, 7.1=9, 7.2=10, 7.3=11 | VERIFIED. `ASSERT_EQ` in `TEST(abi_version)`, `tools/testing/selftests/landlock/base_test.c` at each tag. |
| Landlock network rules | ABI 4 (TCP, kernel 6.7); UDP at ABI 10 (7.2) | VERIFIED. `Documentation/userspace-api/landlock.rst` at v7.3: "Network rules (since ABI v4 for TCP and v10 for UDP)". |
| seccomp user-notif | 5.0 | VERIFIED. `SECCOMP_RET_USER_NOTIF` absent v4.20, present v5.0. |
| io_uring restrictions (per-ring) | 5.10 | VERIFIED. `IORING_REGISTER_RESTRICTIONS` absent v5.9, present v5.10. |
| io_uring per-task restrictions | 7.0 | VERIFIED. `struct io_uring_task_restriction` present v7.0. Commit ed82f35b926b2e505c14b7006473614b8f58b4f4 "io_uring: allow registration of per-task restrictions". |
| io_uring BPF opcode filter | 7.0 | VERIFIED. `IORING_REGISTER_BPF_FILTER` absent v6.19, present v7.0. Commit d42eb05e60fea31de49897d63a1d73f933303bd4. |
| memcg memory.high / memory.max | 4.5 (cgroup v2 documented and non-experimental) | VERIFIED. `Documentation/cgroup-v2.txt` absent v4.4, present v4.5 and lists both. (`memory.high` code existed earlier in mm/memcontrol.c.) |
| memcg memory.reclaim | 5.19 | VERIFIED. Commit 94968384dde15d48263bfc59d280cd71b1259d8c "memcg: introduce per-memcg reclaim interface". |
| PSI | 4.20 (`kernel/sched/psi.c`) | VERIFIED. |
| PSI triggers (poll) | 5.2 | VERIFIED. `psi_trigger_create` present; `Documentation/accounting/psi.txt` documents triggers at v5.2, not v5.1. |
| cgroup dmem controller | 6.14 | VERIFIED. `kernel/cgroup/dmem.c` first at v6.14. |
| sched_ext | 6.12 | VERIFIED. `kernel/sched/ext.c` first at v6.12. File is not at that path in v7.3 or 7.2 stable (refactored); shipped CONFIG values below are the evidence for current kernels. |
| Unprivileged user namespaces | 3.8 (initial implementation) | VERIFIED. man7 user_namespaces(7). Kernel default permits creation; `max_user_namespaces` defaults to max_threads/2. |
| cgroup v2 default | n/a (systemd decision) | VERIFIED. systemd NEWS `https://raw.githubusercontent.com/systemd/systemd/v258/NEWS`: v257 treats cgroup v1 as obsolete and ignores it by default; v258 removed it. |

## 3. Per-distro matrix

Legend: P = present in source, E = enabled by default in shipped config. Config values are from the
config files listed above. Every cell below is VERIFIED unless the cell says TO-VERIFY.

### 3a. Source presence of kernel APIs (no config switch)

| Feature | Ubuntu 6.8 GA | Ubuntu HWE 7.0 | Debian 13 (6.12.113) | Fedora 44 (7.2.13) | RHEL10/Alma/Rocky (6.12.0-211) | Arch (7.2.11) |
|---|---|---|---|---|---|---|
| CLONE_INTO_CGROUP | P | P | P | P | P (`kernel/fork.c` 2 hits) | P |
| CLONE_PIDFD_AUTOKILL | absent | absent | absent | P | absent | P |
| CLONE_NNP | absent | absent | absent | P | absent | P |
| PIDFD_GET_INFO | absent | P | absent | P | P (backport, `fs/pidfs.c` lines 276-401) | P |
| PIDFD_INFO_CGROUPID | absent | P | absent | P | P (backport) | P |
| io_uring per-task restrictions | absent | P | absent | P | absent | P |
| io_uring BPF opcode filter | absent | P | absent | P | absent | P |
| io_uring per-ring restrictions | P | P | P | P | P | P |
| seccomp user-notif | P | P | P | P | P | P |
| memcg memory.reclaim | P | P | P | P | P | P |
| PSI trigger code | P | P | P | P | P | P |
| cgroup dmem source | absent | P | absent | P | P (backport) | P |

Source: Ubuntu `UBU/Ubuntu-6.8.0-147.147/...` and `UBU/Ubuntu-hwe-7.0-7.0.0-38.38_24.04.4/...` (also -42 for pidfs.c); Debian `KO/v6.12.113/...` (stable); Fedora `KO/v7.2.13/...`; RHEL `CS10/...`; Arch `KO/v7.2.11/...`. Checks were file or symbol presence: `include/uapi/linux/sched.h`, `include/uapi/linux/pidfd.h`, `include/uapi/linux/io_uring.h`, `fs/pidfs.c`, `kernel/cgroup/dmem.c`, `kernel/fork.c`.

### 3b. Config-dependent features

| Feature | Ubuntu 6.8 GA | Ubuntu HWE 7.0 | Debian 13 | Fedora 44 | RHEL10/Alma/Rocky | Arch |
|---|---|---|---|---|---|---|
| CONFIG_BPF_LSM | y | y | y | y | y | y |
| CONFIG_LSM (default lsm=) | `landlock,lockdown,yama,integrity,apparmor` | -38: same as GA. -39, -42: `landlock,lockdown,yama,integrity,apparmor,bpf` | `landlock,lockdown,yama,loadpin,safesetid,integrity,apparmor,selinux,smack,tomoyo,bpf,ipe` | `lockdown,yama,integrity,selinux,bpf,landlock,ipe` | `lockdown,yama,integrity,selinux,bpf,landlock` | `landlock,lockdown,yama,integrity,bpf` |
| "bpf" in default lsm= | NO | NO in -38; YES in -39 and -42 (changelog "append bpf to CONFIG_LSM (LP: #2166235)") | yes | yes | yes | yes |
| BPF_LSM_CGROUP usable (needs BPF_LSM, CGROUP_BPF=y, bpf in lsm) | P, needs lsm= override (CGROUP_BPF=y, BPF_LSM=y) | yes on -42 (CGROUP_BPF=y); needs override on -38 | yes (BPF_LSM_CGROUP code exists since 6.0; CGROUP_BPF not separately re-checked on Debian beyond config grep of BPF_LSM) | yes | yes | yes |
| CONFIG_SECURITY_LANDLOCK | y | y | y | y | y | y |
| Landlock in default lsm= | yes | yes | yes | yes | yes | yes |
| Landlock ABI | 4 (TCP net rules yes) | 8 | 6 | 10 | 6 (backport level) | 10 |
| CONFIG_SECCOMP / SECCOMP_FILTER | y / y | y / y | y / y | y / on (def_bool y, not listed) | y / y | y / y |
| CONFIG_IO_URING | y | y | y | y | y | y |
| io_uring_disabled sysctl default | 0 | 0 | 0 (stable v6.12.113 source) | 0 | 2 (disabled) (`io_uring/io_uring.c` line 158, tag kernel-6.12.0-211.el10) | 0 |
| CONFIG_MEMCG | y | y | y | y | y | y |
| CONFIG_PSI | y | y | y | y | y | y |
| PSI default | on (PSI_DEFAULT_DISABLED=n) | on | on (not set) | on (not set) | OFF: PSI_DEFAULT_DISABLED=y, needs `psi=1` | on (not set) |
| CONFIG_CGROUP_DMEM | absent (not in tree) | y | absent | y | y | y |
| CONFIG_SCHED_CLASS_EXT | absent (not in tree) | y | not set (code present) | y | y | y |
| CONFIG_USER_NS | y | y | y | y | y | y |
| Unprivileged userns, kernel default | `unprivileged_userns_clone=1`; AppArmor restriction layer on top (below) | same | allowed (Debian patch default 1, changelog "change the default to enabled") | allowed (mainline default) | allowed (mainline default) | allowed (mainline default) |
| cgroup v2 default (systemd) | yes (noble systemd 255.4 `debian/rules` has `-Ddefault-hierarchy=unified`) | yes | yes (systemd 257.x) | yes (systemd 259.10) | yes (systemd 257) | yes (systemd 262.1) |

Config sources: Ubuntu `UBU/Ubuntu-6.8.0-147.147/debian.master/config/annotations`, `UBU/Ubuntu-hwe-7.0-7.0.0-38.38_24.04.4/...annotations`, and `...-7.0.0-42.42_24.04.1/...annotations` (changelog line in `debian.master/changelog` at the -42 tag). Others from the config files in the URL shorthand above.

Ubuntu userns detail. The 24.04 release notes (`https://documentation.ubuntu.com/release-notes/24.04/`) state that "in combination with the apparmor package, the Ubuntu kernel now restricts the use of unprivileged user namespaces", disabled with `kernel.apparmor_restrict_unprivileged_userns=0`. VERIFIED that the restriction exists and is the default for 24.04 per the release notes. Kernel build option `SECURITY_APPARMOR_RESTRICT_USERNS=n` and the Ubuntu patch default `aa_unprivileged_userns_restricted = IS_ENABLED(...)` means the sysctl is switched on from userspace, not the kernel; the exact sysctl.d file in the apparmor package was not read (TO-VERIFY). What an unconfined process can do under the restriction (creates a userns but loses capabilities) came from a search snippet only: TO-VERIFY.

## 4. Floor

Oldest distro kernel: Ubuntu 24.04 GA 6.8. Floor = highest kernel version that every listed distro
still meets, per feature. Most rows are met by all distros at their own kernel via mainline
version alone; the interesting cases are the ones where some distro falls short.

No pressure (all six distros present and enabled, floor is the 6.8 / 6.12 minimum):

- CLONE_INTO_CGROUP (5.7), BPF_LSM_CGROUP code (6.0), BPF LSM build (5.7), Landlock (5.13), seccomp user-notif (5.0), per-ring io_uring restrictions (5.10), memcg high/max (4.5) and reclaim (5.19), PSI triggers (5.2), cgroup v2 default, userns.
- Landlock: floor is ABI 4 (Ubuntu GA 6.8). TCP bind/connect rules available everywhere; UDP rules (ABI 10) only on Fedora and Arch.

Features that force the floor above the oldest distro kernel (6.8), with who fails:

| Feature | Needs | Missing on | Notes |
|---|---|---|---|
| PIDFD_GET_INFO with cgroup id | 6.13 | Ubuntu GA 6.8, Debian 13 (6.12.113) | RHEL has it by backport. HWE 7.0, Fedora, Arch have it. |
| sched_ext | 6.12 and CONFIG on | Ubuntu GA 6.8 (absent), Debian 13 (code present, config off) | RHEL, Fedora, Arch, HWE have it on. |
| cgroup dmem | 6.14 | Ubuntu GA, Debian 13 | RHEL has it by backport. |
| io_uring per-task restrictions and BPF opcode filter | 7.0 | Ubuntu GA, Debian 13, RHEL 10 | HWE 7.0, Fedora, Arch only. RHEL additionally defaults `io_uring_disabled=2`, which nullifies the whole io_uring row until the sysctl is changed. |
| CLONE_PIDFD_AUTOKILL, CLONE_NNP | 7.1 | Ubuntu GA, Ubuntu HWE 7.0, Debian, RHEL | Fedora 44 and Arch only (both 7.2). No LTS distro has them. |
| bpf in default lsm= | CONFIG_LSM contains bpf | Ubuntu GA 6.8 (all builds), Ubuntu HWE -38 | HWE -39 and -42 fixed it. GA needs `lsm=` cmdline restating the full list plus bpf. |
| PSI | psi=1 | RHEL 10 | Tunable by bootloader arg; off by default. |

Practical floors by kernel number:

- Every row available with no cmdline or sysctl changes on Debian 13 / RHEL 10 / Ubuntu GA: 6.8 for the 5.x and 6.0 features.
- Adding sched_ext, dmem and PIDFD_GET_INFO: 6.14 (kernel number); on the distro set, only RHEL (backport), Ubuntu HWE, Fedora and Arch qualify. Debian 13 and Ubuntu GA do not.
- Adding io_uring task restrictions: 7.0. Adding CLONE_NNP and CLONE_PIDFD_AUTOKILL: 7.1. These two are Fedora and Arch only today.
- No single distro kernel from the set other than Fedora 44 and Arch meets every row. Ubuntu HWE 7.0 (build -42 or later) misses only AUTOKILL and NNP.

## 5. TO-VERIFY list

- Ubuntu userns: apparmor package sysctl.d default read directly; capability-stripping behaviour for unconfined apps (search snippet only).
- RHEL z-stream 211.68.1 source: Section 3a and the `io_uring_disabled` default were read at base tag `kernel-6.12.0-211.el10`, not at the shipped z-stream. Configs are from the shipped rpm.
- Debian: CGROUP_BPF value was not separately recorded; BPF_LSM_CGROUP "yes" rests on BPF_LSM=y and bpf in CONFIG_LSM. Recheck CONFIG_CGROUP_BPF before relying on it.
- Bootloader and cmdline defaults for every distro (a cmdline `lsm=` overrides CONFIG_LSM). Not examined.
- `sysctl.d` userspace overrides for `kernel.io_uring_disabled`, `user.max_user_namespaces` and userns restriction on every distro. Not examined; only kernel defaults stated.
- Arch downstream patch set (`archlinux/linux` branch) not inspected for userns or io_uring changes.
- Debian trixie-backports kernels and Ubuntu 6.8.0-149 (Proposed) not in scope; -149 lacks bpf in lsm per annotations read.
- Mainline commit id for CLONE_NNP not located (series merge a2900f5 verified).
