/*
 * probe.bpf.c: the D-04 fire probe for bpflsm.sh --probe.
 *
 * One global lsm/file_open program. It adds 1 to slot 0 of a one-slot PERCPU_ARRAY and returns 0, so
 * loading it can never deny anything. bpflsm.sh --probe reads fire_count around one open and writes
 * the difference to env.txt as lsm_hook_fires. Attaching proves nothing on a kernel whose active LSM
 * list lacks bpf (the program attaches and never runs), so the counter is the evidence, not the attach.
 *
 * Authored on a host with no BPF toolchain and not compiled or loaded there: the first compile and
 * load happen on the reference host (08-04).
 */
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

struct {
	__uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
	__uint(max_entries, 1);
	__type(key, __u32);
	__type(value, __u64);
} fire_count SEC(".maps");

SEC("lsm/file_open")
int BPF_PROG(probe_file_open, struct file *file)
{
	__u32 slot = 0;
	__u64 *c = bpf_map_lookup_elem(&fire_count, &slot);

	if (c)
		*c += 1;
	return 0;
}
