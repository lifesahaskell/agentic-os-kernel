/* Restriction configurations and their self-checks (task 1: C0 and C1a). */
#include "sandbox.h"

#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/syscall.h>
#include <unistd.h>

#include "common.h"
#include "uapi_compat.h"

int sb_known(const char *name)
{
	return !strcmp(name, "C0") || !strcmp(name, "C1a");
}

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
}

static int install_filter(struct sock_filter *insns, unsigned short n)
{
	struct sock_fprog prog = { .len = n, .filter = insns };
	if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0))
		return -1;
	return (int)syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0, &prog);
}

int sb_apply(const char *name, const struct sb_ctx *ctx, struct sb_report *rep)
{
	(void)ctx;
	if (!strcmp(name, "C0"))
		return 0;
	if (!strcmp(name, "C1a")) {
		struct sock_filter allow_all[] = { BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW) };
		rep->filter_insns = 1;
		return install_filter(allow_all, 1);
	}
	errno = EINVAL;
	return -1;
}

static int load_status(const struct sb_ctx *ctx, struct sb_report *rep)
{
	if (status_field(ctx->status_fd, "Seccomp", rep->seccomp, sizeof rep->seccomp) ||
	    status_field(ctx->status_fd, "Seccomp_filters", rep->seccomp_filters,
			 sizeof rep->seccomp_filters) ||
	    status_field(ctx->status_fd, "NoNewPrivs", rep->nonewprivs, sizeof rep->nonewprivs))
		return -1;
	return 0;
}

int sb_check(const char *name, const struct sb_ctx *ctx, struct sb_report *rep)
{
	int want_seccomp = strcmp(name, "C0") != 0;
	long base = atol(rep->base_filters);
	if (load_status(ctx, rep))
		return 0;
	if (want_seccomp)
		return !strcmp(rep->seccomp, "2") && atol(rep->seccomp_filters) == base + 1;
	return !strcmp(rep->seccomp, "0") && atol(rep->seccomp_filters) == base;
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
