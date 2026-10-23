---
title: Lock-Free Synchronization
type: concept
sources: [ostep]
tags: [concurrency]
---

# Lock-Free Synchronization

Updating shared structures with atomic compare-and-swap retry loops so no lock is held, removing deadlock but not livelock.

## Mechanisms

- AtomicIncrement loops CAS(value, old, old+amount) until it succeeds (ostep p. 427)
- List insert: set n->next = head, CAS(&head, n->next, n), retry if another thread won (ostep p. 428-429)
- Wait-free variants guarantee a finite number of steps but are harder; building full lists/queues lock-free is nontrivial (ostep p. 429, 431)
- Livelock remains possible (ostep p. 428)
- Authors expect limited adoption; new models like MapReduce avoid locks differently (ostep p. 431)

## Trade-offs

- No blocking or deadlock, at the price of retry loops and much harder correctness reasoning.

## Agentic OS relevance

- (interpretation) Optimistic concurrency (read version, write if unchanged, retry) for shared agent state is the same pattern and avoids lock holders stalling others.

## Related

- [[atomic-instructions]]
- [[deadlock]]
- [[lock]]
