---
title: AFS (Andrew File System)
type: concept
sources: [ostep]
tags: [distribution]
---

# AFS (Andrew File System)

A distributed file system built for scale, using whole-file caching on client disks and server callbacks instead of polling.

## Mechanisms

- Goal is scale (ostep p. 675); whole-file caching, fetch on open and store on close (ostep p. 676)
- AFSv1 failed on path-traversal cost and TestAuth polling, about 20 clients per server (ostep pp. 677-678)
- AFSv2 adds callbacks (server promises to notify) and FIDs (volume + file + uniquifier); client walks paths itself (ostep pp. 678-679)
- Updates visible at close, last closer wins; local processes see writes at once (ostep pp. 680-682)
- Crash recovery is harder than NFS because callbacks live in memory, so clients treat caches as suspect and use heartbeats (ostep p. 682)
- About 50 clients per server; worse for overwrites and small reads of large files (ostep pp. 683-684)
- Global namespace, authentication, user-managed ACLs, volumes (ostep p. 685)

## Trade-offs

- Callbacks cut server load and give clean consistency, but make the server stateful and crash recovery complex.

## Agentic OS relevance

- (interpretation) Push invalidation versus poll is the same choice an agent faces for refreshing cached tool or file state.

## Related

- [[nfs]]
- [[cache-consistency]]
- [[access-control-lists]]
