---
title: File and Directory API (POSIX)
type: concept
sources: [ostep]
tags: [persistence]
---

# File and Directory API (POSIX)

The system-call interface for persistent storage: files are byte arrays named by an inode number, directories map names to inode numbers, and a single namespace is assembled by mounting.

## Mechanisms

- open() with flags (O_CREAT, O_WRONLY, O_TRUNC) returns a file descriptor; creat() is the older equivalent of those flags (ostep p. 503)
- read/write advance a per-open offset; lseek() sets it explicitly without a disk seek; random access uses lseek (ostep p. 506-509)
- mkdir creates a directory containing "." and ".."; opendir/readdir/closedir enumerate entries; rmdir only removes empty directories (ostep p. 515-517)
- link() adds another name to an inode and increments its link count; unlink() removes a name and frees the inode only when the count reaches zero, so "delete" is really unlink (ostep p. 517-519)
- Symbolic links are separate files holding a path; they can cross file systems and point to directories, and can dangle (ostep p. 519-521)
- stat()/fstat() return metadata including size, inode number, link count, ownership, times (ostep p. 513-514)
- mount() grafts a file system onto an existing directory (mount point), unifying trees (ostep p. 523-525)

## Trade-offs

- A thin, stable byte-stream interface keeps applications portable but leaves durability and atomicity details (fsync, rename, directory sync) to the caller.

## Agentic OS relevance

- (interpretation) An agent's persistent memory is, mechanically, files and directories; link counts and unlink semantics determine what "forget" really does.

## Related

- [[file-descriptor]]
- [[inode]]
- [[fsync-and-durability]]
- [[atomic-rename]]
- [[file-system-implementation]]
