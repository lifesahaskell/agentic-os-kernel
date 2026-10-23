---
title: Access control and the reference monitor
type: concept
sources: [ostep]
tags: [security]
---

# Access control and the reference monitor

Decides whether a subject may perform an access on an object by consulting a policy at a single mediating point.

## Mechanisms

- Subject, object, access, authorization; the reference monitor mediates (ostep p. 725)
- Virtualization avoids per-access checks by making resources unreachable (ostep pp. 725-726)
- Discretionary versus mandatory control (ostep pp. 734-735); defaults and umask (ostep p. 735)
- Android permission labels (ostep p. 736); RBAC and type enforcement (ostep pp. 737-738)
- ACLs and capabilities are the two basic implementations (ostep pp. 728-734)

## Trade-offs

- Fine-grained policy is expressive but costly to check and administer.

## Agentic OS relevance

- (interpretation) Tool permission prompts and sandbox policies are a reference monitor; complete mediation means no tool path bypasses it.

## Related

- [[access-control-lists]]
- [[capabilities]]
- [[principle-of-least-privilege]]
- [[rbac-and-mac]]
