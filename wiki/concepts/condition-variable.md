---
title: Condition Variable
type: concept
sources: [ostep]
tags: [concurrency]
---

# Condition Variable

A queue where a thread sleeps, releasing its lock, until another thread signals that a state condition may have changed; always paired with a lock and a state variable.

## Mechanisms

- wait(cond, mutex) atomically releases the mutex and sleeps; on wake it reacquires the lock before returning (ostep p. 381-382)
- Spinning on a flag for parent-waits-for-child is correct but wastes CPU; signaling without a state variable loses wakeups (ostep p. 380, 383)
- Always check the condition in a while loop, never an if, because of Mesa semantics (signal is only a hint) (ostep p. 387-388)
- Producer/consumer bounded buffer: single CV breaks with multiple consumers; use two CVs (empty, fill) so producers wake consumers only (ostep p. 384-391)
- Larger buffer with count/MAX allows concurrent produce/consume and fewer context switches (ostep p. 391-392)
- Covering condition: pthread_cond_broadcast to wake all waiters when you cannot tell who should run (memory allocator example) (ostep p. 393-394)

## Trade-offs

- Efficient blocking and correct ordering, but wakeups are hints, so every waiter must re-validate state.

## Agentic OS relevance

- (interpretation) Agent work queues and "wait for result/approval" steps map directly: bounded queue with producer/consumer agents, wake on state change, re-check state on wake because another agent may have taken the work.

## Related

- [[lock]]
- [[semaphore]]
- [[atomicity-and-ordering-violations]]
- [[pthreads-api]]
