/*
 * bpflsm.bpf.c: HARN-02 global BPF-LSM programs. One source, three objects chosen by macro:
 *
 *   bpflsm.bpf.o            default: counters on, key = the current cgroup id
 *   bpflsm_nocount.bpf.o    -DAOK_NO_COUNTER: hook_count and hit_count compiled out (the counter-bias
 *                           spot check; its validity rests on the denial proof alone)
 *   bpflsm_anc.bpf.o        -DAOK_KEY_ANCESTOR: key = the ancestor id at depth AOK_ANC_LEVEL (the D-11
 *                           fallback, measured only if the lsm_cgroup attach fails)
 *
 * Global convention (not the lsm_cgroup one): return 0 to allow, a negative errno to deny. A deny of
 * -13 reaches the caller as EACCES. All programs are non-sleepable.
 *
 * cg_policy holds the only policy. bpflsm.sh keys it so the timed legs are never denied:
 *   - the probe cgroup is keyed to 1 for the untimed denial proof, then the entry is deleted;
 *   - global-hit and anc key the timed cgroup to 0 (an allow value), so every timed lookup hits;
 *   - global-miss and global-miss-nocount leave the map empty, so every lookup misses.
 * hook_count counts every call. hit_count counts the calls whose lookup found an entry, which is how
 * the wrapper tells the hit path from the miss path, since hook_count moves on both.
 *
 * Slots (both counters, 8 entries): 0 file_open, 1 socket_connect, 2 bprm_check_security,
 * 3 socket_sendmsg. Counters are PERCPU_ARRAYs bumped with a plain add: the entry is per CPU, so no
 * atomic is needed and none is paid for.
 *
 * Authored on a host with no BPF toolchain and not compiled or loaded there. Whether bpf_get_current_
 * ancestor_cgroup_id is available to an LSM program is ASSUMED (tracing helper set), as is the
 * program shape the verifier accepts; the first compile and load are on the reference host.
 */
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

#define AOK_EACCES 13

#ifdef AOK_KEY_ANCESTOR
#ifndef AOK_ANC_LEVEL
/* Depth of /sys/fs/cgroup/aok-phase8 under the cgroup2 root; the Makefile passes ANC_LEVEL. */
#define AOK_ANC_LEVEL 1
#endif
#define AOK_CG_KEY() bpf_get_current_ancestor_cgroup_id(AOK_ANC_LEVEL)
#else
#define AOK_CG_KEY() bpf_get_current_cgroup_id()
#endif

char LICENSE[] SEC("license") = "GPL";

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 1024);
	__type(key, __u64);
	__type(value, __u32);
} cg_policy SEC(".maps");

#ifndef AOK_NO_COUNTER
struct {
	__uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
	__uint(max_entries, 8);
	__type(key, __u32);
	__type(value, __u64);
} hook_count SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
	__uint(max_entries, 8);
	__type(key, __u32);
	__type(value, __u64);
} hit_count SEC(".maps");
#endif

static __always_inline int check(__u32 slot)
{
	__u64 id = AOK_CG_KEY();
	__u32 *v;
#ifndef AOK_NO_COUNTER
	__u64 *c = bpf_map_lookup_elem(&hook_count, &slot);

	if (c)
		*c += 1;
#endif
	v = bpf_map_lookup_elem(&cg_policy, &id);
	if (v == NULL)
		return 0;
#ifndef AOK_NO_COUNTER
	c = bpf_map_lookup_elem(&hit_count, &slot);
	if (c)
		*c += 1;
#endif
	return *v == 1 ? -AOK_EACCES : 0;
}

SEC("lsm/file_open")
int BPF_PROG(h_file_open, struct file *file)
{
	return check(0);
}

SEC("lsm/socket_connect")
int BPF_PROG(h_socket_connect, struct socket *sock, struct sockaddr *address, int addrlen)
{
	return check(1);
}

SEC("lsm/bprm_check_security")
int BPF_PROG(h_bprm_check, struct linux_binprm *bprm)
{
	return check(2);
}

SEC("lsm/socket_sendmsg")
int BPF_PROG(h_socket_sendmsg, struct socket *sock, struct msghdr *msg, int size)
{
	return check(3);
}
