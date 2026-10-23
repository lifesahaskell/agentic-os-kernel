---
title: Cache consistency
type: concept
sources: [ostep]
tags: [distribution]
---

# Cache consistency

When clients cache remote data, writers' updates must become visible and readers must not use stale copies; protocols differ in when they guarantee this.

## Mechanisms

- Two sub-problems: update visibility and stale cache (ostep pp. 667-668)
- NFS: flush on close plus attribute-cache validation via GETATTR (ostep pp. 668-669)
- AFS: callbacks break on other clients' close, last closer wins (ostep pp. 680-682)
- Cache consistency is not a panacea; concurrent writers still conflict (ostep p. 680)

## Trade-offs

- Stronger consistency costs more server messages or server state.

## Agentic OS relevance

- (interpretation) An agent's cached view of files or a repo goes stale between reads; decide when to revalidate.

## Related

- [[nfs]]
- [[afs]]
