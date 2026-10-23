---
title: Storage Manager
type: concept
sources: [aios]
tags: [agentic]
---

# Storage Manager

Persists agent data (interaction logs, user preferences) beyond any agent's lifetime.

## Mechanisms

- Durable mediums: local files, databases, cloud (aios pp. 6-7).
- Supports retrieval augmentation; stores user preferences and history to enrich knowledge updates (aios p. 7).
- Syscalls: sto_alloc, sto_write, sto_read, sto_delete (aios p. 7).
- Memory blocks are transient; storage blocks are "consistently stored" (aios Fig. 5, p. 6).
- Future: shared storage, hierarchical storage by access frequency (aios p. 10).

## OS analogue

- File system / disk. Breaks: no file/directory abstraction or consistency model described; paper's mem-to-storage movement policy is unspecified (interpretation).

## Related

- [[memory-manager]]
- [[file-system-implementation]]
- [[access-manager]]
