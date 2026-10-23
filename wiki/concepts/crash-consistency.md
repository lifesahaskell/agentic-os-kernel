---
title: Crash Consistency
type: concept
sources: [ostep]
tags: [persistence]
---

# Crash Consistency

The problem of updating multiple on-disk structures when a crash can land between any two writes, leaving metadata inconsistent, space leaked, or garbage data visible.

## Mechanisms

- Append needs three writes (inode, data bitmap, data block); crash after any subset yields garbage data, bitmap/inode disagreement, or a leak (ostep p. 562-565)
- Disk commits one write at a time, so atomic multi-block update is not available; this is the consistent-update problem (ostep p. 565)
- fsck: lazy approach, scans superblock, bitmaps vs inodes, link counts, duplicates, bad pointers, directories before mount; fixes metadata only, not garbage data, and runtime scales with disk size (ostep p. 565-567)
- Journaling reduces recovery to O(log size) (ostep p. 567, 578)
- Alternatives: Soft Updates (ordered writes), copy-on-write, backpointer-based consistency, optimistic crash consistency (ostep p. 577-578)
- Rule "write the pointed-to object before the pointer" underlies ordering schemes (ostep p. 574)

## Trade-offs

- Consistency can be bought by slow offline repair, extra write traffic (journal), or careful ordering, and each trades recovery time against runtime cost and implementation complexity.

## Agentic OS relevance

- (interpretation) An agent that writes memory as several files (index plus entry) faces the same multi-structure update problem; one-file atomic replacement avoids it.

## Related

- [[journaling]]
- [[log-structured-file-system]]
- [[fsync-and-durability]]
- [[atomic-rename]]
- [[data-integrity]]
