---
title: Multi-Level and Alternative Page Tables
type: concept
sources: [ostep]
tags: [virtualization]
---

# Multi-Level and Alternative Page Tables

Structures that shrink page tables by allocating table space only for address-space regions in use.

## Mechanisms

- Hybrid paging plus segments gives a page table per segment but reintroduces external fragmentation (ostep p. 246-247).
- Page directory of PDEs points to page-table pages; a PDE valid bit means a whole table page exists (ostep p. 247-248).
- Worked example: 16KB space with 64B pages, split into PDIndex and PTIndex (ostep p. 249-252).
- More than two levels when the directory itself exceeds a page (ostep p. 253); two loads on a TLB miss is the time-space trade-off (ostep p. 249).
- Inverted page table: one entry per physical frame, searched via hash (ostep p. 255).
- Page tables themselves can be swapped to disk (ostep p. 255).

## Trade-offs

- Memory saved on sparse spaces is paid for with extra memory accesses on each miss.

## Agentic OS relevance

- (interpretation) Hierarchical memory indexes for agent state (index of indexes) so unused regions of a large knowledge space cost nothing to hold.

## Related

- [[paging]]
- [[tlb]]
- [[linux-virtual-memory]]
