/*
 * bpflsm_cg.bpf.c: HARN-03 per-cgroup BPF-LSM programs (BPF_LSM_CGROUP, section prefix lsm_cgroup/).
 *
 * Return convention (ASSUMED from the kernel documentation, not verified here): 1 allows, 0 denies, and
 * a deny yields -EPERM unless the program calls bpf_set_retval first. That is the reverse of the global
 * programs in bpflsm.bpf.c, which return 0 to allow and a negative errno to deny.
 *
 * Identity here is the attachment, not a map lookup: a program runs for tasks in the cgroup it is
 * attached to (and, ASSUMED, its descendants, which bpflsm.sh --harn03 records as lsm_cgroup_inherits).
 * So the allow programs hold no policy map and only return 1. That makes the timed cost the cost of the
 * per-cgroup attachment plus an empty program, which is what HARN-03 asks about.
 *
 * Constraints met here: non-sleepable programs only, and at most 10 distinct hooks (CGROUP_LSM_NUM).
 * This file declares 3 hooks by default and 4 with socket_sendmsg. cg_deny_file_open shares the
 * file_open hook with cg_allow_file_open, so it does not add a hook.
 *
 * cg_deny_file_open is attached to the probe cgroup only, never to the root or the session cgroup: a
 * deny on file_open locks out every task in the cgroup it is attached to.
 *
 * Authored on a host with no BPF toolchain and not compiled or loaded there. The attach itself is the
 * ADR 0004 decision 2 test and runs on the reference host (08-04 probe, 08-05 measurement).
 */
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

SEC("lsm_cgroup/file_open")
int BPF_PROG(cg_allow_file_open, struct file *file)
{
	return 1;
}

SEC("lsm_cgroup/socket_connect")
int BPF_PROG(cg_allow_socket_connect, struct socket *sock, struct sockaddr *address, int addrlen)
{
	return 1;
}

SEC("lsm_cgroup/bprm_check_security")
int BPF_PROG(cg_allow_bprm_check_security, struct linux_binprm *bprm)
{
	return 1;
}

/* Built only with make CG_SENDMSG=1, which defines AOK_CG_SENDMSG (OQ7, D-10; default off). */
#ifdef AOK_CG_SENDMSG
SEC("lsm_cgroup/socket_sendmsg")
int BPF_PROG(cg_allow_socket_sendmsg, struct socket *sock, struct msghdr *msg, int size)
{
	return 1;
}
#endif

/* Returns 0, expected to surface as EPERM in the cgroup it is attached to. */
SEC("lsm_cgroup/file_open")
int BPF_PROG(cg_deny_file_open, struct file *file)
{
	return 0;
}
