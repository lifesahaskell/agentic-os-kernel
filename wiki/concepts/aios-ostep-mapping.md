---
title: AIOS to OSTEP Concept Mapping
type: concept
sources: [aios, ostep]
tags: [synthesis, agentic]
---

# AIOS to OSTEP Concept Mapping

The bridge page of the wiki. [[aios|AIOS]] borrows classical OS vocabulary for its LLM kernel.
This page pairs each AIOS module with the [[ostep|OSTEP]] concept it borrows from, and records
**where the analogy breaks**. The breaks are what the agentic OS design has to resolve. The left
two columns are sourced; the "breaks" column is analysis *(interpretation)* unless it cites a page.

| OSTEP concept | AIOS counterpart | Where the analogy breaks |
|---|---|---|
| [[process]] | [[agent-as-process]] | AIOS agents have ids, status, priority, suspend/resume (aios p. 7), but no address space, no privilege mode, and no parent/child tree. The paper never calls an agent a process. |
| [[scheduling]], [[mlfq]], [[proportional-share-scheduling]] | [[agent-scheduler]] | The schedulable unit is one LLM request, not a time slice (aios p. 5). Only FIFO was measured (aios p. 9). MLFQ learns a job's length from how it used its last slice (ostep pp. 106-111). An LLM request's length is its output token count, which is unknown until generation stops. |
| [[context-switch]] | [[context-snapshot-restore]] | A CPU switch saves a fixed, small register set (ostep pp. 83-86). An LLM snapshot saves decoding state, a beam-search tree (aios p. 6), whose size grows with the model and the context. AIOS reports no snapshot size or latency. |
| [[address-space]], [[paging]], [[swapping]] | [[context-window-management]], [[memory-manager]] | Paging moves exact bytes out and back (ostep p. 262). AIOS handles overflow by summarization (aios p. 6), which is lossy and cannot be paged back in. The context window is a fixed physical memory with no MMU and no fault. |
| [[working-set-and-thrashing]] | none | When working sets exceed memory, OSTEP's answer is admission control: run fewer processes (ostep p. 282). AIOS has no admission control. Agents oversubscribing context or GPU memory is the same failure. |
| [[system-call]], [[limited-direct-execution]] | [[llm-system-calls]], [[aios-sdk]] | Protection in OSTEP comes from restricted mode plus traps: kernel mode is entered only through the trap table (ostep pp. 77-78). AIOS syscalls are library calls into a Python service with no mode switch (aios p. 4). Nothing stops an agent from calling the model or a tool directly. |
| [[io-devices-and-drivers]] | [[tool-manager]] | Tools are remote APIs and local models with no uniform driver interface (aios p. 7). Table 2 has no tool syscalls, so tool calls are not mediated (aios p. 7). |
| [[file-system-implementation]], [[crash-consistency]], [[fsync-and-durability]] | [[storage-manager]] | AIOS names files, databases and cloud stores (aios pp. 6-7) but gives no consistency or durability model. OSTEP shows that a multi-block update is not atomic without journaling or careful ordering (ostep p. 565). |
| [[access-control]], [[capabilities]], [[access-control-lists]] | [[access-manager]] | AIOS has per-agent privilege groups and audit logs (aios pp. 7-8), but no access syscalls (aios p. 7), no user-vs-agent principal, and no delegation. In OSTEP, capabilities kept in the kernel make delegation and revocation natural (ostep pp. 733-734). |
| [[trusted-computing-base]] | the LLM itself | The TCB "can do anything" and must be small (ostep pp. 695-696, 699-700). In AIOS, the model's output steers scheduling-relevant behavior and tool selection. A component whose output an attacker can influence through its input cannot be in the TCB. |
| [[setuid-and-privilege-escalation]] | agent acting for a user | Attackers fool legitimate users into misusing the system, and the damage scales with the victim's privileges (ostep p. 702). *(interpretation)* An agent that holds a user's privileges while reading untrusted content (web pages, email) is a user that can be fooled at machine speed, which is a confused deputy. This is where prompt injection fits. |
| [[virtual-machine-monitor]] | none | A VMM runs an unmodified OS deprivileged and traps its privileged operations (ostep pp. 783-785). This is one candidate shape for mediating an unmodified agent's actions. |

## Design consequences (interpretation)

1. **The model is a device, not the kernel.** Treat inference as a scheduled, shared device like a
   disk: requests queue, are batched, and complete. Keep policy decisions (who may do what) in
   deterministic code inside the TCB. AIOS's framing of the LLM as the OS's "brain" (aios p. 1)
   is the framing to reject for the security core.
2. **Mediation needs a real boundary.** AIOS's modules are advisory because agents run in the same
   trust domain. A natively agentic OS gets protection the way OSTEP's kernel does: agents run
   deprivileged, and every tool call, file write and network request traps into a mediator.
3. **Context is memory and needs the memory toolbox:** admission control, eviction policy, a
   working-set notion, and accounting per principal.
4. **Snapshots are context switches, and their cost must be measured.** Preemptive agent
   scheduling is only as cheap as snapshot/restore. AIOS gives no numbers for that cost.
5. **Capabilities fit delegation.** An agent spawning a sub-agent with a subset of its rights is
   what capabilities handle natively (ostep pp. 733-734), and ACLs handle badly.

## Outside both sources

These topics come up in the design but neither source covers them; each needs its own ingest:
KV-cache paging (PagedAttention), prompt-injection literature, capability OSes (seL4, Fuchsia
handles, Capsicum), and later AIOS versions.

## Related

- [[aios-architecture]]
- [[virtualization-and-resource-management]]
- [[mechanism-vs-policy]]
- [[principle-of-least-privilege]]
- [[isolation-and-protection]]
- [[agent-recursion-os-implications]]
