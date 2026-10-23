---
title: Journaling (Write-Ahead Logging)
type: concept
sources: [ostep]
tags: [persistence]
---

# Journaling (Write-Ahead Logging)

Before updating structures in place, write a description of the update to a log; after a crash, replay committed transactions. Used by ext3/ext4, NTFS, XFS, JFS.

## Mechanisms

- Data journaling transaction: TxB, inode, bitmap, data, TxE; steps are journal write, journal commit, checkpoint, later free (ostep p. 568, 571, 573)
- TxE must be issued only after the other journal blocks complete, because disks may reorder; TxE sized to one 512-byte atomic sector (ostep p. 569-571)
- Write barriers/cache flushes enforce ordering; some disks ignore them (ostep p. 569)
- Transaction checksum in ext4 lets the whole transaction go out in one write (ostep p. 570)
- Recovery is redo logging: replay committed transactions in order; replay is idempotent so a crash mid-checkpoint is fine (ostep p. 571)
- Batching into a global transaction and a circular log with a journal superblock (ostep p. 572-573)
- Ordered/metadata journaling avoids writing data twice but writes data first so inodes never point to garbage (ostep p. 573-575)
- Block reuse after delete requires revoke records so stale journal entries are not replayed (ostep p. 575-576)

## Trade-offs

- Data journaling writes everything twice for full data safety; metadata journaling halves that but guarantees only metadata consistency.

## Agentic OS relevance

- (interpretation) The same commit-then-apply pattern fits agent memory: append an intent record, fsync, then update the derived store, and replay on restart.

## Related

- [[crash-consistency]]
- [[fsync-and-durability]]
- [[log-structured-file-system]]
- [[data-integrity]]
