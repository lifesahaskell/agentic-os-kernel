/* cgroup legs (L10, A7): spawn cost into a cgroup, and OOM, pids.max, clone3 and PSI behaviour.
 *
 * Runs only inside a delegated cgroup. Timed legs (--config):
 *   spawn_fork        fork + exec /bin/true in S/self (baseline)
 *   spawn_clone3_into clone3(CLONE_INTO_CGROUP) into S/agent + exec
 *   spawn_fork_move   fork, the child writes 0 to S/agent/cgroup.procs, then exec
 * --behaviour runs the OOM, pids.max, clone3 functional and PSI legs once.
 */
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include "cg.h"
#include "common.h"
#include "uapi_compat.h"

static const char *true_argv[] = { "true", NULL };
static char *const empty_env[] = { NULL };

/* Returns 0 when the child exited 0. */
static int reap(pid_t p)
{
	int st = 0;
	if (waitpid(p, &st, 0) != p)
		return 1;
	return !(WIFEXITED(st) && WEXITSTATUS(st) == 0);
}

struct spawn_ctx {
	int mode;	/* 0 fork, 1 clone3_into, 2 fork_move */
	int leaf_fd;
	int procs_fd;
};

static long spawn_one(const struct spawn_ctx *c, double *sample)
{
	uint64_t t0, t1;
	pid_t p;
	int pidfd = -1;
	t0 = now_ns(RAW_CLOCK);
	if (c->mode == 1) {
		p = cg_clone3_into(c->leaf_fd, &pidfd);
		if (p == 0) {
			execve("/bin/true", (char *const *)true_argv, empty_env);
			_exit(127);
		}
	} else {
		p = fork();
		if (p == 0) {
			if (c->mode == 2 && write(c->procs_fd, "0", 1) != 1)
				_exit(126);
			execve("/bin/true", (char *const *)true_argv, empty_env);
			_exit(127);
		}
	}
	if (p < 0)
		return 1;
	int bad = reap(p);
	t1 = now_ns(RAW_CLOCK);
	if (pidfd >= 0)
		close(pidfd);
	*sample = (double)(t1 - t0);
	return bad;
}

static int run_spawn(const char *config, int round, int smoke, const char *out, const char *sc_path)
{
	struct spawn_ctx c = { .mode = -1, .leaf_fd = -1, .procs_fd = -1 };
	if (!strcmp(config, "spawn_fork"))
		c.mode = 0;
	else if (!strcmp(config, "spawn_clone3_into"))
		c.mode = 1;
	else if (!strcmp(config, "spawn_fork_move"))
		c.mode = 2;
	else {
		fprintf(stderr, "cgbench: unknown config %s\n", config);
		return 64;
	}
	FILE *csv = fopen(out, "a");
	FILE *sc = fopen(sc_path, "a");
	if (!csv || !sc)
		die("open out/selfcheck");
	if (cg_setup() != 0)
		die("cg_setup");
	char leaf[640];
	if (cg_make_leaf("agent", leaf, sizeof leaf) != 0)
		die("cg leaf");
	c.leaf_fd = open(leaf, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
	char procs[700];
	snprintf(procs, sizeof procs, "%s/cgroup.procs", leaf);
	c.procs_fd = open(procs, O_WRONLY | O_CLOEXEC);
	if (c.leaf_fd < 0 || c.procs_fd < 0)
		die("open leaf");

	size_t n = scale_n(1000, smoke), w = scale_warm(100, smoke);
	double *s = samples_alloc(n > w ? n : w);
	long errors = 0, e = 0;
	for (size_t i = 0; i < w; i++)
		errors += spawn_one(&c, &s[i]);
	for (size_t i = 0; i < n; i++)
		e += spawn_one(&c, &s[i]);
	errors += e;
	emit_row(csv, "cgroup", round, config, "spawn_exec_wait", n, w, s, errors);
	fclose(csv);
	fprintf(sc, "%s round=%d errors=%ld result=%s\n", config, round, errors, errors ? "FAIL" : "ok");
	fclose(sc);
	cg_kill_and_rmdir(leaf);
	return errors ? 2 : 0;
}

/* ---- behaviour legs ---- */

static const char *errname(int e)
{
	switch (e) {
	case EAGAIN: return "EAGAIN";
	case ENOMEM: return "ENOMEM";
	case EPERM: return "EPERM";
	case EACCES: return "EACCES";
	case EINVAL: return "EINVAL";
	case ENOENT: return "ENOENT";
	case EOPNOTSUPP: return "EOPNOTSUPP";
	case ENOSYS: return "ENOSYS";
	case EBUSY: return "EBUSY";
	case EIO: return "EIO";
	default: return "EOTHER";
	}
}

static long oom_trial(int idx, FILE *beh)
{
	char leaf[640];
	char name[32];
	snprintf(name, sizeof name, "oom%d", idx);
	if (cg_make_leaf(name, leaf, sizeof leaf) != 0)
		return fprintf(beh, "oom_trial=%d error=mkdir:%s\n", idx, errname(errno)), 1;
	/* T-261027-un5-01: never run an allocating child without a verified limit and swap off. */
	if (cg_write(leaf, "memory.max", "67108864") != 0 || cg_write(leaf, "memory.swap.max", "0") != 0) {
		fprintf(beh, "oom_trial=%d error=limit_not_verified:%s\n", idx, errname(errno));
		rmdir(leaf);
		return 1;
	}
	int pp[2];
	if (pipe2(pp, O_CLOEXEC) != 0)
		die("pipe");
	pid_t p = fork();
	if (p < 0)
		die("fork");
	if (p == 0) {
		char procs[700];
		snprintf(procs, sizeof procs, "%s/cgroup.procs", leaf);
		int fd = open(procs, O_WRONLY);
		if (fd < 0 || write(fd, "0", 1) != 1)
			_exit(126);
		close(fd);
		for (int i = 0; i < 512; i++) {
			char *m = mmap(NULL, 1 << 20, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
			if (m == MAP_FAILED)
				_exit(3);
			memset(m, 0xA5, 1 << 20);
			if (write(pp[1], "x", 1) != 1)
				_exit(4);
		}
		_exit(0);
	}
	close(pp[1]);
	int chunks = 0;
	char b;
	while (read(pp[0], &b, 1) == 1)
		chunks++;
	close(pp[0]);
	int st = 0;
	waitpid(p, &st, 0);
	long long oom = cg_key(leaf, "memory.events", "oom");
	long long oomk = cg_key(leaf, "memory.events", "oom_kill");
	char peak[64] = "na";
	if (cg_read(leaf, "memory.peak", peak, sizeof peak) > 0)
		peak[strcspn(peak, "\n")] = 0;
	fprintf(beh, "oom_trial=%d signal=%d exit=%d chunks_mib=%d oom=%lld oom_kill=%lld peak_bytes=%s\n",
		idx, WIFSIGNALED(st) ? WTERMSIG(st) : 0, WIFEXITED(st) ? WEXITSTATUS(st) : -1, chunks,
		oom, oomk, peak);
	cg_kill_and_rmdir(leaf);
	return 0;
}

struct pids_result {
	int forks_ok;
	int err;
};

static long pids_trial(int idx, FILE *beh)
{
	char leaf[640];
	char name[32];
	snprintf(name, sizeof name, "pids%d", idx);
	if (cg_make_leaf(name, leaf, sizeof leaf) != 0)
		return fprintf(beh, "pids_trial=%d error=mkdir:%s\n", idx, errname(errno)), 1;
	if (cg_write(leaf, "pids.max", "8") != 0) {
		fprintf(beh, "pids_trial=%d error=limit_not_verified:%s\n", idx, errname(errno));
		rmdir(leaf);
		return 1;
	}
	int pp[2];
	if (pipe2(pp, O_CLOEXEC) != 0)
		die("pipe");
	pid_t p = fork();
	if (p < 0)
		die("fork");
	if (p == 0) {
		char procs[700];
		snprintf(procs, sizeof procs, "%s/cgroup.procs", leaf);
		int fd = open(procs, O_WRONLY);
		if (fd < 0 || write(fd, "0", 1) != 1)
			_exit(126);
		close(fd);
		struct pids_result r = { 0, 0 };
		for (int i = 0; i < 64; i++) {
			pid_t g = fork();
			if (g == 0) {
				prctl(PR_SET_PDEATHSIG, SIGKILL);
				pause();
				_exit(0);
			}
			if (g < 0) {
				r.err = errno;
				break;
			}
			r.forks_ok++;
		}
		if (write(pp[1], &r, sizeof r) != sizeof r)
			_exit(4);
		pause();
		_exit(0);
	}
	close(pp[1]);
	struct pids_result r = { -1, 0 };
	if (read(pp[0], &r, sizeof r) != sizeof r)
		r.forks_ok = -1;
	close(pp[0]);
	long long evmax = cg_key(leaf, "pids.events", "max");
	fprintf(beh, "pids_trial=%d forks_ok=%d errno=%s pids_events_max=%lld\n", idx, r.forks_ok,
		r.err ? errname(r.err) : "none", evmax);
	cg_put(leaf, "cgroup.kill", "1");
	waitpid(p, NULL, 0);
	cg_kill_and_rmdir(leaf);
	return r.forks_ok < 0;
}

static long clone3_functional(FILE *beh)
{
	char leaf[640];
	if (cg_make_leaf("c3", leaf, sizeof leaf) != 0)
		return fprintf(beh, "clone3_into_cgroup_functional=no error=mkdir:%s\n", errname(errno)), 1;
	int lfd = open(leaf, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
	int pidfd = -1;
	pid_t p = cg_clone3_into(lfd, &pidfd);
	if (p == 0) {
		for (;;)
			__asm__ volatile("" ::: "memory");
	}
	if (p < 0) {
		fprintf(beh, "clone3_into_cgroup_functional=no error=clone3:%s\n", errname(errno));
		close(lfd);
		rmdir(leaf);
		return 1;
	}
	char path[64], got[1024], want[1024];
	snprintf(path, sizeof path, "/proc/%d/cgroup", (int)p);
	int fd = open(path, O_RDONLY | O_CLOEXEC);
	ssize_t r = fd >= 0 ? read(fd, got, sizeof got - 1) : -1;
	if (fd >= 0)
		close(fd);
	got[r > 0 ? r : 0] = 0;
	got[strcspn(got, "\n")] = 0;
	snprintf(want, sizeof want, "0::%s", leaf + strlen(CG_ROOT));
	int ok = r > 0 && !strcmp(got, want);
	fprintf(beh, "clone3_into_cgroup_functional=%s\n", ok ? "yes" : "no");
	kill(p, SIGKILL);
	waitpid(p, NULL, 0);
	close(pidfd);
	close(lfd);
	cg_kill_and_rmdir(leaf);
	return ok ? 0 : 1;
}

static void psi_probe(FILE *beh)
{
	char leaf[640], path[700];
	if (cg_make_leaf("psi", leaf, sizeof leaf) != 0) {
		fprintf(beh, "psi_trigger_register=%s\n", errname(errno));
		return;
	}
	snprintf(path, sizeof path, "%s/memory.pressure", leaf);
	int fd = open(path, O_RDWR | O_NONBLOCK | O_CLOEXEC);
	if (fd < 0) {
		fprintf(beh, "psi_trigger_register=%s\n", errname(errno));
	} else {
		const char *trig = "some 150000 2000000";
		ssize_t w = write(fd, trig, strlen(trig) + 1);
		fprintf(beh, "psi_trigger_register=%s\n", w < 0 ? errname(errno) : "ok");
		close(fd);
	}
	cg_kill_and_rmdir(leaf);
}

static int run_behaviour(int round, int smoke, const char *out, const char *sc_path)
{
	FILE *beh = fopen(out, "a");
	FILE *sc = fopen(sc_path, "a");
	if (!beh || !sc)
		die("open out/selfcheck");
	if (cg_setup() != 0)
		die("cg_setup");
	long errors = 0;
	errors += clone3_functional(beh);
	int trials = smoke ? 1 : 3;
	for (int i = 1; i <= trials; i++)
		errors += oom_trial(i, beh);
	for (int i = 1; i <= trials; i++)
		errors += pids_trial(i, beh);
	psi_probe(beh);
	fclose(beh);
	fprintf(sc, "behaviour round=%d errors=%ld result=%s\n", round, errors, errors ? "FAIL" : "ok");
	fclose(sc);
	return errors ? 2 : 0;
}

int main(int argc, char **argv)
{
	const char *config = NULL, *out = NULL, *selfcheck = NULL;
	int round = 0, smoke = 0, behaviour = 0;
	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--smoke"))
			smoke = 1;
		else if (!strcmp(argv[i], "--behaviour"))
			behaviour = 1;
		else if (i + 1 < argc && !strcmp(argv[i], "--config"))
			config = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--round"))
			round = atoi(argv[++i]);
		else if (i + 1 < argc && !strcmp(argv[i], "--out"))
			out = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--selfcheck"))
			selfcheck = argv[++i];
		else {
			fprintf(stderr, "cgbench: bad argument %s\n", argv[i]);
			return 64;
		}
	}
	if (!out || !selfcheck || round < 1 || (!behaviour && !config)) {
		fprintf(stderr, "usage: cgbench (--config C | --behaviour) --round R --out F "
				"--selfcheck F [--smoke]\n");
		return 64;
	}
	return behaviour ? run_behaviour(round, smoke, out, selfcheck)
			 : run_spawn(config, round, smoke, out, selfcheck);
}
