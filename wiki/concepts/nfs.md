---
title: NFS (v2), a stateless network file system
type: concept
sources: [ostep]
tags: [distribution]
---

# NFS (v2), a stateless network file system

A client/server file protocol designed so the server keeps no per-client state and therefore recovers from a crash by simply restarting.

## Mechanisms

- Goal: fast and simple crash recovery (ostep p. 659); stateless protocol (ostep p. 660)
- File handle = volume + inode + generation number identifies a file across requests (ostep p. 661)
- Protocol messages LOOKUP, READ, WRITE, GETATTR (ostep pp. 662-663)
- Server must commit writes to stable storage before acking (ostep pp. 670-671)
- Client caching and write buffering cause update visibility and stale cache problems (ostep pp. 667-668)
- Flush-on-close and a GETATTR attribute cache (3 s) mitigate them but make semantics hard to reason about (ostep pp. 668-669)
- VFS/vnode layer lets one kernel host many file systems (ostep p. 671); early security was lax, with Kerberos added later (ostep p. 672)

## Trade-offs

- Statelessness buys trivial crash recovery at the price of weak consistency and chatty validation.

## Agentic OS relevance

- (interpretation) A stateless tool server that takes self-contained requests is easier to restart than one holding session state.

## Related

- [[idempotency]]
- [[cache-consistency]]
- [[afs]]
- [[rpc]]
