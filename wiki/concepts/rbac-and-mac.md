---
title: RBAC, mandatory access control, and type enforcement
type: concept
sources: [ostep]
tags: [security]
---

# RBAC, mandatory access control, and type enforcement

Policy attached to roles or labels set by an administrator, not by object owners, so users cannot loosen it.

## Mechanisms

- Mandatory versus discretionary control (ostep pp. 734-735)
- Roles group permissions and users take roles (ostep pp. 737-738)
- Type enforcement labels subjects and objects and lists allowed type interactions (ostep p. 738)
- Android labels apps with declared permissions (ostep p. 736)

## Trade-offs

- Central control resists mistakes and compromise but is heavy to author and maintain.

## Agentic OS relevance

- (interpretation) Agent roles (reader, editor, deployer) map to RBAC; a policy outside the model's reach is MAC.

## Related

- [[access-control]]
- [[principle-of-least-privilege]]
