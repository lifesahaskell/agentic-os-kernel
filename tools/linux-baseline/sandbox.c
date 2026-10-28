/* Restriction configurations and their self-checks.
 *
 *   C0   none
 *   C1a  seccomp-bpf, one instruction (allow all)
 *   C1b  seccomp-bpf, allowlist by syscall number (64 instructions)
 *   C1c  C1b plus one load of args[0] before the nr load (A1)
 *   C2a  Landlock filesystem ruleset (ABI 7, all 16 filesystem rights handled)
 *   C2b  Landlock TCP connect rule for one port, plus abstract-unix-socket and signal scopes
 *   C3   one Landlock ruleset with the C2a and C2b parts, then the C1b filter
 *
 * Landlock is applied before seccomp, so the Landlock syscalls need not be on the allowlist.
 */
#include "sandbox.h"

#include <arpa/inet.h>
#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <netinet/in.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/un.h>
#include <unistd.h>
#include <signal.h>

#include "common.h"
#include "uapi_compat.h"

/* C1b allowlist.
 *
 * Source: the union of syscall names in tools/linux-baseline/.work/strace-micro.txt, produced by
 * `strace -f -qq -c` on a C0 smoke invocation of bin/micro under taskset (taskset's own calls,
 * such as sched_setaffinity, are part of the union). Added by hand: kill (the signal-scope
 * self-check; without it the default EPERM would pass neg_scope_signal without Landlock denying
 * anything), exit_group and exit (strace -c does not count calls that never return). `sysinfo`
 * is excluded on purpose: it is the seccomp negative test. Padded with common syscalls to exactly
 * 58 entries. The measured syscalls come last, so they sit at the end of the linear chain. The
 * program is 58 + 6 = 64 instructions.
 */
static const struct {
	const char *name;
	int nr;
} allow[] = {
	{ "accept4", SYS_accept4 }, /* strace */
	{ "access", SYS_access }, /* strace */
	{ "arch_prctl", SYS_arch_prctl }, /* strace */
	{ "bind", SYS_bind }, /* strace */
	{ "brk", SYS_brk }, /* strace */
	{ "exit", SYS_exit }, /* self-check and exit path */
	{ "exit_group", SYS_exit_group }, /* self-check and exit path */
	{ "fstat", SYS_fstat }, /* strace */
	{ "futex", SYS_futex }, /* strace */
	{ "getpid", SYS_getpid }, /* strace */
	{ "getrandom", SYS_getrandom }, /* strace */
	{ "getsockname", SYS_getsockname }, /* strace */
	{ "kill", SYS_kill }, /* self-check and exit path */
	{ "listen", SYS_listen }, /* strace */
	{ "lseek", SYS_lseek }, /* strace */
	{ "mmap", SYS_mmap }, /* strace */
	{ "mprotect", SYS_mprotect }, /* strace */
	{ "munmap", SYS_munmap }, /* strace */
	{ "newfstatat", SYS_newfstatat }, /* strace */
	{ "pipe2", SYS_pipe2 }, /* strace */
	{ "prctl", SYS_prctl }, /* strace */
	{ "pread64", SYS_pread64 }, /* strace */
	{ "prlimit64", SYS_prlimit64 }, /* strace */
	{ "read", SYS_read }, /* strace */
	{ "recvfrom", SYS_recvfrom }, /* strace */
	{ "rseq", SYS_rseq }, /* strace */
	{ "rt_sigprocmask", SYS_rt_sigprocmask }, /* strace */
	{ "sched_getaffinity", SYS_sched_getaffinity }, /* strace */
	{ "sched_setaffinity", SYS_sched_setaffinity }, /* strace */
	{ "sendto", SYS_sendto }, /* strace */
	{ "set_robust_list", SYS_set_robust_list }, /* strace */
	{ "set_tid_address", SYS_set_tid_address }, /* strace */
	{ "setsockopt", SYS_setsockopt }, /* strace */
	{ "socket", SYS_socket }, /* strace */
	{ "socketpair", SYS_socketpair }, /* strace */
	{ "statfs", SYS_statfs }, /* strace */
	{ "unlink", SYS_unlink }, /* strace */
	{ "write", SYS_write }, /* strace */
	{ "dup", SYS_dup }, /* padding */
	{ "dup2", SYS_dup2 }, /* padding */
	{ "fcntl", SYS_fcntl }, /* padding */
	{ "poll", SYS_poll }, /* padding */
	{ "getuid", SYS_getuid }, /* padding */
	{ "geteuid", SYS_geteuid }, /* padding */
	{ "getgid", SYS_getgid }, /* padding */
	{ "getegid", SYS_getegid }, /* padding */
	{ "getcwd", SYS_getcwd }, /* padding */
	{ "getpgrp", SYS_getpgrp }, /* padding */
	{ "getppid", SYS_getppid }, /* strace, measured */
	{ "openat", SYS_openat }, /* strace, measured */
	{ "close", SYS_close }, /* strace, measured */
	{ "connect", SYS_connect }, /* strace, measured */
	{ "sendmsg", SYS_sendmsg }, /* strace, measured */
	{ "recvmsg", SYS_recvmsg }, /* strace, measured */
	{ "execve", SYS_execve }, /* strace, measured */
	{ "clone", SYS_clone }, /* strace, measured */
	{ "clone3", SYS_clone3 }, /* strace, measured */
	{ "wait4", SYS_wait4 }, /* strace, measured */
};
#define N_ALLOW (sizeof allow / sizeof allow[0])

enum { LL_NONE = 0, LL_FS = 1, LL_NET_SCOPE = 2 };

struct cfg {
	const char *name;
	int seccomp;	/* 0 none, 1 allow-all, 2 allowlist, 3 allowlist plus args[0] load */
	int landlock;	/* bit set of LL_FS, LL_NET_SCOPE */
};

static const struct cfg cfgs[] = {
	{ "C0", 0, LL_NONE },
	{ "C1a", 1, LL_NONE },
	{ "C1b", 2, LL_NONE },
	{ "C1c", 3, LL_NONE },
	{ "C2a", 0, LL_FS },
	{ "C2b", 0, LL_NET_SCOPE },
	{ "C3", 2, LL_FS | LL_NET_SCOPE },
};

static const struct cfg *find_cfg(const char *name)
{
	for (size_t i = 0; i < sizeof cfgs / sizeof cfgs[0]; i++)
		if (!strcmp(cfgs[i].name, name))
			return &cfgs[i];
	return NULL;
}

int sb_known(const char *name)
{
	return find_cfg(name) != NULL;
}

static long base_seccomp_mode;

void sb_baseline(const struct sb_ctx *ctx, struct sb_report *rep)
{
	memset(rep, 0, sizeof *rep);
	strcpy(rep->seccomp, "-");
	strcpy(rep->seccomp_filters, "-");
	strcpy(rep->nonewprivs, "-");
	rep->landlock = "none";
	rep->neg_seccomp = rep->neg_fs = rep->neg_net = "na";
	rep->neg_scope_unix = rep->neg_scope_signal = "na";
	if (status_field(ctx->status_fd, "Seccomp_filters", rep->base_filters,
			 sizeof rep->base_filters))
		strcpy(rep->base_filters, "-1");
	char m[16];
	base_seccomp_mode = status_field(ctx->status_fd, "Seccomp", m, sizeof m) ? -1 : atol(m);
}

/* ---- seccomp ---------------------------------------------------------------------------- */

#define MAX_INSNS 128

static int build_allowlist(struct sock_filter *p, int with_args0)
{
	int n = 0;
	p[n++] = (struct sock_filter)BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
					      offsetof(struct seccomp_data, arch));
	p[n++] = (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH_X86_64, 1, 0);
	p[n++] = (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_KILL_PROCESS);
	if (with_args0)
		p[n++] = (struct sock_filter)BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
						      offsetof(struct seccomp_data, args[0]));
	p[n++] = (struct sock_filter)BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
					      offsetof(struct seccomp_data, nr));
	/* Entry i is followed by (N_ALLOW - 1 - i) entries, the ERRNO return and the ALLOW return;
	 * a match jumps over all of them but the last. */
	for (size_t i = 0; i < N_ALLOW; i++)
		p[n++] = (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, (unsigned)allow[i].nr,
						      (unsigned char)(N_ALLOW - i), 0);
	p[n++] = (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | EPERM);
	p[n++] = (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
	return n;
}

static int install_filter(struct sock_filter *insns, unsigned short n)
{
	struct sock_fprog prog = { .len = n, .filter = insns };
	if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0))
		return -1;
	return (int)syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0, &prog);
}

static int apply_seccomp(int kind, struct sb_report *rep)
{
	struct sock_filter insns[MAX_INSNS];
	int n;
	if (kind == 1) {
		insns[0] = (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
		n = 1;
	} else {
		n = build_allowlist(insns, kind == 3);
	}
	rep->filter_insns = n;
	return install_filter(insns, (unsigned short)n);
}

/* ---- Landlock --------------------------------------------------------------------------- */

static int ll_add_path(int rfd, const char *path, uint64_t access)
{
	int pfd = open(path, O_PATH | O_CLOEXEC);
	if (pfd < 0)
		return -1;
	struct aok_landlock_path_beneath_attr pb = { .allowed_access = access, .parent_fd = pfd };
	int r = (int)syscall(__NR_landlock_add_rule, rfd, AOK_LANDLOCK_RULE_PATH_BENEATH, &pb, 0);
	int e = errno;
	close(pfd);
	errno = e;
	return r;
}

static int apply_landlock(int parts, const struct sb_ctx *ctx, struct sb_report *rep)
{
	struct aok_landlock_ruleset_attr attr;
	memset(&attr, 0, sizeof attr);
	if (parts & LL_FS)
		attr.handled_access_fs = AOK_LANDLOCK_ACCESS_FS_ALL;
	if (parts & LL_NET_SCOPE) {
		attr.handled_access_net = AOK_LANDLOCK_ACCESS_NET_CONNECT_TCP;
		attr.scoped = AOK_LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | AOK_LANDLOCK_SCOPE_SIGNAL;
	}
	int rfd = (int)syscall(__NR_landlock_create_ruleset, &attr, sizeof attr, 0);
	if (rfd < 0)
		return -1;
	if (parts & LL_FS) {
		if (ll_add_path(rfd, ctx->tmpfs_dir, AOK_LANDLOCK_ACCESS_FS_ALL) ||
		    ll_add_path(rfd, ctx->root_dir, AOK_LANDLOCK_ACCESS_FS_ALL) ||
		    ll_add_path(rfd, "/usr",
				AOK_LANDLOCK_ACCESS_FS_EXECUTE | AOK_LANDLOCK_ACCESS_FS_READ_FILE |
					AOK_LANDLOCK_ACCESS_FS_READ_DIR) ||
		    ll_add_path(rfd, "/etc",
				AOK_LANDLOCK_ACCESS_FS_READ_FILE | AOK_LANDLOCK_ACCESS_FS_READ_DIR))
			return -1;
	}
	if (parts & LL_NET_SCOPE) {
		struct aok_landlock_net_port_attr np = {
			.allowed_access = AOK_LANDLOCK_ACCESS_NET_CONNECT_TCP,
			.port = (uint64_t)ctx->net_port,
		};
		if (syscall(__NR_landlock_add_rule, rfd, AOK_LANDLOCK_RULE_NET_PORT, &np, 0))
			return -1;
	}
	if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0))
		return -1;
	if (syscall(__NR_landlock_restrict_self, rfd, 0))
		return -1;
	close(rfd);
	rep->landlock = (parts == LL_FS) ? "fs" : (parts == LL_NET_SCOPE) ? "net_scope" : "fs_net_scope";
	return 0;
}

int sb_apply(const char *name, const struct sb_ctx *ctx, struct sb_report *rep)
{
	const struct cfg *c = find_cfg(name);
	if (!c) {
		errno = EINVAL;
		return -1;
	}
	if (c->landlock && apply_landlock(c->landlock, ctx, rep))
		return -1;
	if (c->seccomp && apply_seccomp(c->seccomp, rep))
		return -1;
	return 0;
}

/* ---- self-checks ------------------------------------------------------------------------ */

static int load_status(const struct sb_ctx *ctx, struct sb_report *rep)
{
	if (status_field(ctx->status_fd, "Seccomp", rep->seccomp, sizeof rep->seccomp) ||
	    status_field(ctx->status_fd, "Seccomp_filters", rep->seccomp_filters,
			 sizeof rep->seccomp_filters) ||
	    status_field(ctx->status_fd, "NoNewPrivs", rep->nonewprivs, sizeof rep->nonewprivs))
		return -1;
	return 0;
}

static const char *expect_errno(int rc, int want)
{
	return (rc == -1 && errno == want) ? "pass" : "fail";
}

static const char *check_neg_seccomp(void)
{
	char buf[256];
	errno = 0;
	return expect_errno((int)syscall(SYS_sysinfo, buf), EPERM);
}

static const char *check_neg_fs(void)
{
	errno = 0;
	int fd = open("/proc/version", O_RDONLY | O_CLOEXEC);
	if (fd >= 0) {
		close(fd);
		return "fail";
	}
	return expect_errno(fd, EACCES);
}

static const char *check_neg_net(const struct sb_ctx *ctx)
{
	struct sockaddr_in a;
	memset(&a, 0, sizeof a);
	a.sin_family = AF_INET;
	a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	a.sin_port = htons((unsigned short)(ctx->net_port + 1));
	int fd = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
	if (fd < 0)
		return "fail";
	errno = 0;
	int rc = connect(fd, (struct sockaddr *)&a, sizeof a);
	const char *r = expect_errno(rc, EACCES);
	close(fd);
	return r;
}

static const char *check_neg_scope_unix(const struct sb_ctx *ctx)
{
	struct sockaddr_un a;
	size_t nl = strlen(ctx->abstract_name);
	memset(&a, 0, sizeof a);
	a.sun_family = AF_UNIX;
	memcpy(a.sun_path + 1, ctx->abstract_name, nl);
	int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
	if (fd < 0)
		return "fail";
	errno = 0;
	int rc = connect(fd, (struct sockaddr *)&a, (socklen_t)(offsetof(struct sockaddr_un, sun_path) + 1 + nl));
	const char *r = expect_errno(rc, EPERM);
	close(fd);
	return r;
}

static const char *check_neg_scope_signal(const struct sb_ctx *ctx)
{
	errno = 0;
	return expect_errno(kill(ctx->peer_pid, 0), EPERM);
}

int sb_check(const char *name, const struct sb_ctx *ctx, struct sb_report *rep)
{
	const struct cfg *c = find_cfg(name);
	if (!c || load_status(ctx, rep))
		return 0;
	long base = atol(rep->base_filters);
	long filters = atol(rep->seccomp_filters);
	int ok = 1;

	if (c->seccomp) {
		ok &= !strcmp(rep->seccomp, "2") && filters == base + 1;
		ok &= !strcmp(rep->nonewprivs, "1");
	} else {
		ok &= filters == base && (base > 0 ? !strcmp(rep->seccomp, "2")
						   : base_seccomp_mode == 0 && !strcmp(rep->seccomp, "0"));
		if (c->landlock)
			ok &= !strcmp(rep->nonewprivs, "1");
	}
	if (c->seccomp >= 2) {
		rep->neg_seccomp = check_neg_seccomp();
		ok &= !strcmp(rep->neg_seccomp, "pass");
	}
	if (c->landlock & LL_FS) {
		rep->neg_fs = check_neg_fs();
		ok &= !strcmp(rep->neg_fs, "pass");
	}
	if (c->landlock & LL_NET_SCOPE) {
		rep->neg_net = check_neg_net(ctx);
		rep->neg_scope_unix = check_neg_scope_unix(ctx);
		rep->neg_scope_signal = check_neg_scope_signal(ctx);
		ok &= !strcmp(rep->neg_net, "pass") && !strcmp(rep->neg_scope_unix, "pass") &&
		      !strcmp(rep->neg_scope_signal, "pass");
	}
	return ok;
}

void sb_selfcheck_line(FILE *f, const char *label, int round, const struct sb_report *rep,
		       long errors, int ok)
{
	fprintf(f,
		"%s round=%d pid=%d Seccomp=%s Seccomp_filters=%s base_filters=%s NoNewPrivs=%s "
		"landlock=%s neg_seccomp=%s neg_fs=%s neg_net=%s neg_scope_unix=%s "
		"neg_scope_signal=%s filter_insns=%d errors=%ld result=%s\n",
		label, round, (int)getpid(), rep->seccomp, rep->seccomp_filters, rep->base_filters,
		rep->nonewprivs, rep->landlock, rep->neg_seccomp, rep->neg_fs, rep->neg_net,
		rep->neg_scope_unix, rep->neg_scope_signal, rep->filter_insns, errors,
		ok && errors == 0 ? "ok" : "FAIL");
	fflush(f);
}
