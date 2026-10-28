/*
 * bpfadv.bpf.c: the ADV-03 / S5 deny programs. Global lsm/ programs, loaded by bpflsm.sh --adv agent.
 *
 * Recorded S5 deny rule (a Claude's-discretion choice, reported with the ADV-03 results):
 *
 *     deny with -13 (EACCES) when agent_cg[current cgroup] = a and subtree[target cgroup] is not a
 *
 * agent_cg maps a cgroup id to the agent number running in it (0 or absent: not an agent). subtree maps
 * a cgroup id to the agent number that owns it; a target cgroup with no entry belongs to no agent and
 * is therefore not agent a's. bpflsm.sh keys agent-a as 1 and agent-b as 2 in both maps and leaves the
 * broker and nonagent cgroups out. Only the exact cgroup id is matched, not its descendants: the agent
 * cgroups are leaves in this phase.
 *
 * Hooks and what each one stands for:
 *   task_kill            kill, tgkill, rt_sigqueueinfo, pidfd_send_signal. Target = the signalled task.
 *   ptrace_access_check  PTRACE_ATTACH, PTRACE_SEIZE, process_vm_readv/writev, /proc/pid/mem and
 *                        pidfd_getfd, which all reach ptrace_may_access. Target = the child task.
 *   socket_sendmsg       route (ii), an SCM_RIGHTS descriptor: the sending socket's creator cgroup is
 *                        compared with the sender's, so an fd made in agent-b and sent by agent-a is
 *                        denied.
 *   socket_bind          denies any AF_UNIX bind from the agent set (the pathname and abstract bind
 *                        routes). No target rule: an agent never binds a unix socket.
 *   file_open            COUNT ONLY, never denies. It counts opens from the agent set and stands in as
 *                        the counter for the cgroup.kill and cgroup.procs routes (A11, Pitfall 5).
 *
 * A11 (assumed, not measured): writing cgroup.kill goes through __cgroup_kill and send_sig, which is
 * likely to skip security_task_kill. So the deny counter for the cgroup.kill route is a file-hook
 * counter (the open of the cgroup.kill file), not a task_kill counter. The same holds for the
 * cgroup.procs route. A zero in slot 0 on a cgroup.kill row says the file-hook count is the evidence.
 *
 * Target cgroup ids are BTF direct reads (A8, assumed): task->cgroups->dfl_cgrp->kn->id for a task and
 * sock->sk->sk_cgrp_data.cgroup->kn->id for a socket. There is no bpf_probe_read_kernel. A null link in
 * the chain reads as 0, which has no subtree entry and so counts as not the agent's. The sock_cgroup_data
 * layout (a plain cgroup pointer) is assumed for the reference kernel; a different layout fails at
 * compile time on the reference host, not silently.
 *
 * adv_count is a PERCPU_ARRAY bumped with a plain add. Slots: 0 task_kill denials, 1 ptrace_access_check
 * denials, 2 socket_sendmsg denials, 3 socket_bind denials, 4 file_open opens from the agent set.
 * bpflsm.sh reads the five slots before and after each route row and writes deny-counters.txt.
 *
 * Authored on a host with no BPF toolchain and not compiled or loaded there. The verifier accepting
 * these chains is ASSUMED; the first compile and load are on the reference host (08-05).
 */
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

#define AOK_EACCES 13
#define AOK_AF_UNIX 1

char LICENSE[] SEC("license") = "GPL";

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 64);
	__type(key, __u64);
	__type(value, __u32);
} agent_cg SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 64);
	__type(key, __u64);
	__type(value, __u32);
} subtree SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
	__uint(max_entries, 8);
	__type(key, __u32);
	__type(value, __u64);
} adv_count SEC(".maps");

static __always_inline void bump(__u32 slot)
{
	__u64 *c = bpf_map_lookup_elem(&adv_count, &slot);

	if (c)
		*c += 1;
}

/* The agent number of the calling task's cgroup, 0 when the caller is not in the agent set. */
static __always_inline __u32 current_agent(void)
{
	__u64 id = bpf_get_current_cgroup_id();
	__u32 *a = bpf_map_lookup_elem(&agent_cg, &id);

	return a ? *a : 0;
}

/* The S5 rule for one target cgroup id. Counts the denial in slot. */
static __always_inline int rule(__u32 slot, __u32 agent, __u64 target)
{
	__u32 *owner = bpf_map_lookup_elem(&subtree, &target);

	if (owner && *owner == agent)
		return 0;
	bump(slot);
	return -AOK_EACCES;
}

SEC("lsm/task_kill")
int BPF_PROG(adv_task_kill, struct task_struct *p, struct kernel_siginfo *info, int sig,
	     const struct cred *cred)
{
	__u32 agent = current_agent();

	if (agent == 0)
		return 0;
	return rule(0, agent, p->cgroups->dfl_cgrp->kn->id);
}

SEC("lsm/ptrace_access_check")
int BPF_PROG(adv_ptrace_access_check, struct task_struct *child, unsigned int mode)
{
	__u32 agent = current_agent();

	if (agent == 0)
		return 0;
	return rule(1, agent, child->cgroups->dfl_cgrp->kn->id);
}

SEC("lsm/socket_sendmsg")
int BPF_PROG(adv_socket_sendmsg, struct socket *sock, struct msghdr *msg, int size)
{
	__u32 agent = current_agent();

	if (agent == 0)
		return 0;
	return rule(2, agent, sock->sk->sk_cgrp_data.cgroup->kn->id);
}

SEC("lsm/socket_bind")
int BPF_PROG(adv_socket_bind, struct socket *sock, struct sockaddr *address, int addrlen)
{
	if (current_agent() == 0)
		return 0;
	if (address->sa_family == AOK_AF_UNIX) {
		bump(3);
		return -AOK_EACCES;
	}
	return 0;
}

/* Count only. See the A11 note at the top of the file. */
SEC("lsm/file_open")
int BPF_PROG(adv_file_open, struct file *file)
{
	if (current_agent())
		bump(4);
	return 0;
}
