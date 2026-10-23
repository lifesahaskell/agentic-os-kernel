---
title: Data Integrity and Protection
type: concept
sources: [ostep]
tags: [persistence]
---

# Data Integrity and Protection

Techniques to detect and recover from partial disk failures (latent sector errors and silent corruption) so data read equals data written.

## Mechanisms

- Fail-partial model: LSEs (detected, returned as error) and block corruption (silent); better drives reduce but do not eliminate either (ostep p. 622-624)
- LSEs recovered with redundancy (mirror copy, parity reconstruction); RAID-DP adds second parity because an LSE during rebuild is fatal otherwise (ostep p. 624-625)
- Checksum functions: XOR, addition, Fletcher, CRC; none is collision-free (ostep p. 625-627)
- Layout: per-sector checksum (520-byte sectors) or packed checksum sectors that cost an extra read-modify-write (ostep p. 627-628)
- On read, compare stored vs computed checksum; on mismatch use a redundant copy or return an error (ostep p. 628)
- Misdirected writes detected by storing physical ID (disk, block) with the checksum; lost writes need read-after-write or checksums in a parent such as the inode (ZFS) (ostep p. 629-630)
- Scrubbing periodically reads and verifies all blocks; overhead is about 0.19% space (8B per 4KB) plus CPU and I/O time (ostep p. 630-631)

## Trade-offs

- Stronger checksums and scrubbing detect more faults at higher CPU, I/O, and space cost, and detection without a redundant copy only converts silent corruption into an error.

## Agentic OS relevance

- (interpretation) Agent memory files that persist for months deserve checksums or content hashes so silent corruption or truncated writes are detected before the agent trusts them.

## Related

- [[raid]]
- [[crash-consistency]]
- [[flash-ssd]]
- [[journaling]]
