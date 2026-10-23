---
title: Paging
type: concept
sources: [ostep]
tags: [virtualization]
---

# Paging

Divide virtual memory into fixed-size pages and physical memory into same-size frames, mapping pages to frames through a per-process page table.

## Mechanisms

- OS keeps a free list of frames; any page can go in any frame (ostep p. 212-214).
- Linear page table indexed by VPN; for 32-bit/4KB pages with 4-byte PTEs the table is 4MB per process (ostep p. 216).
- PTE bits: valid, protection, present, dirty, reference (ostep p. 217-218).
- No external fragmentation and natural support for sparse address spaces (ostep p. 223).
- Larger pages shrink the table but add internal fragmentation (ostep p. 243-244).

## Trade-offs

- Fixed granularity removes external fragmentation but costs table space, internal fragmentation, and an extra lookup per access.

## Agentic OS relevance

- (interpretation) Slice agent context into fixed-size chunks (pages) that can be independently resident, evicted, or shared, rather than treating a conversation as one blob.

## Related

- [[address-translation]]
- [[tlb]]
- [[multi-level-page-table]]
- [[demand-paging]]
