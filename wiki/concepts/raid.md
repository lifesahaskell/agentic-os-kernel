---
title: RAID
type: concept
sources: [ostep]
tags: [persistence]
---

# RAID

Redundant Array of Independent Disks: combine many disks behind one block interface for capacity, performance, and reliability. Levels trade space, throughput, and fault tolerance.

## Mechanisms

- Level 0 striping: best capacity and throughput, no redundancy; chunk size tunes parallelism vs positioning (ostep p. 485-486)
- Level 1 mirroring: copies of each block, survives one failure for certain and up to N/2 depending on which fail, halves usable capacity (ostep p. 485, 488-489)
- Consistent-update problem: a crash between the two mirror writes leaves copies different; fixed with a write-ahead log, often in non-volatile RAM (ostep p. 490)
- Level 4 parity (XOR) on a dedicated disk: tolerates one failure; reconstruct by XOR of survivors (ostep p. 491-493)
- Small-write problem: updating one block needs read old data, read old parity, write both (4 I/Os), making the parity disk a bottleneck in RAID-4 (ostep p. 494)
- Level 5 rotates parity across disks, removing the bottleneck; the summary table compares capacity, reliability, sequential and random throughput (ostep p. 495-497)

## Trade-offs

- Parity gives redundancy at low capacity cost but makes small random writes expensive.

## Agentic OS relevance

- (interpretation) Agent state on shared storage inherits these write costs; storage-level redundancy does not protect against application-level corruption or deletion.

## Related

- [[hard-disk]]
- [[data-integrity]]
- [[log-structured-file-system]]
- [[crash-consistency]]
