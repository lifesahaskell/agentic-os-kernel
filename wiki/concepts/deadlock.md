---
title: Deadlock
type: concept
sources: [ostep]
tags: [concurrency]
---

# Deadlock

A cycle of threads each holding a lock another needs; occurs only when mutual exclusion, hold-and-wait, no preemption, and circular wait all hold.

## Mechanisms

- Example: T1 holds L1 wants L2, T2 holds L2 wants L1; a cycle in the dependency graph (ostep p. 422-423)
- Causes: component dependencies (VM and file system), and encapsulation hiding locks (e.g., vector add with v1.add(v2)) (ostep p. 423-424)
- Four conditions (Coffman): mutual exclusion, hold-and-wait, no preemption, circular wait (ostep p. 424)
- Prevention: total or partial lock ordering; address-based ordering for lock arguments; Linux mm uses partial orders (ostep p. 424-425)
- Prevent hold-and-wait with a global prevention lock acquired first; prevent no-preemption with trylock and back-off (risks livelock) (ostep p. 425-427)
- Avoid mutual exclusion with lock-free structures via compare-and-swap (ostep p. 427-429)
- Avoidance by scheduling (Banker's algorithm) limits concurrency and needs knowledge of lock use; detect-and-recover as used by databases (ostep p. 429-431)

## Trade-offs

- Lock ordering is cheap and practical, but it is a convention that depends on global code understanding; avoidance and detection cost concurrency or restarts.

## Agentic OS relevance

- (interpretation) Agents holding one tool/file lock while requesting another across a shared environment deadlock silently; a global acquisition order, trylock with timeout and backoff, and a periodic cycle detector are the portable fixes.

## Related

- [[lock]]
- [[dining-philosophers]]
- [[lock-free-synchronization]]
- [[semaphore]]
