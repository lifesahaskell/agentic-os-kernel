---
title: Semaphore
type: concept
sources: [ostep]
tags: [concurrency]
---

# Semaphore

An integer-valued object with sem_wait (decrement, block if negative) and sem_post (increment, wake a waiter) that works as both lock and condition variable.

## Mechanisms

- Dijkstra's P()/V(); negative value equals the number of waiting threads (ostep p. 398-399)
- Initial value 1 gives a binary semaphore used as a lock (ostep p. 400-401)
- Initial value 0 lets a parent wait for a child (ordering); rule: initial value equals the number of resources to give away immediately (ostep p. 401-404)
- Bounded buffer with empty/full semaphores plus a mutex; putting the mutex outside the waits causes deadlock (ostep p. 404-407)
- Reader-writer lock: first reader takes writelock, last releases it; readers can starve writers (ostep p. 408-409)
- Thread throttling: initialize to N to cap concurrent entrants (e.g., memory-hungry sections) (ostep p. 413)
- Semaphores can be built from locks and CVs (Zemaphore); the reverse is much harder (ostep p. 413-415)

## Trade-offs

- One primitive covers locking, ordering, and counting, but the meaning lives in the initial value and the call placement, which is easy to get wrong.

## Agentic OS relevance

- (interpretation) A counting semaphore is the natural rate limiter for N agents sharing a model endpoint, tool quota, or sandbox pool (throttling); a reader-writer lock fits many-readers/one-writer shared knowledge files.

## Related

- [[lock]]
- [[condition-variable]]
- [[deadlock]]
- [[reader-writer-lock]]
