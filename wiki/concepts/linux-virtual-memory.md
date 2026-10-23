---
title: Linux Virtual Memory System
type: concept
sources: [ostep]
tags: [virtualization]
---

# Linux Virtual Memory System

A production VM combining hardware multi-level tables, huge pages, a unified page cache with 2Q replacement, and layered security defenses.

## Mechanisms

- User/kernel split at 0xC0000000 on 32-bit; kernel logical addresses (direct-mapped, unswappable, kmalloc) vs kernel virtual (vmalloc, non-contiguous) (ostep p. 295-296).
- x86-64 uses four-level tables over 48 bits, 12-bit offset (ostep p. 297).
- Huge pages (2MB, 1GB) mainly for TLB coverage (up to roughly 10% of cycles spent on TLB misses cited); added incrementally via mmap/shmget, then transparent huge pages (ostep p. 297-298).
- Unified page cache: mmapped files, file data, and anonymous memory in one hash table; dirty pages written by background threads (ostep p. 299).
- Modified 2Q: inactive and active lists, active kept near two-thirds, cyclic scans confined to the inactive list (ostep p. 299).
- Security: NX bit, ASLR and KASLR, kernel page-table isolation against Meltdown/Spectre at a performance cost (ostep p. 301-303).

## Trade-offs

- Generality across phones to datacenters (curse of generality) and security hardening cost performance and complexity.

## Agentic OS relevance

- (interpretation) One unified cache for all agent state sources with active/inactive tiers so a single bulk read cannot flush hot context; isolate each agent's memory view even at a performance cost.

## Related

- [[multi-level-page-table]]
- [[page-replacement-policy]]
- [[copy-on-write]]
- [[demand-paging]]
