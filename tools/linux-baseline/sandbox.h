/* Restriction configurations C0, C1a, C1b, C1c, C2a, C2b, C3 and their self-checks. */
#ifndef AOK_SANDBOX_H
#define AOK_SANDBOX_H

#include <stdio.h>
#include <sys/types.h>

struct sb_ctx {
	const char *tmpfs_dir;
	const char *root_dir;
	int status_fd;		/* /proc/self/status, opened before any restriction */
	int net_port;		/* TCP listener port the net rule allows */
	pid_t peer_pid;		/* unrestricted helper, target of the signal-scope check */
	const char *abstract_name; /* abstract unix socket the peer listens on (no leading NUL) */
};

struct sb_report {
	char seccomp[16];
	char seccomp_filters[16];
	char base_filters[16];
	char nonewprivs[16];
	const char *landlock;
	const char *neg_seccomp, *neg_fs, *neg_net, *neg_scope_unix, *neg_scope_signal;
	int filter_insns;
};

int sb_known(const char *name);
/* Reads Seccomp_filters before anything is applied. */
void sb_baseline(const struct sb_ctx *ctx, struct sb_report *rep);
/* Applies the configuration. Returns 0 on success, -1 on failure (errno set). */
int sb_apply(const char *name, const struct sb_ctx *ctx, struct sb_report *rep);
/* Proves the restriction is active. Returns 1 when every applicable check passed. */
int sb_check(const char *name, const struct sb_ctx *ctx, struct sb_report *rep);
void sb_selfcheck_line(FILE *f, const char *label, int round, const struct sb_report *rep,
		       long errors, int ok);

#endif
