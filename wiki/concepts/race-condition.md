---
title: Race Condition and Critical Section
type: concept
sources: [ostep]
tags: [concurrency]
---

# Race Condition and Critical Section

A data race where results depend on the timing of thread execution; the code that touches shared state this way is a critical section needing mutual exclusion.

## Mechanisms

- Two threads incrementing a shared counter: load, add, store are three instructions, so a timer interrupt between them loses an update (ostep p. 318-321)
- Result is indeterminate: different runs produce different outputs (ostep p. 322)
- A critical section accesses a shared variable and must not be executed concurrently; mutual exclusion guarantees only one thread is inside (ostep p. 322)
- The wish for atomicity: ideally a "super instruction" executing all-or-nothing, like a database transaction (ostep p. 323)
- Hardware gives a few atomic primitives; the OS builds locks, sleep/wake on top of them (ostep p. 323-324)

## Trade-offs

- Correctness requires serializing exactly the shared-state window; too wide kills parallelism, too narrow races.

## Agentic OS relevance

- (interpretation) Two agents doing read-modify-write on the same file or ticket field produce lost updates identical to the counter example.

## Related

- [[atomic-instructions]]
- [[lock]]
- [[atomicity-and-ordering-violations]]
