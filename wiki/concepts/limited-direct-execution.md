---
title: Limited Direct Execution (LDE)
type: concept
sources: [ostep]
tags: [virtualization]
---

# Limited Direct Execution (LDE)

Run programs directly on the CPU for speed, but with hardware-enforced limits and OS intervention at key points so the OS keeps control.

## Mechanisms

- Boot: install trap table, start interrupt timer (ostep pp. 79, 177)
- Run: kernel allocates process entry, sets up stack, return-from-trap into the program (ostep pp. 79, 85)
- Restricted operations handled by system calls; kernel mode is entered only through traps (ostep pp. 77-78)
- Cooperative scheduling relies on yields and syscalls; non-cooperative scheduling uses the timer interrupt (ostep pp. 81-83)
- Same pattern reused for memory: translation by hardware, OS steps in only on faults (ostep pp. 167, 177)

## Trade-offs

- Efficiency (get out of the way) versus control (be able to stop anything).

## Agentic OS relevance

- (interpretation) Let an agent call the model directly with no per-token supervision, but wrap it with enforced limits (token budget, tool allowlist, deadline) and a supervisor that can preempt.

## Related

- [[trap-and-interrupt]]
- [[context-switch]]
- [[system-call]]
- [[address-translation]]
