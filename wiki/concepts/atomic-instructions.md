---
title: Atomic Hardware Instructions
type: concept
sources: [ostep]
tags: [concurrency]
---

# Atomic Hardware Instructions

Single indivisible CPU instructions (test-and-set, compare-and-swap, load-linked/store-conditional, fetch-and-add) that are the foundation of working locks on multiprocessors.

## Mechanisms

- Test-and-set returns the old value and sets the new one atomically; it enables a working spin lock (ostep p. 346, 348)
- Compare-and-swap writes only if memory equals an expected value; strictly more powerful, and the basis of lock-free code (ostep p. 350-351, 427-428)
- Load-linked / store-conditional: the store succeeds only if no intervening store occurred to the address (ostep p. 351-352)
- Fetch-and-add atomically increments and returns the old value; used for ticket locks (ostep p. 352-354)
- Software-only alternatives (Dekker, Peterson) exist but are not used in practice with modern memory systems (ostep p. 347)
- Load/store flag locks fail because test and set are separate steps (ostep p. 345-346)

## Trade-offs

- Hardware atomicity is cheap and sufficient but only on single words; everything bigger is built on top.

## Agentic OS relevance

- (interpretation) The analog is an atomic compare-and-set on a version field or a file rename; agents need one such primitive to build safe coordination.

## Related

- [[spin-lock]]
- [[lock]]
- [[lock-free-synchronization]]
