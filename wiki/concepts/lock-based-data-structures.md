---
title: Lock-Based Concurrent Data Structures
type: concept
sources: [ostep]
tags: [concurrency]
---

# Lock-Based Concurrent Data Structures

Making counters, lists, queues, and hash tables thread-safe by adding locks; where and how many locks determines correctness and scalability.

## Mechanisms

- Simple counter with one lock is correct but does not scale; perfect scaling would keep multi-thread time near one-thread time (ostep p. 363-365)
- Approximate (sloppy) counter: per-CPU local counters plus global, flushed when local hits threshold S; large S gives speed but lag up to CPUs*S (ostep p. 365-368)
- Linked list with one lock held across the whole operation; keep the lock only around the critical section and avoid unlock-on-error bugs (ostep p. 369-371)
- Hand-over-hand (lock coupling) lists: per-node locks, usually slower than one lock due to acquire/release cost (ostep p. 371-372)
- Michael-Scott queue: separate head and tail locks plus a dummy node so enqueue and dequeue run concurrently (ostep p. 372-373)
- More concurrency is not necessarily faster; start simple, optimize only when needed; big kernel lock history (ostep p. 374-376)

## Trade-offs

- Finer-grained locking buys parallelism and costs complexity, overhead, and deadlock/bug risk.

## Agentic OS relevance

- (interpretation) Shared memory/state stores for agents should shard by key (per-entity locks, local buffers merged later) rather than one global lock that serializes all agents.

## Related

- [[lock]]
- [[deadlock]]
- [[race-condition]]
