---
title: Security design principles (Saltzer-Schroeder) and least privilege
type: concept
sources: [ostep]
tags: [security]
---

# Security design principles (Saltzer-Schroeder) and least privilege

A checklist of design rules for protection mechanisms, of which least privilege (grant only the rights needed, only for as long as needed) is the most operationally important.

## Mechanisms

- Economy of mechanism, fail-safe defaults, complete mediation, open design (ostep pp. 699-700)
- Separation of privilege, least privilege, least common mechanism, psychological acceptability (ostep pp. 699-700)
- Least privilege is practical, not only theory: root-equivalent daemons turn any bug into total compromise (ostep pp. 737-738)
- Attackers go after root, so shrink what runs as root (ostep p. 739)
- Capabilities make it easy to hand a child a subset of rights (ostep pp. 733-734)

## Trade-offs

- Narrow privileges limit damage but increase configuration work and breakage when a task needs one more right.

## Agentic OS relevance

- (interpretation) Give each agent and each tool call the narrowest scoped credential and directory access, with deny-by-default.

## Related

- [[capabilities]]
- [[access-control]]
- [[setuid-and-privilege-escalation]]
