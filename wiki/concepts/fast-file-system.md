---
title: Fast File System (FFS) and Locality
type: concept
sources: [ostep]
tags: [persistence]
---

# Fast File System (FFS) and Locality

Berkeley redesign that keeps the file-system API but makes layout disk-aware: group related data and metadata near each other to avoid long seeks.

## Mechanisms

- Old UNIX fs got ~2% of disk bandwidth: inodes far from data, free list fragmentation, 512B blocks (ostep p. 547-548)
- Cylinder (modern: block) groups, each with superblock copy, inode and data bitmaps, inodes, data (ostep p. 549-550)
- Policy "keep related stuff together": directories go to a group with few directories and many free inodes; files go in the same group as their inode and parent directory (ostep p. 551-552)
- Locality measured on SEER traces: ~40% of opens are same file or same directory (ostep p. 553-554)
- Large-file exception spreads big files across groups after e.g. 12 direct blocks, amortizing seeks; chunk size comes from positioning time x bandwidth (409.6KB for 50% at 10ms, 40MB/s) (ostep p. 554-556)
- Sub-blocks (512B) for small files, parameterized rotational layout, long file names, symlinks, atomic rename() (ostep p. 556-558)

## Trade-offs

- Locality helps common patterns but does nothing for access patterns that cross the namespace tree (e.g. src vs obj).

## Agentic OS relevance

- (interpretation) Keeping an agent's related memory files in one directory exploits the same locality; scattering across many directories forfeits it.

## Related

- [[file-system-implementation]]
- [[hard-disk]]
- [[atomic-rename]]
- [[log-structured-file-system]]
