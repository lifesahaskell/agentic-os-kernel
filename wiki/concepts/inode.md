---
title: Inode
type: concept
sources: [ostep]
tags: [persistence]
---

# Inode

The on-disk structure holding a file's metadata and block pointers, addressed by inode number; directories supply the name-to-number mapping.

## Mechanisms

- Inode table is a fixed array on disk; inode number gives the block and offset by arithmetic, e.g. 256-byte inodes, 16 per 4KB block (ostep p. 531-533)
- Contains type, size, owner, permissions, times, link count, and block pointers (ostep p. 531-534)
- Multi-level index: direct pointers, then single, double, (triple) indirect blocks; 12 direct pointers cover 48KB at 4KB blocks (ostep p. 534-536)
- Extents (pointer plus length) are the alternative, more compact but less flexible (ostep p. 535)
- Directory is a file whose data is (name, inode number) entries (ostep p. 536)
- Link count in the inode is what unlink decrements and fsck verifies (ostep p. 518-519, 566)

## Trade-offs

- Pointer-based indexing is flexible and handles sparse and small files well, at the cost of extra indirect-block reads for large files.

## Agentic OS relevance

- (interpretation) Many small memory files each cost an inode plus a data block; inode exhaustion, not bytes, can be the limit for file-per-memory designs.

## Related

- [[file-system-implementation]]
- [[file-and-directory-api]]
- [[fast-file-system]]
- [[journaling]]
