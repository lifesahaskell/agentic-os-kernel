---
title: Thread
type: concept
sources: [ostep]
tags: [concurrency]
---

# Thread

A second point of execution inside one process: own PC, registers and stack, but a shared address space and heap. Context switching between threads saves state to a thread control block (TCB) and does not swap page tables.

## Mechanisms

- Each thread has private registers and PC; switching T1 to T2 saves/restores registers into a TCB, but the address space stays the same (ostep p. 313)
- One stack per thread; stack-allocated variables are "thread-local storage", so a multi-threaded address space holds several stacks (ostep p. 314)
- Two reasons to use threads: parallelism across CPUs, and overlapping computation with blocking I/O (ostep p. 315)
- Threads are chosen over processes when data sharing is needed; processes for logically separate tasks (ostep p. 315)
- Creation resembles a call that runs independently; which thread runs next is the scheduler's choice, so B may print before A (ostep p. 316, 318)

## Trade-offs

- Cheap shared-memory sharing and I/O overlap, paid for with nondeterministic interleavings.

## Agentic OS relevance

- (interpretation) Agents sharing a process/context are threads: shared state is easy, and every shared tool handle or file becomes a hazard; separate processes suit agents that need isolation.

## Related

- [[pthreads-api]]
- [[race-condition]]
- [[event-based-concurrency]]
