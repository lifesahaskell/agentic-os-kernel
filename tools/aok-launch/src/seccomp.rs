//! Seccomp layer: a hand-assembled allow-list filter over the x86-64 syscall
//! table (D-01 to D-05), installed pre-exec after `landlock::apply` and before
//! `sys::set_no_new_privs`.
//!
//! # Allowlist record
//!
//! `09-06` writes `09-ALLOWLIST.md` from this record; `09-03-SUMMARY.md`
//! carries a copy.
//!
//! - **Source.** `arch/x86/entry/syscalls/syscall_64.tbl` at the Linux v6.18
//!   tag (fetched once during development; the file is GPL and is not
//!   vendored, only the numbers and names below are data). Rows with ABI
//!   `common` and `64` are kept; `x32` rows are not.
//! - **Table.** 383 entries, numbers 0 to 336 and 424 to 469. The maximum is
//!   469 (`file_setattr`). The one gap is 337 to 423 (87 numbers).
//! - **Coalesced allow ranges** (inclusive, the table minus the deny set):
//!   0-100, 102-154, 156-164, 167-174, 177-245, 247-297, 299-312, 314-319,
//!   322, 324-336, 424, 434-441, 443-466, 468-469. That is 14 ranges and 360
//!   allowed numbers.
//! - **Deny set** (23 numbers, `EPERM`, D-01 known-denied):
//!   - `ptrace` 101: cross-process memory and register access.
//!   - `perf_event_open` 298: hardware counters, a side-channel and a large
//!     kernel attack surface.
//!   - `bpf` 321: loads programs into the kernel.
//!   - `userfaultfd` 323: lets user space stall the kernel at a chosen page
//!     fault, an exploit primitive.
//!   - Mount family 165 `mount`, 166 `umount2`, 155 `pivot_root`, 428
//!     `open_tree`, 429 `move_mount`, 430 `fsopen`, 431 `fsconfig`, 432
//!     `fsmount`, 433 `fspick`, 442 `mount_setattr`, 467 `open_tree_attr`:
//!     change mount topology, which Landlock path rules do not model.
//!   - `kexec_load` 246, `kexec_file_load` 320: replace the running kernel.
//!   - `init_module` 175, `delete_module` 176, `finit_module` 313: load or
//!     unload kernel code.
//!   - io_uring 425 `io_uring_setup`, 426 `io_uring_enter`, 427
//!     `io_uring_register`: operations submitted through a ring do not pass
//!     through this filter as syscalls.
//! - **`clone3` (435) is allowed** (D-02): the launcher and glibc spawn with
//!   it, and its flags are in a user struct this filter never reads (D-04).
//! - **`statmount` (457) and `listmount` (458) stay allowed** (discretion,
//!   recorded): they only read the mount table visible to the caller's own
//!   mount namespace; the topology-changing calls above are the denied ones.
//! - **x32.** A number with bit `0x40000000` set returns `EPERM` (D-05). An
//!   x32 call arrives under `AUDIT_ARCH_X86_64`, so the arch check passes and
//!   this bit test is what catches it.
//! - **Other architectures.** `arch != AUDIT_ARCH_X86_64` (the i386 compat
//!   path through `int 0x80`, and any other) returns `EPERM`, not a kill
//!   (D-05). The filter never returns `KILL_PROCESS` or `KILL_THREAD`.
//! - **Gap and above-table numbers** (337-423 and anything over 469, not x32)
//!   return `ENOSYS` (D-01), exactly what an older kernel returns, so a libc
//!   probe for a newer syscall degrades the way it would on that kernel.
//! - **What the filter reads.** Only `seccomp_data.arch` (offset 4) and
//!   `seccomp_data.nr` (offset 0); never `args` and never `instruction_pointer`
//!   (D-04), so no pointer argument is dereferenced (LNCH-06). That the kernel
//!   then serves the filter from its per-syscall action cache (Linux 5.11+) is
//!   ASSUMED, not measured; no test here asserts it.
//!
//! # A1: do libc probes degrade on `ENOSYS`?
//!
//! The question: does any syscall the launcher or a libc probes at run time
//! branch on an errno other than `ENOSYS`, so that this filter's `EPERM` would
//! break it. Result:
//!
//! - The launcher's own probes are `clone3` (falls back to `fork` on
//!   `ENOSYS`), `ioctl(PIDFD_GET_INFO)` (falls back on `ENOTTY`) and
//!   `landlock_create_ruleset` with the version flag (`ENOSYS` and
//!   `EOPNOTSUPP` both mean ABI 0). All three numbers are in the allow
//!   ranges, so the filter never changes their result. MEASURED: every
//!   launch in the crate suite runs through this filter and the spawn path
//!   works.
//! - glibc (the host libc, MEASURED): `strace -f` run over the launcher
//!   with `sh -c true`, `python3 -I -B -c pass`, `cat /proc/self/status` and
//!   `sleep` as the command shows 71 instructions installed and then not one
//!   syscall in any of the four processes returning `EPERM` or `ENOSYS`
//!   after the install. So glibc's start-up and `posix_spawn` probes
//!   (`rseq`, `clone3`, `close_range`, `statx`, `faccessat2`, `getrandom`)
//!   all land in allow ranges. (Landlock `EACCES` on the shell's `PATH`
//!   search is not the filter.) None of the denied families is a libc
//!   start-up probe.
//! - musl 1.2.6: NOT MEASURED, no musl toolchain is installed on this host.
//!   Reasoned only: the probes musl makes are also ordinary syscalls inside
//!   the allow ranges, and a probe of a number above 469 gets the same
//!   `ENOSYS` an older kernel gives. This stays an open verification item for
//!   the first musl-linked agent (phase B3).
//! - No probe was found that branches on a non-`ENOSYS` errno, so no
//!   allow-list decision was needed. The denied families return `EPERM`,
//!   which is also what `io_uring_disabled=2` and an unprivileged `bpf` call
//!   return on a stock kernel, so software that uses them already handles it.
//!
//! This module is safe Rust. It assembles the program by hand because
//! `seccompiler`'s high-level constructor emits `SECCOMP_RET_KILL_PROCESS` on
//! an arch mismatch, which would kill the i386 compat path (D-05, LNCH-06).

use seccompiler::sock_filter;

use crate::landlock::LayerError;

const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JEQ_K: u16 = 0x15;
const BPF_JGE_K: u16 = 0x35;
const BPF_JSET_K: u16 = 0x45;
const BPF_RET_K: u16 = 0x06;

/// `seccomp_data` offsets: `nr` is first, `arch` second.
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;

/// `EM_X86_64 | __AUDIT_ARCH_64BIT | __AUDIT_ARCH_LE`.
pub const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
/// The x32 ABI marker bit in the syscall number.
pub const X32_BIT: u32 = 0x4000_0000;

pub const RET_ALLOW: u32 = 0x7fff_0000;
pub const RET_ERRNO: u32 = 0x0005_0000;
/// Mask of the action bits of a return value.
pub const RET_ACTION_MASK: u32 = 0xffff_0000;

/// The v6.18 x86-64 table, `common` and `64` ABIs: (number, name).
/// Data derived from `syscall_64.tbl`; see the module doc.
#[rustfmt::skip]
pub const TABLE: &[(u32, &str)] = &[
    (0, "read"), (1, "write"), (2, "open"), (3, "close"),
    (4, "stat"), (5, "fstat"), (6, "lstat"), (7, "poll"),
    (8, "lseek"), (9, "mmap"), (10, "mprotect"), (11, "munmap"),
    (12, "brk"), (13, "rt_sigaction"), (14, "rt_sigprocmask"), (15, "rt_sigreturn"),
    (16, "ioctl"), (17, "pread64"), (18, "pwrite64"), (19, "readv"),
    (20, "writev"), (21, "access"), (22, "pipe"), (23, "select"),
    (24, "sched_yield"), (25, "mremap"), (26, "msync"), (27, "mincore"),
    (28, "madvise"), (29, "shmget"), (30, "shmat"), (31, "shmctl"),
    (32, "dup"), (33, "dup2"), (34, "pause"), (35, "nanosleep"),
    (36, "getitimer"), (37, "alarm"), (38, "setitimer"), (39, "getpid"),
    (40, "sendfile"), (41, "socket"), (42, "connect"), (43, "accept"),
    (44, "sendto"), (45, "recvfrom"), (46, "sendmsg"), (47, "recvmsg"),
    (48, "shutdown"), (49, "bind"), (50, "listen"), (51, "getsockname"),
    (52, "getpeername"), (53, "socketpair"), (54, "setsockopt"), (55, "getsockopt"),
    (56, "clone"), (57, "fork"), (58, "vfork"), (59, "execve"),
    (60, "exit"), (61, "wait4"), (62, "kill"), (63, "uname"),
    (64, "semget"), (65, "semop"), (66, "semctl"), (67, "shmdt"),
    (68, "msgget"), (69, "msgsnd"), (70, "msgrcv"), (71, "msgctl"),
    (72, "fcntl"), (73, "flock"), (74, "fsync"), (75, "fdatasync"),
    (76, "truncate"), (77, "ftruncate"), (78, "getdents"), (79, "getcwd"),
    (80, "chdir"), (81, "fchdir"), (82, "rename"), (83, "mkdir"),
    (84, "rmdir"), (85, "creat"), (86, "link"), (87, "unlink"),
    (88, "symlink"), (89, "readlink"), (90, "chmod"), (91, "fchmod"),
    (92, "chown"), (93, "fchown"), (94, "lchown"), (95, "umask"),
    (96, "gettimeofday"), (97, "getrlimit"), (98, "getrusage"), (99, "sysinfo"),
    (100, "times"), (101, "ptrace"), (102, "getuid"), (103, "syslog"),
    (104, "getgid"), (105, "setuid"), (106, "setgid"), (107, "geteuid"),
    (108, "getegid"), (109, "setpgid"), (110, "getppid"), (111, "getpgrp"),
    (112, "setsid"), (113, "setreuid"), (114, "setregid"), (115, "getgroups"),
    (116, "setgroups"), (117, "setresuid"), (118, "getresuid"), (119, "setresgid"),
    (120, "getresgid"), (121, "getpgid"), (122, "setfsuid"), (123, "setfsgid"),
    (124, "getsid"), (125, "capget"), (126, "capset"), (127, "rt_sigpending"),
    (128, "rt_sigtimedwait"), (129, "rt_sigqueueinfo"), (130, "rt_sigsuspend"), (131, "sigaltstack"),
    (132, "utime"), (133, "mknod"), (134, "uselib"), (135, "personality"),
    (136, "ustat"), (137, "statfs"), (138, "fstatfs"), (139, "sysfs"),
    (140, "getpriority"), (141, "setpriority"), (142, "sched_setparam"), (143, "sched_getparam"),
    (144, "sched_setscheduler"), (145, "sched_getscheduler"), (146, "sched_get_priority_max"), (147, "sched_get_priority_min"),
    (148, "sched_rr_get_interval"), (149, "mlock"), (150, "munlock"), (151, "mlockall"),
    (152, "munlockall"), (153, "vhangup"), (154, "modify_ldt"), (155, "pivot_root"),
    (156, "_sysctl"), (157, "prctl"), (158, "arch_prctl"), (159, "adjtimex"),
    (160, "setrlimit"), (161, "chroot"), (162, "sync"), (163, "acct"),
    (164, "settimeofday"), (165, "mount"), (166, "umount2"), (167, "swapon"),
    (168, "swapoff"), (169, "reboot"), (170, "sethostname"), (171, "setdomainname"),
    (172, "iopl"), (173, "ioperm"), (174, "create_module"), (175, "init_module"),
    (176, "delete_module"), (177, "get_kernel_syms"), (178, "query_module"), (179, "quotactl"),
    (180, "nfsservctl"), (181, "getpmsg"), (182, "putpmsg"), (183, "afs_syscall"),
    (184, "tuxcall"), (185, "security"), (186, "gettid"), (187, "readahead"),
    (188, "setxattr"), (189, "lsetxattr"), (190, "fsetxattr"), (191, "getxattr"),
    (192, "lgetxattr"), (193, "fgetxattr"), (194, "listxattr"), (195, "llistxattr"),
    (196, "flistxattr"), (197, "removexattr"), (198, "lremovexattr"), (199, "fremovexattr"),
    (200, "tkill"), (201, "time"), (202, "futex"), (203, "sched_setaffinity"),
    (204, "sched_getaffinity"), (205, "set_thread_area"), (206, "io_setup"), (207, "io_destroy"),
    (208, "io_getevents"), (209, "io_submit"), (210, "io_cancel"), (211, "get_thread_area"),
    (212, "lookup_dcookie"), (213, "epoll_create"), (214, "epoll_ctl_old"), (215, "epoll_wait_old"),
    (216, "remap_file_pages"), (217, "getdents64"), (218, "set_tid_address"), (219, "restart_syscall"),
    (220, "semtimedop"), (221, "fadvise64"), (222, "timer_create"), (223, "timer_settime"),
    (224, "timer_gettime"), (225, "timer_getoverrun"), (226, "timer_delete"), (227, "clock_settime"),
    (228, "clock_gettime"), (229, "clock_getres"), (230, "clock_nanosleep"), (231, "exit_group"),
    (232, "epoll_wait"), (233, "epoll_ctl"), (234, "tgkill"), (235, "utimes"),
    (236, "vserver"), (237, "mbind"), (238, "set_mempolicy"), (239, "get_mempolicy"),
    (240, "mq_open"), (241, "mq_unlink"), (242, "mq_timedsend"), (243, "mq_timedreceive"),
    (244, "mq_notify"), (245, "mq_getsetattr"), (246, "kexec_load"), (247, "waitid"),
    (248, "add_key"), (249, "request_key"), (250, "keyctl"), (251, "ioprio_set"),
    (252, "ioprio_get"), (253, "inotify_init"), (254, "inotify_add_watch"), (255, "inotify_rm_watch"),
    (256, "migrate_pages"), (257, "openat"), (258, "mkdirat"), (259, "mknodat"),
    (260, "fchownat"), (261, "futimesat"), (262, "newfstatat"), (263, "unlinkat"),
    (264, "renameat"), (265, "linkat"), (266, "symlinkat"), (267, "readlinkat"),
    (268, "fchmodat"), (269, "faccessat"), (270, "pselect6"), (271, "ppoll"),
    (272, "unshare"), (273, "set_robust_list"), (274, "get_robust_list"), (275, "splice"),
    (276, "tee"), (277, "sync_file_range"), (278, "vmsplice"), (279, "move_pages"),
    (280, "utimensat"), (281, "epoll_pwait"), (282, "signalfd"), (283, "timerfd_create"),
    (284, "eventfd"), (285, "fallocate"), (286, "timerfd_settime"), (287, "timerfd_gettime"),
    (288, "accept4"), (289, "signalfd4"), (290, "eventfd2"), (291, "epoll_create1"),
    (292, "dup3"), (293, "pipe2"), (294, "inotify_init1"), (295, "preadv"),
    (296, "pwritev"), (297, "rt_tgsigqueueinfo"), (298, "perf_event_open"), (299, "recvmmsg"),
    (300, "fanotify_init"), (301, "fanotify_mark"), (302, "prlimit64"), (303, "name_to_handle_at"),
    (304, "open_by_handle_at"), (305, "clock_adjtime"), (306, "syncfs"), (307, "sendmmsg"),
    (308, "setns"), (309, "getcpu"), (310, "process_vm_readv"), (311, "process_vm_writev"),
    (312, "kcmp"), (313, "finit_module"), (314, "sched_setattr"), (315, "sched_getattr"),
    (316, "renameat2"), (317, "seccomp"), (318, "getrandom"), (319, "memfd_create"),
    (320, "kexec_file_load"), (321, "bpf"), (322, "execveat"), (323, "userfaultfd"),
    (324, "membarrier"), (325, "mlock2"), (326, "copy_file_range"), (327, "preadv2"),
    (328, "pwritev2"), (329, "pkey_mprotect"), (330, "pkey_alloc"), (331, "pkey_free"),
    (332, "statx"), (333, "io_pgetevents"), (334, "rseq"), (335, "uretprobe"),
    (336, "uprobe"), (424, "pidfd_send_signal"), (425, "io_uring_setup"), (426, "io_uring_enter"),
    (427, "io_uring_register"), (428, "open_tree"), (429, "move_mount"), (430, "fsopen"),
    (431, "fsconfig"), (432, "fsmount"), (433, "fspick"), (434, "pidfd_open"),
    (435, "clone3"), (436, "close_range"), (437, "openat2"), (438, "pidfd_getfd"),
    (439, "faccessat2"), (440, "process_madvise"), (441, "epoll_pwait2"), (442, "mount_setattr"),
    (443, "quotactl_fd"), (444, "landlock_create_ruleset"), (445, "landlock_add_rule"), (446, "landlock_restrict_self"),
    (447, "memfd_secret"), (448, "process_mrelease"), (449, "futex_waitv"), (450, "set_mempolicy_home_node"),
    (451, "cachestat"), (452, "fchmodat2"), (453, "map_shadow_stack"), (454, "futex_wake"),
    (455, "futex_wait"), (456, "futex_requeue"), (457, "statmount"), (458, "listmount"),
    (459, "lsm_get_self_attr"), (460, "lsm_set_self_attr"), (461, "lsm_list_modules"), (462, "mseal"),
    (463, "setxattrat"), (464, "getxattrat"), (465, "listxattrat"), (466, "removexattrat"),
    (467, "open_tree_attr"), (468, "file_getattr"), (469, "file_setattr"),
];

/// Known-denied syscalls (`EPERM`), by number and table name. The names are
/// checked against `TABLE` by a unit test, so a mistyped number cannot hide.
pub const DENY: &[(u32, &str)] = &[
    (101, "ptrace"),
    (298, "perf_event_open"),
    (321, "bpf"),
    (323, "userfaultfd"),
    (165, "mount"),
    (166, "umount2"),
    (155, "pivot_root"),
    (428, "open_tree"),
    (429, "move_mount"),
    (430, "fsopen"),
    (431, "fsconfig"),
    (432, "fsmount"),
    (433, "fspick"),
    (442, "mount_setattr"),
    (467, "open_tree_attr"),
    (246, "kexec_load"),
    (320, "kexec_file_load"),
    (175, "init_module"),
    (176, "delete_module"),
    (313, "finit_module"),
    (425, "io_uring_setup"),
    (426, "io_uring_enter"),
    (427, "io_uring_register"),
];

fn is_denied(nr: u32) -> bool {
    DENY.iter().any(|&(n, _)| n == nr)
}

/// The allowed numbers as inclusive ranges: the table minus the deny set,
/// with adjacent numbers coalesced.
pub fn allow_ranges() -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut nrs: Vec<u32> = TABLE
        .iter()
        .map(|&(n, _)| n)
        .filter(|&n| !is_denied(n))
        .collect();
    nrs.sort_unstable();
    for n in nrs {
        match out.last_mut() {
            Some(last) if last.1 + 1 == n => last.1 = n,
            _ => out.push((n, n)),
        }
    }
    out
}

fn ins(code: u16, jt: u8, jf: u8, k: u32) -> sock_filter {
    sock_filter { code, jt, jf, k }
}

fn off(n: usize) -> u8 {
    n.try_into().expect("BPF jump offset fits in 8 bits")
}

/// The assembled program. It loads only `arch` and `nr`:
///
/// 1. `arch != AUDIT_ARCH_X86_64` returns `ERRNO(EPERM)`.
/// 2. the x32 bit set returns `ERRNO(EPERM)`.
/// 3. each allow range returns `ALLOW`.
/// 4. each known-denied number returns `ERRNO(EPERM)`.
/// 5. anything else returns `ERRNO(ENOSYS)`.
pub fn program() -> Vec<sock_filter> {
    let eperm = RET_ERRNO | libc::EPERM as u32;
    let enosys = RET_ERRNO | libc::ENOSYS as u32;
    let mut p = vec![
        ins(BPF_LD_W_ABS, 0, 0, OFF_ARCH),
        ins(BPF_JEQ_K, 1, 0, AUDIT_ARCH_X86_64),
        ins(BPF_RET_K, 0, 0, eperm),
        ins(BPF_LD_W_ABS, 0, 0, OFF_NR),
        ins(BPF_JSET_K, 0, 1, X32_BIT),
        ins(BPF_RET_K, 0, 0, eperm),
    ];

    // Ranges are ascending, so below `lo` means no later range matches
    // either; the chain only needs to skip this block.
    for (lo, hi) in allow_ranges() {
        if lo == hi {
            // JEQ lo: match -> RET ALLOW, else skip it.
            p.push(ins(BPF_JEQ_K, 0, 1, lo));
            p.push(ins(BPF_RET_K, 0, 0, RET_ALLOW));
        } else {
            // JGE lo else skip 2; JGE hi+1 skips the RET, else RET ALLOW.
            p.push(ins(BPF_JGE_K, 0, 2, lo));
            p.push(ins(BPF_JGE_K, 1, 0, hi + 1));
            p.push(ins(BPF_RET_K, 0, 0, RET_ALLOW));
        }
    }

    // N compares, then RET ENOSYS (the fall-through), then the shared RET
    // EPERM: compare k jumps off(N - k) to land on the EPERM.
    let n = DENY.len();
    for (k, &(nr, _)) in DENY.iter().enumerate() {
        p.push(ins(BPF_JEQ_K, off(n - k), 0, nr));
    }
    p.push(ins(BPF_RET_K, 0, 0, enosys));
    p.push(ins(BPF_RET_K, 0, 0, eperm));
    p
}

/// Child-side filter installation, run after `landlock::apply` and before
/// `sys::set_no_new_privs`. `apply_filter` sets NoNewPrivs itself and then
/// installs; the explicit call after it is harmless.
pub fn apply() -> Result<(), LayerError> {
    let prog = program();
    seccompiler::apply_filter(&prog).map_err(|e| {
        let errno = match &e {
            seccompiler::Error::Prctl(io) | seccompiler::Error::Seccomp(io) => {
                io.raw_os_error().unwrap_or(libc::EIO)
            }
            _ => libc::EIO,
        };
        LayerError::refused(errno, "seccomp: install filter")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A minimal classic-BPF interpreter for the opcodes this program uses.
    /// It panics on anything else, which doubles as an opcode allow-list.
    fn run(prog: &[sock_filter], arch: u32, nr: u32) -> u32 {
        let mut a: u32 = 0;
        let mut pc = 0usize;
        loop {
            let i = &prog[pc];
            pc += 1;
            match i.code {
                BPF_LD_W_ABS => {
                    a = match i.k {
                        0 => nr,
                        4 => arch,
                        k => panic!("load at offset {k}"),
                    }
                }
                BPF_JEQ_K => pc += if a == i.k { i.jt } else { i.jf } as usize,
                BPF_JGE_K => pc += if a >= i.k { i.jt } else { i.jf } as usize,
                BPF_JSET_K => pc += if a & i.k != 0 { i.jt } else { i.jf } as usize,
                BPF_RET_K => return i.k,
                c => panic!("unexpected opcode {c:#x}"),
            }
        }
    }

    fn expect(nr: u32) -> u32 {
        if is_denied(nr) {
            RET_ERRNO | libc::EPERM as u32
        } else if TABLE.iter().any(|&(n, _)| n == nr) {
            RET_ALLOW
        } else {
            RET_ERRNO | libc::ENOSYS as u32
        }
    }

    #[test]
    fn table_shape_matches_the_record() {
        assert_eq!(TABLE.len(), 383);
        assert_eq!(TABLE.last().unwrap().0, 469);
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0), "sorted, unique");
        let gap: Vec<u32> = (0..=469)
            .filter(|n| !TABLE.iter().any(|&(t, _)| t == *n))
            .collect();
        assert_eq!(gap, (337..=423).collect::<Vec<_>>());
    }

    #[test]
    fn deny_set_names_match_the_table() {
        let names: BTreeMap<u32, &str> = TABLE.iter().copied().collect();
        for &(nr, name) in DENY {
            assert_eq!(names.get(&nr), Some(&name), "deny entry {nr}");
        }
        assert_eq!(DENY.len(), 23);
        for allowed in [
            (435, "clone3"),
            (457, "statmount"),
            (458, "listmount"),
            (39, "getpid"),
            (317, "seccomp"),
            (444, "landlock_create_ruleset"),
        ] {
            assert_eq!(names.get(&allowed.0), Some(&allowed.1));
            assert!(!is_denied(allowed.0), "{} must stay allowed", allowed.1);
        }
    }

    #[test]
    fn allow_ranges_match_the_record() {
        assert_eq!(
            allow_ranges(),
            vec![
                (0, 100),
                (102, 154),
                (156, 164),
                (167, 174),
                (177, 245),
                (247, 297),
                (299, 312),
                (314, 319),
                (322, 322),
                (324, 336),
                (424, 424),
                (434, 441),
                (443, 466),
                (468, 469),
            ]
        );
        let allowed: u32 = allow_ranges().iter().map(|&(l, h)| h - l + 1).sum();
        assert_eq!(allowed, 360);
    }

    #[test]
    fn every_number_gets_the_documented_action() {
        let prog = program();
        for nr in 0..=0x1000u32 {
            assert_eq!(run(&prog, AUDIT_ARCH_X86_64, nr), expect(nr), "nr {nr}");
        }
        for nr in [u32::MAX >> 1, 0x3fff_ffff, 0x0001_0000, 0x7fff_ffff] {
            assert_eq!(
                run(&prog, AUDIT_ARCH_X86_64, nr & !X32_BIT),
                RET_ERRNO | libc::ENOSYS as u32,
                "nr {nr:#x}"
            );
        }
    }

    #[test]
    fn x32_and_foreign_arch_are_eperm_for_every_number() {
        let prog = program();
        let eperm = RET_ERRNO | libc::EPERM as u32;
        for nr in 0..=0x1000u32 {
            assert_eq!(run(&prog, AUDIT_ARCH_X86_64, nr | X32_BIT), eperm);
            // AUDIT_ARCH_I386, AUDIT_ARCH_AARCH64, and garbage.
            for arch in [0x4000_0003, 0xc000_00b7, 0, 0xffff_ffff] {
                assert_eq!(run(&prog, arch, nr), eperm, "arch {arch:#x} nr {nr}");
            }
        }
    }

    #[test]
    fn program_reads_only_arch_and_nr_and_never_kills() {
        let prog = program();
        assert!(prog.len() < 4096);
        for i in &prog {
            match i.code {
                BPF_LD_W_ABS => assert!(i.k == 0 || i.k == 4, "load at {}", i.k),
                BPF_RET_K => {
                    let action = i.k & RET_ACTION_MASK;
                    assert!(
                        action == RET_ALLOW || action == RET_ERRNO,
                        "return {:#x}",
                        i.k
                    );
                }
                BPF_JEQ_K | BPF_JGE_K | BPF_JSET_K => {}
                c => panic!("unexpected opcode {c:#x}"),
            }
        }
        // Every jump lands inside the program.
        for (pc, i) in prog.iter().enumerate() {
            if matches!(i.code, BPF_JEQ_K | BPF_JGE_K | BPF_JSET_K) {
                assert!(pc + 1 + (i.jt.max(i.jf) as usize) < prog.len());
            }
        }
    }
}
