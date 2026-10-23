---
title: VAX/VMS: Segmented FIFO and Second-Chance Lists
type: concept
sources: [ostep]
tags: [virtualization]
---

# VAX/VMS: Segmented FIFO and Second-Chance Lists

VMS's replacement scheme for hardware without a reference bit: per-process FIFO with global clean and dirty free lists as a second chance.

## Mechanisms

- 512B pages with 23-bit VPN, hybrid of paging and segmentation (P0, P1, S) (ostep p. 288).
- Per-segment page tables, with user page tables placed in pageable kernel virtual memory (ostep p. 288-289).
- No hardware reference bit; protection bits can emulate one via traps (ostep p. 291-292).
- Per-process resident set size with FIFO; evicted pages go to global clean or dirty lists, reclaimable on fault (ostep p. 292).
- Larger second-chance lists approach LRU; dirty list written out in clusters (ostep p. 292-293).
- Kernel mapped in every address space, zero page invalid to catch null pointers (ostep p. 289-291).

## Trade-offs

- Simple fairness-preserving policy plus a cheap undo path reclaims most of LRU's benefit.

## Agentic OS relevance

- (interpretation) Per-agent quota with a shared "recently evicted" pool lets an agent reclaim dropped context cheaply before it is truly overwritten.

## Related

- [[page-replacement-policy]]
- [[copy-on-write]]
- [[working-set-and-thrashing]]
