# Wiki Index

Entry point. Pages are listed in the per-topic files under Groups. Start with [[aios-ostep-mapping]]; it bridges the two sources and lists the open design problems.

Where to start, by question (read the named page, then follow its `## Related`):

- OS concept for agents (scheduling, memory, access): [[aios-ostep-mapping]].
- Self-improving or self-rewriting agents, grader isolation: [[agent-recursion-os-implications]], then [[recursive-self-improvement-loop]].
- Multi-agent latent communication, cost of rounds: [[agent-recursion-os-implications]], then [[recursive-multi-agent-system]].
- Protection and authority: [[capabilities]], [[access-control]], [[trusted-computing-base]].
- Spike candidates and hardware (outside the wiki): `docs/architecture/research/recursion-and-cheap-inference-spikes.md`.

## Sources

- [[aios]]: AIOS: LLM Agent Operating System (arXiv 2403.16971v2, 2024)
- [[ostep]]: Operating Systems: Three Easy Pieces (Arpaci-Dusseau)
- [[recursivemas]]: Recursive Multi-Agent Systems (arXiv 2604.25917v1, 2026)
- [[aide2]]: Recursive self-improvement of AI research agents (arXiv 2609.26457v1, 2026)

## Synthesis (2)

- [[aios-ostep-mapping|AIOS to OSTEP Concept Mapping]]: The bridge page of the wiki.
- [[agent-recursion-os-implications|Agent Recursion and OS Implications]]: What latent-space agent loops and self-rewriting agents ask of a kernel that treats agents as objects; all interpretation.

## Groups

One file per topic, each lists its pages with a one-line definition. Read the group that matches the question, not all of them.

- [[index-aios|AIOS (agentic)]] (13): agent scheduling, context snapshots, the AIOS kernel modules.
- [[index-agent-research|Agent systems research]] (11): recursive agents, held-out grading, reward hacking, latent communication.
- [[index-virtualization|Virtualization]] (28): processes, scheduling, address spaces, paging, swapping.
- [[index-concurrency|Concurrency]] (15): locks, condition variables, semaphores, concurrency bugs.
- [[index-persistence|Persistence]] (15): disks, file systems, journaling, crash consistency.
- [[index-distribution|Distribution]] (6): RPC, NFS, AFS, cache consistency, idempotency.
- [[index-security|Security]] (12): access control, capabilities, TCB, privilege escalation.
