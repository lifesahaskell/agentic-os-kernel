---
title: File Descriptor and Open File Table
type: concept
sources: [ostep]
tags: [persistence]
---

# File Descriptor and Open File Table

A small per-process integer indexing an array of pointers to entries in a system-wide open file table, each holding the current offset, flags, and a pointer to the inode.

## Mechanisms

- Each process has an array of descriptors in struct proc; 0, 1, 2 are stdin, stdout, stderr (ostep p. 503-506)
- Open file table entry stores inode pointer, current offset, and a reference count (ostep p. 506-507)
- Each open() creates a new table entry with its own offset, even for the same file; read advances only that entry's offset (ostep p. 507-508)
- fork() and dup()/dup2() make multiple descriptors share one table entry (shared offset, ref count raised); the entry is freed when the count hits zero (ostep p. 508-510)
- lseek changes only the entry's offset (ostep p. 508)
- Descriptors are the handle fsync() and fstat() take (ostep p. 511, 513)

## Trade-offs

- Shared vs separate offsets give flexibility (redirection, child processes) but make concurrent writers on a shared entry order-sensitive.

## Agentic OS relevance

- (interpretation) Child processes spawned by an agent share offsets for inherited descriptors, so two tools appending through one inherited fd interleave unpredictably; open separately or use O_APPEND.

## Related

- [[file-and-directory-api]]
- [[inode]]
- [[fsync-and-durability]]
