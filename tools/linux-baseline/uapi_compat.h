/* Constants the installed uapi headers (linux-libc-dev 6.8) lack.
 *
 * Source: upstream include/uapi/linux/landlock.h and include/uapi/linux/pidfd.h at tag v6.18.
 * Every name carries an AOK_ prefix so it cannot collide with the system headers or with
 * glibc's sys/pidfd.h. The layouts are confirmed at runtime: micro --env checks that
 * PIDFD_GET_INFO returns this process's pid and a cgroup id, and the Landlock ABI is read
 * from the kernel before any ruleset is built.
 */
#ifndef AOK_UAPI_COMPAT_H
#define AOK_UAPI_COMPAT_H

#include <linux/types.h>
#include <sys/ioctl.h>

/* Landlock (v6.18 landlock.h). */
struct aok_landlock_ruleset_attr {
	__u64 handled_access_fs;
	__u64 handled_access_net;
	__u64 scoped;
};

struct aok_landlock_path_beneath_attr {
	__u64 allowed_access;
	__s32 parent_fd;
} __attribute__((packed));

struct aok_landlock_net_port_attr {
	__u64 allowed_access;
	__u64 port;
};

#define AOK_LANDLOCK_CREATE_RULESET_VERSION (1U << 0)
#define AOK_LANDLOCK_RULE_PATH_BENEATH 1
#define AOK_LANDLOCK_RULE_NET_PORT 2

/* FS rights are bits 0..15, EXECUTE (0) through IOCTL_DEV (15). */
#define AOK_LANDLOCK_ACCESS_FS_ALL 0xFFFFULL
#define AOK_LANDLOCK_ACCESS_FS_EXECUTE (1ULL << 0)
#define AOK_LANDLOCK_ACCESS_FS_READ_FILE (1ULL << 2)
#define AOK_LANDLOCK_ACCESS_FS_READ_DIR (1ULL << 3)

#define AOK_LANDLOCK_ACCESS_NET_BIND_TCP (1ULL << 0)
#define AOK_LANDLOCK_ACCESS_NET_CONNECT_TCP (1ULL << 1)

#define AOK_LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET (1ULL << 0)
#define AOK_LANDLOCK_SCOPE_SIGNAL (1ULL << 1)

#ifndef __NR_landlock_create_ruleset
#define __NR_landlock_create_ruleset 444
#endif
#ifndef __NR_landlock_add_rule
#define __NR_landlock_add_rule 445
#endif
#ifndef __NR_landlock_restrict_self
#define __NR_landlock_restrict_self 446
#endif
#ifndef __NR_lsm_list_modules
#define __NR_lsm_list_modules 461
#endif
#ifndef __NR_pidfd_open
#define __NR_pidfd_open 434
#endif
#ifndef __NR_pidfd_send_signal
#define __NR_pidfd_send_signal 424
#endif
#ifndef __NR_clone3
#define __NR_clone3 435
#endif

/* pidfd (v6.18 pidfd.h). */
#define AOK_PIDFD_INFO_PID (1ULL << 0)
#define AOK_PIDFD_INFO_CREDS (1ULL << 1)
#define AOK_PIDFD_INFO_CGROUPID (1ULL << 2)
#define AOK_PIDFD_INFO_EXIT (1ULL << 3)

struct aok_pidfd_info {
	__u64 mask;
	__u64 cgroupid;
	__u32 pid;
	__u32 tgid;
	__u32 ppid;
	__u32 ruid;
	__u32 rgid;
	__u32 euid;
	__u32 egid;
	__u32 suid;
	__u32 sgid;
	__u32 fsuid;
	__u32 fsgid;
	__s32 exit_code;
	__u32 coredump_mask;
	__u32 __spare1;
};

#define AOK_PIDFS_IOCTL_MAGIC 0xFF
#define AOK_PIDFD_GET_INFO _IOWR(AOK_PIDFS_IOCTL_MAGIC, 11, struct aok_pidfd_info)

#define AOK_SO_PEERPIDFD 77

#endif
