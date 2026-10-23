---
title: Authentication
type: concept
sources: [ostep]
tags: [security]
---

# Authentication

Establishing which principal is acting, from something it knows, has, or is, and binding that identity to a process for its lifetime.

## Mechanisms

- Terms: principal, agent, object, credential; identities of user, group, program (ostep pp. 705-706)
- Identity is set at login, inherited by children, relied on for the process life (ostep pp. 707-708)
- Three factors: know, have, are (ostep pp. 708-709); multi-factor combines them (ostep p. 713)
- Passwords: store a salted cryptographic hash, never the password (ostep pp. 710-713); rate limit guessing (ostep pp. 711-712); vaults (ostep p. 712)
- Login failures must not reveal which part failed (ostep p. 714); tokens and SMS codes (ostep pp. 715-716)
- Biometrics trade false positives against false negatives (crossover error rate) and cannot be trusted when supplied remotely (ostep pp. 717-719)
- Non-human principals such as a web server account (ostep pp. 720-721); location/behavior (ostep p. 721); groups (ostep pp. 721-722)

## Trade-offs

- Stronger factors raise attacker cost and user friction; biometrics cannot be rotated once leaked.

## Agentic OS relevance

- (interpretation) An agent acts as a non-human principal; its identity should be distinct from the human's and not be inherited implicitly by spawned subagents.

## Related

- [[password-storage]]
- [[access-control]]
- [[distributed-security]]
