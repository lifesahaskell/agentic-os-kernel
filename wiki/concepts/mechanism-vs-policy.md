---
title: Mechanism versus Policy
type: concept
sources: [ostep]
tags: [virtualization]
---

# Mechanism versus Policy

Design rule that low-level machinery (how to do something) is kept separate from the decision logic (which thing to do), so policies can change without rebuilding mechanisms.

## Mechanisms

- Stated as a tip: context switch is mechanism, scheduler is policy (ostep p. 49)
- Scheduling policies (FIFO, SJF, MLFQ, CFS) all sit atop the same switch machinery (ostep pp. 83-84, 92-127)
- Advice interfaces (nice, hints) let users influence policy without changing it (ostep pp. 112-113)
- Tunable constants ("voo-doo") are the policy surface; Ousterhout's Law warns against them (ostep p. 112)

## Trade-offs

- Separation gives flexibility but pushes hard parameter choices onto administrators.

## Agentic OS relevance

- (interpretation) Keep the agent runtime (spawn, pause, resume, budget accounting) free of scheduling decisions so priority and quota policies can be swapped independently.

## Related

- [[scheduling]]
- [[context-switch]]
- [[mlfq]]
