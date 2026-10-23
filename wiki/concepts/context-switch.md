---
title: Context Switch
type: concept
sources: [ostep]
tags: [virtualization]
---

# Context Switch

Saving the running process's register state and restoring another's so the CPU resumes the second as if it had never stopped.

## Mechanisms

- Two register save/restore events: hardware saves user registers to the kernel stack on trap; the OS saves kernel registers to the proc struct in swtch() (ostep pp. 83-86)
- swtch switches kernel stacks, then return-from-trap resumes the new process (ostep p. 86)
- Decision to switch is a policy (scheduler); the switch is a mechanism (ostep pp. 83-84)
- Costs: direct (register copy) and indirect (cold caches, TLB) (ostep p. 87)
- Base/bounds registers are also saved and restored per process (ostep pp. 176, 178)

## Trade-offs

- Frequent switches improve responsiveness and fairness but waste time on overhead.

## Agentic OS relevance

- (interpretation) Switching agents means reloading its context into the model; if the prefix is uncached, the switch costs a full re-read of tokens, so switch cost scales with context size, not register count.

## Related

- [[scheduling]]
- [[trap-and-interrupt]]
- [[process]]
- [[limited-direct-execution]]
