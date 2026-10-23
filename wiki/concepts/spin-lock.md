---
title: Spin Lock (and Ticket Lock)
type: concept
sources: [ostep]
tags: [concurrency]
---

# Spin Lock (and Ticket Lock)

A lock whose waiters busy-loop on an atomic instruction; correct but wasteful when the holder is descheduled, and unfair unless built as a ticket lock.

## Mechanisms

- Test-and-set loop: while (TestAndSet(&flag,1)==1) ; (ostep p. 348)
- Correct but not fair; a waiter may spin forever (ostep p. 349)
- On one CPU spinning is pure waste until a preempting timer tick; with N threads, N-1 slices wasted (ostep p. 349, 355)
- Ticket lock: fetch-and-add a ticket, spin until turn equals your ticket; guarantees progress and FIFO fairness (ostep p. 353-354)
- Yield instead of spin helps for few threads but still costs a context switch and can starve (ostep p. 355)
- Spin locks can hang a system under priority inversion (ostep p. 357)

## Trade-offs

- Lowest latency when the hold time is shorter than a context switch, but burns CPU and can livelock the rest otherwise.

## Agentic OS relevance

- (interpretation) Polling loops that retry a busy resource are spin locks; with many agents, bounded backoff or queued waiting beats burning inference calls spinning.

## Related

- [[lock]]
- [[atomic-instructions]]
- [[condition-variable]]
