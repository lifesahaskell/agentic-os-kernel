/* Per-call latency microbenchmarks under one restriction configuration.
 *
 * micro --config NAME --round R --tmpfs-dir D --root-dir D --out FILE --selfcheck FILE
 *       [--smoke] [--label L]
 * micro --env
 *
 * The process applies the configuration through sandbox.c, proves it is active (selfcheck line),
 * then times each call with clock_gettime(CLOCK_MONOTONIC_RAW).
 */
#include <linux/lsm.h>
#include <netinet/in.h>
#include <spawn.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/prctl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/statfs.h>
#include <sys/syscall.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <pthread.h>
#include <signal.h>
#include <unistd.h>

#include "common.h"
#include "sandbox.h"
#include "uapi_compat.h"

struct mctx {
	int smoke;
	pid_t ppid;
	char tmp_file[256];	/* <tmpfs-dir>/f */
	char root_file[256];	/* <root-dir>/f */
	struct sockaddr_un unix_addr;
	socklen_t unix_len;
	int unix_lfd;		/* non-blocking unix stream listener */
	struct sockaddr_in tcp_addr;
	int tcp_lfd;		/* non-blocking TCP listener on 127.0.0.1 */
	int dgram_fd;		/* parent end of the ping-pong socketpair */
	pid_t peer_pid;
};

static char *const true_argv[] = { "true", NULL };
static char *const true_envp[] = { NULL };

typedef long (*bench_fn)(struct mctx *c, double *s, size_t n);

struct bench {
	const char *name;
	size_t n, warmup;
	bench_fn fn;
};

static long bench_timer_pair(struct mctx *c, double *s, size_t n)
{
	(void)c;
	for (size_t i = 0; i < n; i++) {
		uint64_t t0 = now_ns(RAW_CLOCK);
		uint64_t t1 = now_ns(RAW_CLOCK);
		s[i] = (double)(t1 - t0);
	}
	return 0;
}

static long bench_getppid(struct mctx *c, double *s, size_t n)
{
	long err = 0;
	for (size_t i = 0; i < n; i++) {
		uint64_t t0 = now_ns(RAW_CLOCK);
		long r = syscall(SYS_getppid);
		uint64_t t1 = now_ns(RAW_CLOCK);
		if (r != (long)c->ppid)
			err++;
		s[i] = (double)(t1 - t0);
	}
	return err;
}

/* A2: time 1000 consecutive calls and divide, to resolve below the timer quantum. */
static long bench_getppid_x1000(struct mctx *c, double *s, size_t n)
{
	long err = 0;
	for (size_t i = 0; i < n; i++) {
		long r = 0;
		uint64_t t0 = now_ns(RAW_CLOCK);
		for (int j = 0; j < 1000; j++)
			r = syscall(SYS_getppid);
		uint64_t t1 = now_ns(RAW_CLOCK);
		if (r != (long)c->ppid)
			err++;
		s[i] = (double)(t1 - t0) / 1000.0;
	}
	return err;
}


static long open_close(struct mctx *c, double *s, size_t n, const char *path)
{
	(void)c;
	long err = 0;
	for (size_t i = 0; i < n; i++) {
		uint64_t t0 = now_ns(RAW_CLOCK);
		int fd = openat(AT_FDCWD, path, O_RDONLY | O_CLOEXEC);
		if (fd >= 0)
			close(fd);
		else
			err++;
		uint64_t t1 = now_ns(RAW_CLOCK);
		s[i] = (double)(t1 - t0);
	}
	return err;
}

static long bench_openat_tmpfs(struct mctx *c, double *s, size_t n)
{
	return open_close(c, s, n, c->tmp_file);
}

static long bench_openat_rootfs(struct mctx *c, double *s, size_t n)
{
	return open_close(c, s, n, c->root_file);
}

/* Accepts and closes everything queued on a non-blocking listener, outside any timed region. */
static void drain(int lfd)
{
	for (;;) {
		int fd = accept4(lfd, NULL, NULL, SOCK_NONBLOCK | SOCK_CLOEXEC);
		if (fd < 0)
			break;
		close(fd);
	}
}

static long bench_unix_connect(struct mctx *c, double *s, size_t n)
{
	long err = 0;
	for (size_t i = 0; i < n; i++) {
		int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
		if (fd < 0) {
			err++;
			s[i] = 0;
			continue;
		}
		uint64_t t0 = now_ns(RAW_CLOCK);
		int r = connect(fd, (struct sockaddr *)&c->unix_addr, c->unix_len);
		close(fd);
		uint64_t t1 = now_ns(RAW_CLOCK);
		if (r != 0)
			err++;
		s[i] = (double)(t1 - t0);
		drain(c->unix_lfd);
	}
	return err;
}

/* A3: SO_LINGER {1,0} makes close send RST, so no TIME_WAIT builds up over ~700k connects. */
static long bench_tcp_connect(struct mctx *c, double *s, size_t n)
{
	long err = 0;
	struct linger lg = { 1, 0 };
	for (size_t i = 0; i < n; i++) {
		int fd = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
		if (fd < 0 || setsockopt(fd, SOL_SOCKET, SO_LINGER, &lg, sizeof lg)) {
			err++;
			s[i] = 0;
			if (fd >= 0)
				close(fd);
			continue;
		}
		uint64_t t0 = now_ns(RAW_CLOCK);
		int r = connect(fd, (struct sockaddr *)&c->tcp_addr, sizeof c->tcp_addr);
		close(fd);
		uint64_t t1 = now_ns(RAW_CLOCK);
		if (r != 0)
			err++;
		s[i] = (double)(t1 - t0);
		drain(c->tcp_lfd);
	}
	return err;
}

/* Timed region: fork through waitpid returning, so exec in the restricted process is included. */
static long bench_fork_exec_wait(struct mctx *c, double *s, size_t n)
{
	(void)c;
	long err = 0;
	for (size_t i = 0; i < n; i++) {
		int st = 0;
		uint64_t t0 = now_ns(RAW_CLOCK);
		pid_t pid = fork();
		if (pid == 0) {
			execve("/bin/true", true_argv, true_envp);
			_exit(127);
		}
		if (pid < 0 || waitpid(pid, &st, 0) != pid)
			err++;
		else if (!WIFEXITED(st) || WEXITSTATUS(st) != 0)
			err++;
		uint64_t t1 = now_ns(RAW_CLOCK);
		s[i] = (double)(t1 - t0);
	}
	return err;
}

static long bench_posix_spawn(struct mctx *c, double *s, size_t n)
{
	(void)c;
	long err = 0;
	for (size_t i = 0; i < n; i++) {
		pid_t pid = 0;
		int st = 0;
		uint64_t t0 = now_ns(RAW_CLOCK);
		int r = posix_spawn(&pid, "/bin/true", NULL, NULL, true_argv, true_envp);
		if (r != 0 || waitpid(pid, &st, 0) != pid)
			err++;
		else if (!WIFEXITED(st) || WEXITSTATUS(st) != 0)
			err++;
		uint64_t t1 = now_ns(RAW_CLOCK);
		s[i] = (double)(t1 - t0);
	}
	return err;
}

static long bench_unix_dgram_pingpong(struct mctx *c, double *s, size_t n)
{
	long err = 0;
	char out[64], in[64];
	memset(out, 'x', sizeof out);
	struct iovec oi = { out, sizeof out }, ii = { in, sizeof in };
	struct msghdr om, im;
	memset(&om, 0, sizeof om);
	memset(&im, 0, sizeof im);
	om.msg_iov = &oi;
	om.msg_iovlen = 1;
	im.msg_iov = &ii;
	im.msg_iovlen = 1;
	for (size_t i = 0; i < n; i++) {
		uint64_t t0 = now_ns(RAW_CLOCK);
		ssize_t a = sendmsg(c->dgram_fd, &om, 0);
		ssize_t b = recvmsg(c->dgram_fd, &im, 0);
		uint64_t t1 = now_ns(RAW_CLOCK);
		if (a != (ssize_t)sizeof out || b != (ssize_t)sizeof in)
			err++;
		s[i] = (double)(t1 - t0);
	}
	return err;
}

static const struct bench benches[] = {
	{ "timer_pair", 1000000, 100000, bench_timer_pair },
	{ "getppid", 1000000, 100000, bench_getppid },
	{ "getppid_x1000", 2000, 200, bench_getppid_x1000 },
	{ "openat_tmpfs", 200000, 20000, bench_openat_tmpfs },
	{ "openat_rootfs", 200000, 20000, bench_openat_rootfs },
	{ "unix_connect", 50000, 5000, bench_unix_connect },
	{ "tcp_connect", 20000, 2000, bench_tcp_connect },
	{ "fork_exec_wait", 1000, 100, bench_fork_exec_wait },
	{ "posix_spawn", 1000, 100, bench_posix_spawn },
	{ "unix_dgram_pingpong", 200000, 20000, bench_unix_dgram_pingpong },
};

static const char *lsm_name(uint64_t id)
{
	static char buf[24];
	switch (id) {
	case LSM_ID_CAPABILITY: return "capability";
	case LSM_ID_SELINUX: return "selinux";
	case LSM_ID_SMACK: return "smack";
	case LSM_ID_TOMOYO: return "tomoyo";
	case LSM_ID_APPARMOR: return "apparmor";
	case LSM_ID_YAMA: return "yama";
	case LSM_ID_LOADPIN: return "loadpin";
	case LSM_ID_SAFESETID: return "safesetid";
	case LSM_ID_LOCKDOWN: return "lockdown";
	case LSM_ID_BPF: return "bpf";
	case LSM_ID_LANDLOCK: return "landlock";
	/* Absent from the 6.8 headers; values from upstream v6.18 include/uapi/linux/lsm.h. */
	case 111: return "ima";
	case 112: return "evm";
	default:
		snprintf(buf, sizeof buf, "id%llu", (unsigned long long)id);
		return buf;
	}
}

static int do_env(void)
{
	/* active_lsms */
	uint64_t ids[64];
	uint32_t size = sizeof ids;
	long cnt = syscall(__NR_lsm_list_modules, ids, &size, 0);
	printf("active_lsms=");
	if (cnt < 0) {
		printf("unsupported:%d", errno);
	} else {
		for (long i = 0; i < cnt; i++)
			printf("%s%s", i ? "," : "", lsm_name(ids[i]));
	}
	printf("\n");

	/* landlock_abi */
	long abi = syscall(__NR_landlock_create_ruleset, NULL, 0, AOK_LANDLOCK_CREATE_RULESET_VERSION);
	if (abi < 0)
		printf("landlock_abi=unsupported:%d\n", errno);
	else
		printf("landlock_abi=%ld\n", abi);

	/* so_peerpidfd */
	int sv[2];
	if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv))
		die("socketpair");
	int pfd = -1;
	socklen_t len = sizeof pfd;
	if (getsockopt(sv[0], SOL_SOCKET, AOK_SO_PEERPIDFD, &pfd, &len) == 0) {
		printf("so_peerpidfd=ok\n");
		close(pfd);
	} else {
		printf("so_peerpidfd=unsupported:%d\n", errno);
	}
	close(sv[0]);
	close(sv[1]);

	/* pidfd_get_info */
	int self = (int)syscall(__NR_pidfd_open, getpid(), 0);
	if (self < 0) {
		printf("pidfd_get_info=unsupported:%d\n", errno);
	} else {
		struct aok_pidfd_info info;
		memset(&info, 0, sizeof info);
		info.mask = AOK_PIDFD_INFO_CGROUPID;
		errno = 0;
		if (ioctl(self, AOK_PIDFD_GET_INFO, &info) == 0 && info.pid == (uint32_t)getpid() &&
		    (info.mask & AOK_PIDFD_INFO_CGROUPID))
			printf("pidfd_get_info=ok\n");
		else if (errno)
			printf("pidfd_get_info=unsupported:%d\n", errno);
		else
			printf("pidfd_get_info=unsupported:0\n");
		close(self);
	}

	/* timer */
	struct timespec res;
	clock_getres(RAW_CLOCK, &res);
	printf("timer_res_ns=%lld\n", (long long)res.tv_sec * 1000000000LL + res.tv_nsec);
	uint64_t q = UINT64_MAX;
	for (int i = 0; i < 100000; i++) {
		uint64_t a = now_ns(RAW_CLOCK), b = now_ns(RAW_CLOCK);
		if (b > a && b - a < q)
			q = b - a;
	}
	printf("timer_quantum_ns=%llu\n", (unsigned long long)(q == UINT64_MAX ? 0 : q));
	return 0;
}

#define TMPFS_MAGIC_V 0x01021994L
#define EXT4_MAGIC_V 0xEF53L

static long make_file(const char *dir, char *dst, size_t dstlen, long magic)
{
	long err = 0;
	struct statfs sf;
	snprintf(dst, dstlen, "%s/f", dir);
	int fd = open(dst, O_CREAT | O_WRONLY | O_CLOEXEC, 0600);
	if (fd < 0)
		die("create bench file");
	close(fd);
	if (statfs(dst, &sf) != 0 || (long)sf.f_type != magic) {
		fprintf(stderr, "micro: %s is not on the expected filesystem type\n", dst);
		err++;
	}
	return err;
}

static void fill_sockaddr_un(struct sockaddr_un *a, socklen_t *len, const char *path)
{
	memset(a, 0, sizeof *a);
	a->sun_family = AF_UNIX;
	if (strlen(path) >= sizeof a->sun_path)
		die("unix path too long");
	strcpy(a->sun_path, path);
	*len = (socklen_t)(offsetof(struct sockaddr_un, sun_path) + strlen(path) + 1);
}

/* Echo peer for the dgram ping-pong. Forked before any restriction, so it is unrestricted.
 * It also owns the abstract unix listener the scope self-check connects to. It exits on error or
 * through PDEATHSIG, never on a signal from the (possibly signal-scoped) measured process. */
static void *life_watch(void *arg)
{
	char b;
	int fd = *(int *)arg;
	/* The parent holds the write end; EOF means the parent is gone. Landlock's signal scope
	 * blocks the kernel's PDEATHSIG from a scoped parent, so it cannot be relied on here. */
	while (read(fd, &b, 1) > 0)
		;
	_exit(0);
}

static void dgram_peer(int fd, int ready_fd, int life_fd, pid_t parent)
{
	static int life;
	pthread_t th;
	life = life_fd;
	prctl(PR_SET_PDEATHSIG, SIGKILL);
	if (getppid() != parent)
		_exit(1);
	if (pthread_create(&th, NULL, life_watch, &life))
		_exit(1);
	char name[64];
	int nl = snprintf(name, sizeof name, "aok-lb-%d", (int)parent);
	struct sockaddr_un a;
	memset(&a, 0, sizeof a);
	a.sun_family = AF_UNIX;
	memcpy(a.sun_path + 1, name, (size_t)nl);
	int l = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
	char ok = 0;
	if (l >= 0 &&
	    bind(l, (struct sockaddr *)&a, (socklen_t)(offsetof(struct sockaddr_un, sun_path) + 1 + nl)) == 0 &&
	    listen(l, 8) == 0)
		ok = 1;
	if (write(ready_fd, &ok, 1) != 1)
		_exit(1);
	close(ready_fd);
	char buf[64];
	for (;;) {
		ssize_t n = recv(fd, buf, sizeof buf, 0);
		if (n <= 0)
			_exit(0);
		if (send(fd, buf, (size_t)n, 0) != n)
			_exit(1);
	}
}

static long setup_env(struct mctx *c, struct sb_ctx *sb, const char *tmpfs_dir,
		      const char *root_dir, char *abstract_name, size_t abn_len)
{
	long err = 0;
	err += make_file(tmpfs_dir, c->tmp_file, sizeof c->tmp_file, TMPFS_MAGIC_V);
	err += make_file(root_dir, c->root_file, sizeof c->root_file, EXT4_MAGIC_V);

	char path[300];
	snprintf(path, sizeof path, "%s/stream.sock", tmpfs_dir);
	unlink(path);
	fill_sockaddr_un(&c->unix_addr, &c->unix_len, path);
	c->unix_lfd = socket(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
	if (c->unix_lfd < 0 ||
	    bind(c->unix_lfd, (struct sockaddr *)&c->unix_addr, c->unix_len) != 0 ||
	    listen(c->unix_lfd, 128) != 0)
		die("unix listener");

	c->tcp_lfd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
	memset(&c->tcp_addr, 0, sizeof c->tcp_addr);
	c->tcp_addr.sin_family = AF_INET;
	c->tcp_addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	c->tcp_addr.sin_port = 0;
	socklen_t sl = sizeof c->tcp_addr;
	if (c->tcp_lfd < 0 ||
	    bind(c->tcp_lfd, (struct sockaddr *)&c->tcp_addr, sizeof c->tcp_addr) != 0 ||
	    listen(c->tcp_lfd, 128) != 0 ||
	    getsockname(c->tcp_lfd, (struct sockaddr *)&c->tcp_addr, &sl) != 0)
		die("tcp listener");

	int sv[2], rp[2], lp[2];
	if (socketpair(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC, 0, sv) || pipe2(rp, O_CLOEXEC) ||
	    pipe2(lp, O_CLOEXEC))
		die("socketpair/pipe");
	pid_t parent = getpid();
	pid_t pid = fork();
	if (pid < 0)
		die("fork peer");
	if (pid == 0) {
		close(sv[0]);
		close(rp[0]);
		close(lp[1]);
		dgram_peer(sv[1], rp[1], lp[0], parent);
		_exit(0);
	}
	close(sv[1]);
	close(rp[1]);
	close(lp[0]);	/* lp[1] stays open in the parent until it exits */
	char ok = 0;
	if (read(rp[0], &ok, 1) != 1 || !ok)
		die("peer not ready");
	close(rp[0]);
	c->dgram_fd = sv[0];
	c->peer_pid = pid;

	snprintf(abstract_name, abn_len, "aok-lb-%d", (int)parent);
	sb->tmpfs_dir = tmpfs_dir;
	sb->root_dir = root_dir;
	sb->net_port = ntohs(c->tcp_addr.sin_port);
	sb->peer_pid = pid;
	sb->abstract_name = abstract_name;
	return err;
}

int main(int argc, char **argv)
{
	const char *config = NULL, *label = NULL, *tmpfs_dir = NULL, *root_dir = NULL;
	const char *out = NULL, *selfcheck = NULL;
	int round = 0, smoke = 0, env = 0;

	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--env"))
			env = 1;
		else if (!strcmp(argv[i], "--smoke"))
			smoke = 1;
		else if (i + 1 < argc && !strcmp(argv[i], "--config"))
			config = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--label"))
			label = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--round"))
			round = atoi(argv[++i]);
		else if (i + 1 < argc && !strcmp(argv[i], "--tmpfs-dir"))
			tmpfs_dir = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--root-dir"))
			root_dir = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--out"))
			out = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--selfcheck"))
			selfcheck = argv[++i];
		else {
			fprintf(stderr, "micro: bad argument %s\n", argv[i]);
			return 64;
		}
	}
	if (env)
		return do_env();
	if (!config || !out || !selfcheck || !tmpfs_dir || !root_dir || round < 1) {
		fprintf(stderr, "usage: micro --config C --round R --tmpfs-dir D --root-dir D "
				"--out F --selfcheck F [--smoke] [--label L]\n");
		return 64;
	}
	if (!sb_known(config)) {
		fprintf(stderr, "micro: unknown config %s\n", config);
		return 64;
	}
	if (!label)
		label = config;

	/* Open everything that needs a path before any restriction. */
	FILE *csv = fopen(out, "a");
	if (!csv)
		die("open out");
	FILE *sc = fopen(selfcheck, "a");
	if (!sc)
		die("open selfcheck");
	struct sb_ctx ctx = { .tmpfs_dir = tmpfs_dir, .root_dir = root_dir };
	ctx.status_fd = open("/proc/self/status", O_RDONLY | O_CLOEXEC);
	if (ctx.status_fd < 0)
		die("open /proc/self/status");

	struct sb_report rep;
	sb_baseline(&ctx, &rep);

	size_t maxn = 0;
	for (size_t i = 0; i < sizeof benches / sizeof benches[0]; i++) {
		size_t n = scale_n(benches[i].n, smoke), w = scale_warm(benches[i].warmup, smoke);
		if (n > maxn)
			maxn = n;
		if (w > maxn)
			maxn = w;
	}
	double *samples = samples_alloc(maxn);

	struct mctx mc = { .smoke = smoke, .ppid = getppid() };
	char abstract_name[64];
	long setup_errors = setup_env(&mc, &ctx, tmpfs_dir, root_dir, abstract_name, sizeof abstract_name);
	if (setup_errors) {
		sb_selfcheck_line(sc, label, round, &rep, setup_errors, 0);
		return 2;
	}

	if (sb_apply(config, &ctx, &rep) != 0) {
		fprintf(stderr, "micro: apply %s failed: %s\n", config, strerror(errno));
		sb_selfcheck_line(sc, label, round, &rep, 0, 0);
		return 2;
	}
	if (!sb_check(config, &ctx, &rep)) {
		fprintf(stderr, "micro: self-check for %s failed\n", config);
		sb_selfcheck_line(sc, label, round, &rep, 0, 0);
		return 2;
	}

	long errors = 0;
	for (size_t i = 0; i < sizeof benches / sizeof benches[0]; i++) {
		const struct bench *b = &benches[i];
		size_t n = scale_n(b->n, smoke), w = scale_warm(b->warmup, smoke);
		long e = b->fn(&mc, samples, w);
		e += b->fn(&mc, samples, n);
		errors += e;
		emit_row(csv, "micro", round, label, b->name, n, w, samples, e);
	}
	fclose(csv);
	sb_selfcheck_line(sc, label, round, &rep, errors, 1);
	return errors ? 2 : 0;
}
