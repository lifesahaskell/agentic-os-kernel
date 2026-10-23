---
title: Reader-Writer Lock
type: concept
sources: [ostep]
tags: [concurrency]
---

# Reader-Writer Lock

A lock that permits many concurrent readers or exactly one writer, exploiting that lookups do not change data structures.

## Mechanisms

- Reader acquire: take lock, increment reader count, first reader grabs writelock, release lock (ostep p. 408-409)
- Reader release: last reader releases writelock so waiting writers can run (ostep p. 409)
- Writers take writelock directly and block while any reader is active (ostep p. 408)
- Simple implementation lets readers starve writers (ostep p. 408)
- Added overhead means it often is no faster than a simple lock; keep it simple (ostep p. 409-410)

## Trade-offs

- Read parallelism versus writer starvation and added bookkeeping cost.

## Agentic OS relevance

- (interpretation) Many agents reading a shared plan/notes file while one agent updates it fits this; writer starvation maps to a stuck updater never getting a turn.

## Related

- [[semaphore]]
- [[lock]]
- [[lock-based-data-structures]]
