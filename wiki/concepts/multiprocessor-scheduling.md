---
title: Multiprocessor Scheduling
type: concept
sources: [ostep]
tags: [virtualization]
---

# Multiprocessor Scheduling

Scheduling across several CPUs, where hardware caches, shared-data synchronisation and load balance create problems a single-CPU scheduler never sees.

## Mechanisms

- Cache coherence: per-CPU caches can hold stale data; hardware bus snooping invalidates or updates copies (ostep pp. 132-133)
- Shared scheduler structures need locks; lock contention grows with CPU count (ostep pp. 133-135)
- Cache affinity: keep a process on the CPU holding its warm state (ostep p. 134)
- SQMS: one global queue, simple, but poor scalability and poor affinity (ostep pp. 135-136)
- MQMS: per-CPU queues scale and keep affinity but suffer load imbalance (ostep pp. 136-137)
- Fixes for imbalance: migration and work stealing, with a tension over how often to peek at other queues (ostep pp. 138-139)
- Linux has had O(1) (multi-queue, priority), CFS (multi-queue, proportional), BFS (single-queue, EEVDF) (ostep p. 139)

## Trade-offs

- Global view balances load easily but does not scale; local queues scale but need rebalancing.

## Agentic OS relevance

- (interpretation) Parallel inference replicas with warm KV caches act like CPUs with caches: route an agent back to the replica that holds its prefix (affinity) but steal work when queues skew.

## Related

- [[scheduling]]
- [[proportional-share-scheduling]]
- [[context-switch]]
