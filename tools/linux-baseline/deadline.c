/* S6 wall-time leg: deadline to reaped latency for a spinner child.
 *
 * A pure busy-loop child is spawned, a timerfd is armed 10 ms after the spawn, and on expiry the
 * parent kills the child either with pidfd_send_signal(SIGKILL) (pidfd_*) or by writing 1 to the
 * leaf's cgroup.kill (cgkill_*). Competitors are busy loops on the same CPU (0, 1 or 4).
 * This suite uses CLOCK_MONOTONIC because timerfd cannot use CLOCK_MONOTONIC_RAW.
 */
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/syscall.h>
#include <sys/timerfd.h>
#include <sys/wait.h>
#include <unistd.h>

#include "cg.h"
#include "common.h"
#include "uapi_compat.h"

#define CLK CLOCK_MONOTONIC
#define DEADLINE_NS 10000000ULL

#ifndef P_PIDFD
#define P_PIDFD 3
#endif

static pid_t competitors[4];
static int n_comp;

static void spinner_body(void)
{
	for (;;)
		__asm__ volatile("" ::: "memory");
}

static void start_competitors(int n)
{
	for (int i = 0; i < n; i++) {
		pid_t p = fork();
		if (p < 0)
			die("fork competitor");
		if (p == 0) {
			prctl(PR_SET_PDEATHSIG, SIGKILL);
			spinner_body();
		}
		competitors[n_comp++] = p;
	}
}

static void stop_competitors(void)
{
	for (int i = 0; i < n_comp; i++) {
		kill(competitors[i], SIGKILL);
		waitpid(competitors[i], NULL, 0);
	}
	n_comp = 0;
}

struct dctx {
	int cgkill;
	int leaf_fd;	/* cgkill: directory fd of S/agent */
	int kill_fd;	/* cgkill: S/agent/cgroup.kill */
	int events_fd;	/* cgkill: S/agent/cgroup.events */
};

/* Spawns the spinner. pidfd configs: fork then pidfd_open. cgkill configs: clone3 into the leaf. */
static pid_t spawn_spinner(const struct dctx *d, int *pidfd)
{
	pid_t p;
	if (d->cgkill) {
		p = cg_clone3_into(d->leaf_fd, pidfd);
		if (p == 0)
			spinner_body();	/* the child only loops; no libc state is touched */
		return p;
	}
	p = fork();
	if (p == 0) {
		prctl(PR_SET_PDEATHSIG, SIGKILL);
		spinner_body();
	}
	if (p > 0) {
		*pidfd = (int)syscall(__NR_pidfd_open, p, 0);
		if (*pidfd < 0) {
			kill(p, SIGKILL);
			waitpid(p, NULL, 0);
			return -1;
		}
	}
	return p;
}

/* Waits until cgroup.events reports "populated 0". Returns the time, or 0 on timeout (1 s). */
static uint64_t wait_empty(int events_fd)
{
	uint64_t t_limit = now_ns(CLK) + 1000000000ULL;
	for (;;) {
		char buf[256];
		ssize_t r = pread(events_fd, buf, sizeof buf - 1, 0);
		if (r < 0)
			return 0;
		buf[r] = 0;
		if (strstr(buf, "populated 0"))
			return now_ns(CLK);
		uint64_t now = now_ns(CLK);
		if (now >= t_limit)
			return 0;
		struct pollfd pf = { .fd = events_fd, .events = POLLPRI };
		poll(&pf, 1, (int)((t_limit - now) / 1000000ULL) + 1);
	}
}

/* One trial. Returns the number of errors, and fills the four latencies when ok (ns, as double). */
static long trial(const struct dctx *d, int tfd, double out[4])
{
	int pidfd = -1;
	pid_t p = spawn_spinner(d, &pidfd);
	if (p < 0)
		return 1;

	uint64_t t_start = now_ns(CLK);
	uint64_t t_deadline = t_start + DEADLINE_NS;
	struct itimerspec its = { .it_value = { .tv_sec = (time_t)(t_deadline / 1000000000ULL),
						.tv_nsec = (long)(t_deadline % 1000000000ULL) } };
	if (timerfd_settime(tfd, TFD_TIMER_ABSTIME, &its, NULL) != 0) {
		kill(p, SIGKILL);
		waitpid(p, NULL, 0);
		close(pidfd);
		return 1;
	}
	uint64_t expirations;
	if (read(tfd, &expirations, sizeof expirations) != sizeof expirations) {
		kill(p, SIGKILL);
		waitpid(p, NULL, 0);
		close(pidfd);
		return 1;
	}
	uint64_t t_wakeup = now_ns(CLK);

	long err = 0;
	if (d->cgkill) {
		if (write(d->kill_fd, "1", 1) != 1)
			err++;
	} else {
		if (syscall(__NR_pidfd_send_signal, pidfd, SIGKILL, NULL, 0) != 0)
			err++;
	}
	siginfo_t si;
	memset(&si, 0, sizeof si);
	if (waitid(P_PIDFD, (id_t)pidfd, &si, WEXITED) != 0) {
		err++;
		kill(p, SIGKILL);
		waitpid(p, NULL, 0);
	} else if (si.si_code != CLD_KILLED || si.si_status != SIGKILL) {
		err++;
	}
	uint64_t t_reaped = now_ns(CLK);
	uint64_t t_empty = 0;
	if (d->cgkill) {
		t_empty = wait_empty(d->events_fd);
		if (!t_empty)
			err++;
	}
	close(pidfd);

	out[0] = (double)(t_wakeup - t_deadline);
	out[1] = (double)(t_reaped - t_wakeup);
	out[2] = (double)(t_reaped - t_deadline);
	out[3] = t_empty ? (double)(t_empty - t_deadline) : 0.0;
	return err;
}

int main(int argc, char **argv)
{
	const char *config = NULL, *out = NULL, *selfcheck = NULL;
	int round = 0, smoke = 0;
	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--smoke"))
			smoke = 1;
		else if (i + 1 < argc && !strcmp(argv[i], "--config"))
			config = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--round"))
			round = atoi(argv[++i]);
		else if (i + 1 < argc && !strcmp(argv[i], "--out"))
			out = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--selfcheck"))
			selfcheck = argv[++i];
		else {
			fprintf(stderr, "deadline: bad argument %s\n", argv[i]);
			return 64;
		}
	}
	if (!config || !out || !selfcheck || round < 1) {
		fprintf(stderr, "usage: deadline --config pidfd_L0|..|cgkill_L4 --round R --out F "
				"--selfcheck F [--smoke]\n");
		return 64;
	}
	struct dctx d = { .cgkill = 0, .leaf_fd = -1, .kill_fd = -1, .events_fd = -1 };
	int load = -1;
	if (!strncmp(config, "pidfd_L", 7))
		load = atoi(config + 7);
	else if (!strncmp(config, "cgkill_L", 8)) {
		d.cgkill = 1;
		load = atoi(config + 8);
	}
	if (load != 0 && load != 1 && load != 4) {
		fprintf(stderr, "deadline: unknown config %s\n", config);
		return 64;
	}

	FILE *csv = fopen(out, "a");
	FILE *sc = fopen(selfcheck, "a");
	if (!csv || !sc)
		die("open out/selfcheck");

	size_t trials = smoke ? 10 : 200, warm = smoke ? 2 : 10;
	size_t total = trials + warm;

	char leaf[640] = "";
	if (d.cgkill) {
		/* Before the competitors: the no-internal-process rule needs S empty of processes. */
		if (cg_setup() != 0)
			die("cg_setup");
		if (cg_make_leaf("agent", leaf, sizeof leaf) != 0)
			die("cg leaf");
		d.leaf_fd = open(leaf, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
		char p[700];
		snprintf(p, sizeof p, "%s/cgroup.kill", leaf);
		d.kill_fd = open(p, O_WRONLY | O_CLOEXEC);
		snprintf(p, sizeof p, "%s/cgroup.events", leaf);
		d.events_fd = open(p, O_RDONLY | O_CLOEXEC);
		if (d.leaf_fd < 0 || d.kill_fd < 0 || d.events_fd < 0)
			die("open cgroup files");
	}

	int tfd = timerfd_create(CLK, TFD_CLOEXEC);
	if (tfd < 0)
		die("timerfd_create");

	double *s[4];
	for (int k = 0; k < 4; k++)
		s[k] = samples_alloc(trials);
	start_competitors(load);

	long errors = 0, row_err = 0;
	size_t got = 0;
	for (size_t i = 0; i < total; i++) {
		double v[4];
		long e = trial(&d, tfd, v);
		if (i < warm)
			continue;	/* warm-up trials are not recorded; their errors still count */
		row_err += e;
		if (e == 0) {
			for (int k = 0; k < 4; k++)
				s[k][got] = v[k];
			got++;
		} else {
			/* a failed trial leaves a hole; fill with the previous good sample to keep n fixed */
			for (int k = 0; k < 4; k++)
				s[k][got] = got ? s[k][got - 1] : 0.0;
			got++;
		}
	}
	errors = row_err;
	stop_competitors();

	if (errors == 0) {
		static const char *names[4] = { "deadline_to_wakeup", "wakeup_to_reaped",
						"deadline_to_reaped", "deadline_to_empty" };
		for (int k = 0; k < (d.cgkill ? 4 : 3); k++)
			emit_row(csv, "deadline", round, config, names[k], trials, warm, s[k], 0);
	} else {
		fprintf(stderr, "deadline: %ld trial errors under %s\n", errors, config);
	}
	fclose(csv);
	fprintf(sc, "%s round=%d errors=%ld result=%s\n", config, round, errors,
		errors ? "FAIL" : "ok");
	fclose(sc);

	if (d.cgkill)
		cg_kill_and_rmdir(leaf);
	return errors ? 2 : 0;
}
