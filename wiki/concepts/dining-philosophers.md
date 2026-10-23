---
title: Dining Philosophers
type: concept
sources: [ostep]
tags: [concurrency]
---

# Dining Philosophers

Dijkstra's problem of five philosophers each needing two shared forks to eat, used to show how naive resource acquisition deadlocks.

## Mechanisms

- Goal: no deadlock, no starvation, high concurrency (ostep p. 410-411)
- Naive solution: each grabs left then right with a semaphore per fork (ostep p. 411)
- Deadlock occurs if every philosopher grabs the left fork first (ostep p. 412)
- Fix: break the cycle by making one philosopher acquire right before left (ostep p. 412)
- Equivalent to imposing a global order on resource acquisition (ostep p. 412)

## Trade-offs

- Breaking symmetry removes deadlock but leaves the system reliant on an ordering convention.

## Agentic OS relevance

- (interpretation) Agents each needing two exclusive tools or files (e.g., repo + deploy slot) hit exactly this cycle unless acquisition order is fixed.

## Related

- [[deadlock]]
- [[semaphore]]
