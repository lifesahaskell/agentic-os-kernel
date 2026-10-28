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
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <unistd.h>

#include "common.h"
#include "sandbox.h"
#include "uapi_compat.h"

struct mctx {
	int smoke;
	pid_t ppid;
};

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

static const struct bench benches[] = {
	{ "timer_pair", 1000000, 100000, bench_timer_pair },
	{ "getppid", 1000000, 100000, bench_getppid },
	{ "getppid_x1000", 2000, 200, bench_getppid_x1000 },
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
