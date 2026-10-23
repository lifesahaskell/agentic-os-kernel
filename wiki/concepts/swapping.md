---
title: Swapping and Page Faults
type: concept
sources: [ostep]
tags: [virtualization]
---

# Swapping and Page Faults

Treating disk as a larger, slower backing store so the sum of address spaces can exceed physical memory.

## Mechanisms

- Reserved swap space; code pages can be backed by the binary itself (ostep p. 259-260).
- Present bit in the PTE; an access to a non-present page raises a page fault (ostep p. 260-261).
- The OS, not hardware, handles faults in software because disk latency dominates (ostep p. 262).
- The faulting process blocks while others run, overlapping I/O with compute (ostep p. 262).
- Page-fault control flow: find free frame, issue read, update PTE, retry (ostep p. 263-264).
- Page daemon with high/low watermarks evicts in the background; writes are clustered (ostep p. 264-265).

## Trade-offs

- Capacity illusion in exchange for a roughly 10^5x latency penalty on a fault.

## Agentic OS relevance

- (interpretation) Evict idle agent contexts to cheap storage, rehydrate on demand; run other agents while one waits on rehydration; keep a free-budget watermark with background eviction.

## Related

- [[demand-paging]]
- [[page-replacement-policy]]
- [[working-set-and-thrashing]]
