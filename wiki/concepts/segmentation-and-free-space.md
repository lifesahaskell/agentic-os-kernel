---
title: Segmentation and Free-Space Management
type: concept
sources: [ostep]
tags: [virtualization]
---

# Segmentation and Free-Space Management

Variable-size allocation of memory regions and the allocator strategies for tracking holes; the source of external fragmentation that paging was designed to remove.

## Mechanisms

- Free list with split on allocate and coalesce on free (ostep p. 196-197).
- Allocation header holds size plus a magic number for integrity checks (ostep p. 198).
- Placement policies: best, worst, first, and next fit (ostep p. 204-205).
- Segregated lists and slab allocation keep pre-initialized objects of common sizes (ostep p. 206-207).
- Buddy allocation splits and merges power-of-two blocks; buddies differ by one address bit (ostep p. 207-208).

## Trade-offs

- Variable sizes fit requests tightly but fragment; fixed sizes or power-of-two rounding trade internal waste for easy coalescing.

## Agentic OS relevance

- (interpretation) Variable-length context windows for agents fragment the token budget the same way; fixed-size context blocks or buddy-style budget splits avoid packing problems.

## Related

- [[address-translation]]
- [[paging]]
