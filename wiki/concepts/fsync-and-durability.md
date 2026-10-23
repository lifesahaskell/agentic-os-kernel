---
title: fsync and Durability
type: concept
sources: [ostep]
tags: [persistence]
---

# fsync and Durability

write() only hands data to a buffering file system; fsync(fd) forces the file's dirty data to storage and returns when done, and is the application's lever on the durability/performance trade-off.

## Mechanisms

- write() is buffered for roughly 5 to 30 seconds, so a crash after write() returns can lose data (ostep p. 511, 543)
- fsync(int fd) forces all dirty data for that file to disk and returns after completion (ostep p. 511)
- A newly created file also needs fsync() on its containing directory so the directory entry is durable; commonly missed and a source of application bugs (ostep p. 511)
- Alternatives: direct I/O or the raw device interface (ostep p. 544)
- Below the file system, disk write caches and ignored barriers can still break the guarantee, so correctness depends on the device honoring flushes (ostep p. 569)
- Journals order commit blocks with barriers; fsync-heavy workloads motivated optimistic crash consistency (ostep p. 569, 578-579)

## Trade-offs

- fsync buys safety at the price of waiting for the device, so calling it per write is slow and never calling it is risky.

## Agentic OS relevance

- (interpretation) Agents should fsync after writing memory or state they cannot reconstruct, and fsync the parent directory when creating new files.

## Related

- [[atomic-rename]]
- [[file-and-directory-api]]
- [[crash-consistency]]
- [[journaling]]
- [[file-system-implementation]]
