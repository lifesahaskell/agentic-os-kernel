---
title: File System Implementation (vsfs)
type: concept
sources: [ostep]
tags: [persistence]
---

# File System Implementation (vsfs)

The very simple file system: disk divided into superblock, inode and data bitmaps, inode table, and data blocks, plus the read/write paths and caching that operate on them.

## Mechanisms

- Layout: superblock, inode bitmap, data bitmap, inode table, data region (ostep p. 531-532)
- Superblock describes the volume and is read at mount (ostep p. 532, 538)
- Open of /foo/bar traverses root inode, root data, foo inode, foo data, bar inode: I/O proportional to path length (ostep p. 538-540)
- Read updates access time in the inode, so even reads cause writes unless disabled (ostep p. 540)
- Creating a file touches inode bitmap, inode, directory data, directory inode; appending touches data bitmap, data block, inode (ostep p. 541)
- Page/buffer cache absorbs reads; writes are buffered 5-30 seconds, enabling batching, scheduling, and avoided writes, but buffered data is lost on crash (ostep p. 542-543)
- Durability/performance trade-off: apps force writes with fsync(), direct I/O, or the raw device (ostep p. 544)

## Trade-offs

- Delayed writes buy speed by risking recent data; the cache helps reads fully but writes only partially.

## Agentic OS relevance

- (interpretation) A write() that returned success is not durable; an agent killed or machine crashed within the buffering window can lose its latest memory.

## Related

- [[inode]]
- [[fast-file-system]]
- [[crash-consistency]]
- [[fsync-and-durability]]
