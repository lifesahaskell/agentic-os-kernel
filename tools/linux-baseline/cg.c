/* cgroup v2 helpers. See cg.h. */
#include <errno.h>
#include <fcntl.h>
#include <linux/sched.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

#include "cg.h"
#include "uapi_compat.h"

static char g_S[512];

const char *cg_S(void)
{
	return g_S;
}

int cg_self_path(char *out, size_t n)
{
	char buf[1024];
	int fd = open("/proc/self/cgroup", O_RDONLY | O_CLOEXEC);
	if (fd < 0)
		return -1;
	ssize_t r = read(fd, buf, sizeof buf - 1);
	close(fd);
	if (r <= 0)
		return -1;
	buf[r] = 0;
	char *p = strstr(buf, "0::");
	if (!p) {
		errno = ENOTSUP;
		return -1;
	}
	p += 3;
	char *nl = strchr(p, '\n');
	if (nl)
		*nl = 0;
	if (snprintf(out, n, "%s%s", CG_ROOT, p) >= (int)n) {
		errno = ENAMETOOLONG;
		return -1;
	}
	return 0;
}

static int put_path(const char *path, const char *value)
{
	int fd = open(path, O_WRONLY | O_CLOEXEC);
	if (fd < 0)
		return -1;
	ssize_t w = write(fd, value, strlen(value));
	int e = errno;
	close(fd);
	if (w != (ssize_t)strlen(value)) {
		errno = w < 0 ? e : EIO;
		return -1;
	}
	return 0;
}

int cg_put(const char *dir, const char *file, const char *value)
{
	char path[640];
	snprintf(path, sizeof path, "%s/%s", dir, file);
	return put_path(path, value);
}

int cg_read(const char *dir, const char *file, char *buf, size_t n)
{
	char path[640];
	snprintf(path, sizeof path, "%s/%s", dir, file);
	int fd = open(path, O_RDONLY | O_CLOEXEC);
	if (fd < 0)
		return -1;
	ssize_t r = read(fd, buf, n - 1);
	close(fd);
	if (r < 0)
		return -1;
	buf[r] = 0;
	return (int)r;
}

int cg_write(const char *dir, const char *file, const char *value)
{
	char back[256];
	if (cg_put(dir, file, value) != 0)
		return -1;
	if (cg_read(dir, file, back, sizeof back) < 0)
		return -1;
	size_t l = strlen(back);
	while (l && (back[l - 1] == '\n' || back[l - 1] == ' '))
		back[--l] = 0;
	if (strcmp(back, value) != 0) {
		errno = EIO;
		return -1;
	}
	return 0;
}

long long cg_key(const char *dir, const char *file, const char *key)
{
	char buf[1024];
	if (cg_read(dir, file, buf, sizeof buf) < 0)
		return -1;
	size_t kl = strlen(key);
	for (char *p = buf; p && *p;) {
		if (!strncmp(p, key, kl) && p[kl] == ' ')
			return strtoll(p + kl + 1, NULL, 10);
		p = strchr(p, '\n');
		if (p)
			p++;
	}
	return -1;
}

int cg_make_leaf(const char *name, char *path_out, size_t n)
{
	if (snprintf(path_out, n, "%s/%s", g_S, name) >= (int)n) {
		errno = ENAMETOOLONG;
		return -1;
	}
	if (mkdir(path_out, 0755) != 0 && errno != EEXIST)
		return -1;
	return 0;
}

int cg_setup(void)
{
	char self[640];
	if (cg_self_path(g_S, sizeof g_S) != 0)
		return -1;
	if (cg_make_leaf("self", self, sizeof self) != 0)
		return -1;
	if (cg_put(self, "cgroup.procs", "0") != 0)
		return -1;
	if (cg_put(g_S, "cgroup.subtree_control", "+memory +pids") != 0)
		return -1;
	return 0;
}

pid_t cg_clone3_into(int leaf_fd, int *pidfd)
{
	struct clone_args args;
	memset(&args, 0, sizeof args);
	args.flags = CLONE_INTO_CGROUP | CLONE_PIDFD;
	args.exit_signal = SIGCHLD;
	args.cgroup = (uint64_t)leaf_fd;
	args.pidfd = (uint64_t)(uintptr_t)pidfd;
	return (pid_t)syscall(SYS_clone3, &args, sizeof args);
}

int cg_kill_and_rmdir(const char *leaf)
{
	cg_put(leaf, "cgroup.kill", "1");
	for (int i = 0; i < 200; i++) {
		if (rmdir(leaf) == 0)
			return 0;
		if (errno != EBUSY)
			return -1;
		struct timespec ts = { 0, 5 * 1000 * 1000 };
		nanosleep(&ts, NULL);
	}
	return -1;
}
