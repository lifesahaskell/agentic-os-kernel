---
title: Hardware Address Translation (Base and Bounds)
type: concept
sources: [ostep]
tags: [virtualization]
---

# Hardware Address Translation (Base and Bounds)

Hardware rewrites each virtual address to a physical address on every memory reference, using a base register for relocation and a bounds register for protection.

## Mechanisms

- physical = virtual + base; fault if virtual >= bounds (ostep pp. 172-174)
- Base/bounds registers live in the MMU, one pair per CPU, modifiable only by privileged instructions (ostep p. 174)
- Static (software) relocation by a loader gives no protection and cannot move a running process (ostep p. 172)
- OS duties: allocate from a free list at creation, reclaim at exit, save/restore base and bounds on switch, install exception handlers (ostep pp. 175-177)
- A stopped process can be moved by copying its memory and updating the saved base (ostep p. 176)
- Internal fragmentation: the gap between stack and heap in the fixed slot is wasted, motivating segmentation (ostep p. 179)
- Segmentation keeps a base/bounds pair per segment, with segment bits in the VA selecting the pair (ostep p. 185-186).
- A negative-growth bit handles the stack; protection bits allow code-segment sharing (ostep p. 187-188).
- Variable-size segments cause external fragmentation; compaction is the costly fix (ostep p. 190-191).
- Paging replaces segments with fixed-size units: VPN selects a PTE, PFN replaces it, offset passes through unchanged (ostep p. 214-216).
- Without a cache, every access costs an extra memory reference for the page-table lookup, a 2x or worse slowdown (ostep p. 219-220).

## Trade-offs

- Very cheap and transparent, but wastes memory inside fixed slots.
- Flexibility of the address-space layout is bought with translation metadata and per-access lookup cost.

## Agentic OS relevance

- (interpretation) A per-agent offset-and-limit on a shared store (namespace prefix plus a size cap) gives cheap relocation and protection for agent memory or scratch space.
- (interpretation) An agent's logical context handle could be decoupled from where its tokens physically live (cache, summary, external store), with a cheap translation layer.

## Related

- [[address-space]]
- [[limited-direct-execution]]
- [[isolation-and-protection]]
- [[context-switch]]
- [[paging]]
- [[tlb]]
- [[multi-level-page-table]]
- [[segmentation-and-free-space]]
