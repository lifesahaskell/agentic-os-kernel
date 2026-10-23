---
title: Memory Manager
type: concept
sources: [aios]
tags: [agentic]
---

# Memory Manager

Holds each agent's short-term interaction history, available only while the agent is active.

## Mechanisms

- Lifetime: while the agent is waiting for execution or running (aios p. 6).
- Each agent's memory stored independently; others cannot access without access-manager authorization (aios p. 6).
- Syscalls: mem_alloc, mem_write, mem_read, mem_clear, each with agent id (aios p. 7).
- Chosen for rapid retrieval without burdening AIOS storage (aios p. 6).
- Future: shared memory pools among agents, hierarchical caches (aios p. 6).

## OS analogue

- Per-process RAM allocation. Breaks: it stores interaction logs, not addressable program memory; no paging or eviction policy is described (interpretation).

## Related

- [[storage-manager]]
- [[access-manager]]
- [[address-space]]
- [[paging]]
