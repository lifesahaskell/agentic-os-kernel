/* Unprivileged probe of the 12 feature-floor rows (scoping-linux.md section 3), printed as
 * key=value lines for env.txt. Nothing here needs root and nothing here writes outside stdout.
 *
 * Every row is one floor_<row> line plus one floormethod_<row> line:
 *   probe    the value came from a syscall or a /proc or /sys read on the running kernel
 *   derived  the value came from the kernel release string alone and carries ASSUMED
 * A source that cannot be read writes the value unreadable with method probe; a guess is never
 * written. Kernel build-configuration files are never read: the point of ENV-02 is the running
 * system, not what a distro says it shipped.
 *
 * Clone-flag bit values for CLONE_PIDFD_AUTOKILL and CLONE_NNP are ASSUMED (taken from memory of
 * the v7.1 sched.h, not read from a tree). They stay ASSUMED until include/uapi/linux/sched.h and
 * kernel/fork.c at v7.1 are read on the reference host. A wrong value reads as absent, so the
 * rows carry ASSUMED in their value.
 */
#include <errno.h>
#include <fcntl.h>
#include <glob.h>
#include <linux/sched.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/utsname.h>
#include <unistd.h>

#include "common.h"
#include "uapi_compat.h"

#define FLAG_INTO_CGROUP (1ULL << 33)
#define FLAG_PIDFD_AUTOKILL_ASSUMED (1ULL << 34)
#define FLAG_NNP_ASSUMED (1ULL << 35)

/* Reads the first line of path into out, stripping the newline. Returns 0 on success, -1 with
 * errno set otherwise. An empty file is a success with an empty string. */
static int read_line(const char *path, char *out, size_t outlen)
{
	FILE *f = fopen(path, "re");
	if (!f)
		return -1;
	if (!fgets(out, (int)outlen, f)) {
		if (ferror(f)) {
			int e = errno;
			fclose(f);
			errno = e;
			return -1;
		}
		out[0] = 0;
	}
	fclose(f);
	out[strcspn(out, "\n")] = 0;
	return 0;
}

static void row(const char *name, const char *value, const char *method)
{
	printf("floor_%s=%s\n", name, value);
	printf("floormethod_%s=%s\n", name, method);
}

/* True when the whitespace-separated token list contains tok. */
static int has_token(const char *list, const char *tok)
{
	size_t tl = strlen(tok);
	for (const char *p = list; *p;) {
		while (*p == ' ' || *p == '\t')
			p++;
		const char *e = p;
		while (*e && *e != ' ' && *e != '\t')
			e++;
		if ((size_t)(e - p) == tl && !strncmp(p, tok, tl))
			return 1;
		p = e;
	}
	return 0;
}

/* A clone3 whose cgroup fd is invalid. A kernel that knows every flag in the request reaches the
 * cgroup stage and fails EBADF; a kernel that does not know one fails EINVAL first. The call can
 * never create a child, but a child is stopped defensively. */
static const char *probe_clone3(uint64_t flags)
{
	struct clone_args a;
	int pidfd = -1;
	memset(&a, 0, sizeof a);
	a.flags = flags | FLAG_INTO_CGROUP;
	if (flags & CLONE_PIDFD)
		a.pidfd = (uint64_t)(uintptr_t)&pidfd;
	a.exit_signal = SIGCHLD;
	a.cgroup = 9999;
	long r = syscall(__NR_clone3, &a, sizeof a);
	if (r == 0)
		_exit(0);
	if (r > 0)
		return "unreadable";
	if (errno == EBADF)
		return "present";
	if (errno == EINVAL)
		return "absent";
	if (errno == ENOSYS)
		return "absent";
	return "unreadable";
}

static void row_clone(void)
{
	/* A kernel without CLONE_INTO_CGROUP answers EINVAL to the bare probe. */
	row("clone_into_cgroup", probe_clone3(0), "probe");

	char v[96];
	snprintf(v, sizeof v, "%s (flag value ASSUMED)",
		 probe_clone3(FLAG_PIDFD_AUTOKILL_ASSUMED | CLONE_PIDFD));
	row("clone_pidfd_autokill", v, "probe");
	snprintf(v, sizeof v, "%s (flag value ASSUMED)", probe_clone3(FLAG_NNP_ASSUMED));
	row("clone_nnp", v, "probe");
}

static void row_pidfd_info(void)
{
	long fd = syscall(__NR_pidfd_open, getpid(), 0);
	if (fd < 0) {
		row("pidfd_get_info_cgroupid", errno == ENOSYS ? "absent" : "unreadable", "probe");
		return;
	}
	struct aok_pidfd_info info;
	memset(&info, 0, sizeof info);
	info.mask = AOK_PIDFD_INFO_CGROUPID;
	int rc = ioctl((int)fd, AOK_PIDFD_GET_INFO, &info);
	int e = errno;
	close((int)fd);
	if (rc == 0)
		row("pidfd_get_info_cgroupid",
		    (info.mask & AOK_PIDFD_INFO_CGROUPID) ? "present" : "absent", "probe");
	else if (e == ENOTTY || e == EINVAL || e == ENOSYS)
		row("pidfd_get_info_cgroupid", "absent", "probe");
	else
		row("pidfd_get_info_cgroupid", "unreadable", "probe");
}

static void row_landlock(void)
{
	long abi = syscall(__NR_landlock_create_ruleset, NULL, 0,
			   AOK_LANDLOCK_CREATE_RULESET_VERSION);
	char v[32];
	if (abi >= 0) {
		snprintf(v, sizeof v, "%ld", abi);
		row("landlock_abi", v, "probe");
	} else if (errno == ENOSYS) {
		row("landlock_abi", "absent", "probe");
	} else if (errno == EOPNOTSUPP) {
		row("landlock_abi", "disabled", "probe");
	} else {
		row("landlock_abi", "unreadable", "probe");
	}
}

/* Present when the symbol table lists any bpf_lsm_ hook. Names stay visible to an unprivileged
 * reader even when addresses are zeroed. A table with no entries at all is unreadable. */
static void row_bpf_lsm_symbols(void)
{
	FILE *f = fopen("/proc/kallsyms", "re");
	if (!f) {
		row("config_bpf_lsm", "unreadable", "probe");
		return;
	}
	char line[512];
	int any = 0, hit = 0;
	while (fgets(line, sizeof line, f)) {
		any = 1;
		if (strstr(line, " bpf_lsm_")) {
			hit = 1;
			break;
		}
	}
	fclose(f);
	row("config_bpf_lsm", !any ? "unreadable" : hit ? "present" : "absent", "probe");
}

static void row_bpf_lsm_order(void)
{
	char lsm[512], cmd[4096];
	if (read_line("/sys/kernel/security/lsm", lsm, sizeof lsm) < 0) {
		row("bpf_default_lsm_order", "unreadable", "probe");
		return;
	}
	int in_order = 0;
	for (char *p = lsm; *p; p++)
		if (*p == ',')
			*p = ' ';
	in_order = has_token(lsm, "bpf");
	const char *override = "unreadable";
	if (read_line("/proc/cmdline", cmd, sizeof cmd) == 0) {
		override = "no";
		for (const char *p = cmd; *p;) {
			while (*p == ' ')
				p++;
			if (!strncmp(p, "lsm=", 4)) {
				override = "yes";
				break;
			}
			while (*p && *p != ' ')
				p++;
		}
	}
	char v[96];
	snprintf(v, sizeof v, "%s lsm_override=%s", in_order ? "yes" : "no", override);
	row("bpf_default_lsm_order", v, "probe");
}

static void row_controller(const char *name, const char *controller)
{
	char c[512];
	if (read_line("/sys/fs/cgroup/cgroup.controllers", c, sizeof c) < 0)
		row(name, "unreadable", "probe");
	else
		row(name, has_token(c, controller) ? "present" : "absent", "probe");
}

static void row_psi_cgroup(void)
{
	char cg[4096], path[4400], buf[8];
	FILE *f = fopen("/proc/self/cgroup", "re");
	cg[0] = 0;
	if (f) {
		char line[4096];
		while (fgets(line, sizeof line, f)) {
			if (!strncmp(line, "0::", 3)) {
				snprintf(cg, sizeof cg, "%s", line + 3);
				cg[strcspn(cg, "\n")] = 0;
				break;
			}
		}
		fclose(f);
	}
	if (!cg[0]) {
		row("psi_per_cgroup", "unreadable", "probe");
		return;
	}
	snprintf(path, sizeof path, "/sys/fs/cgroup%s/memory.pressure", strcmp(cg, "/") ? cg : "");
	int fd = open(path, O_RDONLY | O_CLOEXEC);
	if (fd < 0) {
		row("psi_per_cgroup", errno == ENOENT ? "absent" : "unreadable", "probe");
		return;
	}
	ssize_t n = read(fd, buf, sizeof buf);
	int e = errno;
	close(fd);
	/* Writing PSI off at boot makes the file exist but fail the read with EOPNOTSUPP. */
	if (n >= 0)
		row("psi_per_cgroup", "present", "probe");
	else
		row("psi_per_cgroup", e == EOPNOTSUPP ? "absent" : "unreadable", "probe");
}

/* Kernel major.minor from the release string; 0 on parse failure. */
static int kernel_at_least(int maj, int min, int *ok)
{
	struct utsname u;
	int a = 0, b = 0;
	*ok = 0;
	if (uname(&u) < 0 || sscanf(u.release, "%d.%d", &a, &b) != 2)
		return 0;
	*ok = 1;
	return a > maj || (a == maj && b >= min);
}

static void row_derived(const char *name, int maj, int min)
{
	int ok;
	int yes = kernel_at_least(maj, min, &ok);
	struct utsname u;
	char v[160];
	if (!ok || uname(&u) < 0) {
		row(name, "unreadable", "probe");
		return;
	}
	snprintf(v, sizeof v, "%s ASSUMED (kernel %s against first-in-mainline %d.%d)",
		 yes ? "present" : "absent", u.release, maj, min);
	row(name, v, "derived");
}

static void kv_file(const char *key, const char *path)
{
	char v[512];
	if (read_line(path, v, sizeof v) < 0)
		snprintf(v, sizeof v, "unreadable");
	printf("%s=%s\n", key, v);
}

static void kv_presence(const char *key, const char *path)
{
	struct stat st;
	printf("%s=%s\n", key, stat(path, &st) == 0 ? "present" : "absent");
}

static void kv_secure_boot(void)
{
	glob_t g;
	const char *v = "unreadable";
	if (glob("/sys/firmware/efi/efivars/SecureBoot-*", 0, NULL, &g) == 0) {
		FILE *f = fopen(g.gl_pathv[0], "re");
		if (f) {
			unsigned char b[8];
			size_t n = fread(b, 1, sizeof b, f);
			fclose(f);
			/* Four attribute bytes, then the one-byte value. */
			if (n == 5)
				v = b[4] ? "enabled" : "disabled";
		}
	}
	globfree(&g);
	printf("secure_boot=%s\n", v);
}

static void kv_kernel_pkg(void)
{
	char v[512];
	if (read_line("/proc/version_signature", v, sizeof v) < 0) {
		struct utsname u;
		if (uname(&u) == 0)
			snprintf(v, sizeof v, "%s", u.version);
		else
			snprintf(v, sizeof v, "unreadable");
	}
	printf("kernel_pkg=%s\n", v);
}

int main(void)
{
	row_clone();
	row_pidfd_info();
	row_landlock();
	row_derived("bpf_lsm_cgroup", 6, 0);
	row_bpf_lsm_symbols();
	row_bpf_lsm_order();
	row_derived("io_uring_task_restrictions", 7, 0);
	row_controller("memcg", "memory");
	row_psi_cgroup();
	row_controller("dmem", "dmem");

	kv_presence("btf_vmlinux", "/sys/kernel/btf/vmlinux");
	kv_file("yama_ptrace_scope", "/proc/sys/kernel/yama/ptrace_scope");
	kv_file("lockdown", "/sys/kernel/security/lockdown");
	kv_secure_boot();
	kv_file("scaling_governor", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");
	kv_file("scaling_driver", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver");
	kv_file("smt_active", "/sys/devices/system/cpu/smt/active");
	kv_kernel_pkg();
	kv_file("apparmor_userns_restrict", "/proc/sys/kernel/apparmor_restrict_unprivileged_userns");
	return 0;
}
