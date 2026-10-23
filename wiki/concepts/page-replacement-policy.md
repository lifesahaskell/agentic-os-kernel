---
title: Page Replacement Policies
type: concept
sources: [ostep]
tags: [virtualization]
---

# Page Replacement Policies

Rules for choosing which resident page to evict when memory is full, evaluated by hit rate against a cost model.

## Mechanisms

- AMAT = T_M + P_Miss*T_D; with 100ns vs 10ms, even 1% misses dominate (ostep p. 269-270).
- Belady optimal evicts the page used furthest in the future; unattainable but the right baseline (ostep p. 270-272); misses classed as compulsory, capacity, conflict (ostep p. 272).
- FIFO and Random are simple but ignore importance; FIFO shows Belady's anomaly, LRU has the stack property (ostep p. 272-274).
- LRU/LFU exploit temporal and spatial locality; the looping-sequential workload is worst case for LRU and FIFO (ostep p. 275-278).
- Perfect LRU needs per-access accounting; approximate with a use bit and the clock algorithm (ostep p. 279-281).
- Prefer evicting clean over dirty pages since clean eviction is free (ostep p. 281).
- Scan-resistant variants such as ARC and Linux 2Q handle cyclic scans (ostep p. 283, 299).

## Trade-offs

- Better prediction of future use costs bookkeeping on the hot path; cheap approximations capture most of the gain.

## Agentic OS relevance

- (interpretation) Deciding which agent's context or which cached tool output to drop under a token budget: weigh recency, cost to regenerate (dirty vs clean), and scan resistance for bulk reads.

## Related

- [[working-set-and-thrashing]]
- [[swapping]]
- [[linux-virtual-memory]]
