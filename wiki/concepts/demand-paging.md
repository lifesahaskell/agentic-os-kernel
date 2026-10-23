---
title: Demand Paging, Prefetching, and Clustering
type: concept
sources: [ostep]
tags: [virtualization]
---

# Demand Paging, Prefetching, and Clustering

Page-selection and write-out policies: bring pages in only when touched, optionally guess ahead, and batch writes.

## Mechanisms

- Demand paging loads a page on first access (ostep p. 282).
- Prefetching loads page P+1 when P is loaded, only when success is likely (ostep p. 282).
- Clustering batches pending writes into one large disk write (ostep p. 282, 292).
- mmap'd files fault in pages lazily; every process's code is mapped this way (ostep p. 300).
- Demand zeroing defers allocating and zeroing a page until first touch (ostep p. 293).

## Trade-offs

- Laziness avoids unneeded work and cuts latency, but each first touch pays a fault.

## Agentic OS relevance

- (interpretation) Load tools, documents, and memory into context only when the agent references them; prefetch only on high-confidence patterns; batch writebacks to long-term memory.

## Related

- [[swapping]]
- [[copy-on-write]]
- [[page-replacement-policy]]
