/* cgroup v2 helpers for the deadline and cgroup suites. glibc only.
 *
 * All of this works only for a process that starts inside a delegated cgroup S (a cgroup its
 * uid owns, with memory and pids available). run.sh launches such processes through a
 * transient service unit with Delegate=yes. Every function returns 0 on success or -1 with
 * errno set, unless stated otherwise.
 */
#ifndef AOK_CG_H
#define AOK_CG_H

#include <stddef.h>
#include <sys/types.h>

#define CG_ROOT "/sys/fs/cgroup"

/* Absolute path of the delegated cgroup S, recorded by cg_setup(). */
const char *cg_S(void);

/* Reads this process's cgroup from /proc/self/cgroup and writes CG_ROOT + that path to out. */
int cg_self_path(char *out, size_t n);

/* Records S, creates S/self, moves this process into it, and enables +memory +pids on S. After
 * this, S holds no process directly, so subtree_control can be written. */
int cg_setup(void);

/* mkdir S/name; the absolute path goes to path_out. EEXIST is not an error. */
int cg_make_leaf(const char *name, char *path_out, size_t n);

/* Writes value to dir/file and reads it back; fails with EIO when the trimmed read-back differs.
 * For limit files such as memory.max and pids.max. */
int cg_write(const char *dir, const char *file, const char *value);

/* Writes value to dir/file without read-back. For cgroup.procs, cgroup.kill, subtree_control. */
int cg_put(const char *dir, const char *file, const char *value);

/* Reads dir/file into buf (NUL-terminated, trailing newline kept). Returns bytes read or -1. */
int cg_read(const char *dir, const char *file, char *buf, size_t n);

/* Returns the value of "key" from a flat-keyed file such as memory.events, or -1. */
long long cg_key(const char *dir, const char *file, const char *key);

/* clone3 with CLONE_INTO_CGROUP | CLONE_PIDFD and exit signal SIGCHLD. Returns the child pid in
 * the parent (the pidfd goes to *pidfd), 0 in the child, -1 on error.
 * The child may only call execve, _exit or raw syscalls: glibc state after a raw clone3 is stale
 * (no atfork handlers ran, the TID cache is not refreshed). */
pid_t cg_clone3_into(int leaf_fd, int *pidfd);

/* Writes 1 to cgroup.kill, then removes the directory, retrying while zombies drain. */
int cg_kill_and_rmdir(const char *leaf);

#endif
