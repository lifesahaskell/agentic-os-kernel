---
title: Capabilities
type: concept
sources: [ostep]
tags: [security]
---

# Capabilities

An unforgeable token that both names an object and grants specific rights to it; possession is the authorization.

## Mechanisms

- Kept in kernel, per-process list reachable from the PCB; grants mediated by syscalls (ostep pp. 731-734)
- Revocation is easy if the OS holds them; delegation of a subset to a child is natural (ostep pp. 733-734)
- Linux file descriptors are capabilities created after an ACL check (ostep p. 734)
- Cryptographic capabilities: signed or encrypted data with expiry and holder identity, checkable without a table (ostep pp. 758-760)
- Symmetric version needs a shared key; public key version allows separate creator and checker; may bind to an IP (ostep pp. 759-760)
- Web cookies are cryptographic capabilities (ostep p. 773, footnote 6)

## Trade-offs

- Easy delegation and cheap checks versus hard revocation of copied tokens and hard enumeration of who holds what.

## Agentic OS relevance

- (interpretation) Scoped, expiring tokens handed to a subagent are capabilities; a leaked token is full authority, so keep them short-lived.

## Related

- [[access-control-lists]]
- [[principle-of-least-privilege]]
- [[cryptography]]
- [[distributed-security]]
