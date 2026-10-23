---
title: Hard Disk Drive
type: concept
sources: [ostep]
tags: [persistence]
---

# Hard Disk Drive

A rotating-platter block device whose I/O time is seek plus rotational delay plus transfer, so sequential access is far cheaper than random access. The OS sees a linear array of sectors.

## Mechanisms

- Seek moves the arm to the right track, then rotation waits for the sector, then transfer; positioning dominates small I/O (ostep p. 468-469)
- Track skew offsets sector numbering so sequential reads across a track boundary do not miss a rotation (ostep p. 469)
- Multi-zoned tracks and an on-drive cache (track buffer); write-back caching reports completion before data is on the platter, write-through does not (ostep p. 470)
- Disk scheduling: SSTF (starvation risk), SCAN/elevator, SPTF which accounts for rotation, all in the work-conserving family (ostep p. 475-479)
- I/O merging coalesces adjacent requests before dispatch (ostep p. 478)
- Random vs sequential throughput gap is the quantity every later file-system design (FFS, LFS) exploits (ostep p. 479)

## Trade-offs

- Scheduling and caching raise throughput but reorder or hide writes, which complicates durability and ordering.

## Agentic OS relevance

- (interpretation) Many small appends from an agent (logs, memory entries) pay positioning cost each time; batching into larger sequential writes is the HDD-friendly pattern.

## Related

- [[io-devices-and-drivers]]
- [[raid]]
- [[fast-file-system]]
- [[log-structured-file-system]]
- [[flash-ssd]]
