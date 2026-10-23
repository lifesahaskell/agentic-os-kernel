---
title: Access control lists
type: concept
sources: [ostep]
tags: [security]
---

# Access control lists

Each object stores a list of which principals may do what, checked on each access.

## Mechanisms

- One list per object, stored in the inode (ostep p. 728)
- Unix owner/group/other 9-bit mode is a compact ACL (ostep pp. 729-730)
- Easy to see who can access an object and easy to revoke (ostep p. 730)
- Hard to enumerate everything a principal can access; needs a common identity namespace across machines (ostep p. 730); namespace aside (ostep p. 731)

## Trade-offs

- Object-centric audit and revocation versus no cheap answer to "what can this principal touch".

## Agentic OS relevance

- (interpretation) A per-resource allowlist of agents is an ACL; it fails when agents are created dynamically and lack stable identities.

## Related

- [[capabilities]]
- [[access-control]]
- [[afs]]
