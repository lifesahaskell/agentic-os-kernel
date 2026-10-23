---
title: Lock (Mutex)
type: concept
sources: [ostep]
tags: [concurrency]
---

# Lock (Mutex)

A variable that is either free or held, giving mutual exclusion over a critical section; judged by correctness, fairness, and performance.

## Mechanisms

- lock()/unlock() bracket the critical section; one owner at a time, others wait (ostep p. 341-342)
- Evaluation criteria: mutual exclusion, fairness (starvation), performance overhead single-CPU and multi-CPU (ostep p. 343)
- Disabling interrupts works on one CPU but requires trusting callers, fails on multiprocessors, can lose interrupts (ostep p. 344-345)
- Spinning vs sleeping: yield, queues with park()/unpark(), Linux futex wait/wake (ostep p. 355-359)
- Two-phase lock: spin briefly then sleep; Linux's futex lock is a form of it (ostep p. 360)
- Priority inversion (Mars Pathfinder): low-priority lock holder starves a high-priority waiter; fixed via priority inheritance (ostep p. 357)

## Trade-offs

- Simple model, but every design choice (spin, sleep, queue) trades CPU waste against latency and fairness.

## Agentic OS relevance

- (interpretation) A lock on a shared repo/tool is cheap protection but a stalled or slow holder blocks every waiting agent, and a low-priority holder can block a high-priority agent.

## Related

- [[spin-lock]]
- [[deadlock]]
- [[condition-variable]]
- [[semaphore]]
- [[atomic-instructions]]
