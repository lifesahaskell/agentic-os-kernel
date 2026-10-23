---
title: Atomicity and Order Violations
type: concept
sources: [ostep]
tags: [concurrency]
---

# Atomicity and Order Violations

The two patterns making up 97% of non-deadlock concurrency bugs in the Lu et al. study: code intended to be atomic is interleaved, or an intended ordering between threads is not enforced.

## Mechanisms

- Study: 105 bugs in MySQL, Apache, Mozilla, OpenOffice; 74 non-deadlock, 31 deadlock (ostep p. 418-419)
- Atomicity violation: check for non-NULL then use, while another thread sets NULL; fix with a lock around both accesses (ostep p. 419-420)
- Order violation: thread 2 reads mThread before thread 1 initializes it; desired order A before B is flipped (ostep p. 420-421)
- Fix for ordering: a condition variable plus a state variable (mtInit) so the waiter checks or waits (ostep p. 421-422)
- 97% of non-deadlock bugs are one of these two, so look for them first (ostep p. 422)

## Trade-offs

- Easy to fix once recognized, hard to notice because the window is tiny and rarely hit in testing.

## Agentic OS relevance

- (interpretation) Check-then-act on shared state (file exists, then write) is an atomicity violation; an agent reading a result before another agent wrote it is an order violation; both appear intermittently.

## Related

- [[race-condition]]
- [[lock]]
- [[condition-variable]]
- [[deadlock]]
