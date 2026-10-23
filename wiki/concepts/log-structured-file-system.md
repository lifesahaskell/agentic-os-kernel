---
title: Log-Structured File System (LFS)
type: concept
sources: [ostep]
tags: [persistence]
---

# Log-Structured File System (LFS)

Buffer all updates, including metadata, into large in-memory segments and write them sequentially to free disk space; never overwrite in place.

## Mechanisms

- Motivation: growing memory makes workloads write-dominated; sequential vs random gap; FFS needs many seeks per create; RAID small-write problem (ostep p. 582)
- Segment size chosen by amortization: D = F/(1-F) x Rpeak x Tposition, e.g. 9MB for 90% of 100MB/s at 10ms (ostep p. 584-586)
- Inode map (imap) maps inode number to latest inode address; imap chunks written next to new data; a fixed checkpoint region points at them (ostep p. 586-588)
- Imap avoids the recursive update problem: moving an inode does not change its parent directory (ostep p. 589)
- Garbage collection: segment summary records (inode, offset) per block; block is live iff the inode still points to it; cleaner compacts M segments into N<M (ostep p. 590-592)
- Cleaning policy separates hot and cold segments (ostep p. 592)
- Crash recovery: two alternating checkpoint regions with matching timestamps; roll forward through the log after the last checkpoint (ostep p. 593)
- Legacy: copy-on-write in WAFL, ZFS, btrfs and in SSD firmware (ostep p. 593-594)

## Trade-offs

- Writes approach peak bandwidth, but the system accumulates garbage and must pay for cleaning.

## Agentic OS relevance

- (interpretation) An append-only event log for agent memory is LFS in miniature; compaction of stale entries is the cleaner and has the same hot/cold policy question.

## Related

- [[journaling]]
- [[flash-ssd]]
- [[raid]]
- [[crash-consistency]]
- [[fast-file-system]]
