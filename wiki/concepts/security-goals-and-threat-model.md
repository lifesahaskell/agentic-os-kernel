---
title: Security goals, policy, and threat model
type: concept
sources: [ostep]
tags: [security]
---

# Security goals, policy, and threat model

Security is stated as goals (confidentiality, integrity, availability, plus authenticity and non-repudiation), turned into policies, and enforced by mechanisms; the OS is inside the trusted base.

## Mechanisms

- CIA goals plus authenticity and non-repudiation (ostep p. 697); mechanism is separated from policy (ostep p. 698)
- The OS can do anything with the hardware, so processes are at its mercy; enclaves and TPM reduce that (ostep pp. 695-696)
- Security differs from fault tolerance because the failures are adversarial (ostep p. 699)
- Syscall trap is the mediation point and the PCB identifies the caller (ostep pp. 701-703)
- Security is only as strong as the weakest link (ostep p. 702)
- Text notes the danger of fooling a user who has complete privileges (ostep p. 702)

## Trade-offs

- Every added protection has cost and usability friction, so goals must be prioritized against the actual threat.

## Agentic OS relevance

- (interpretation) An agent threat model must name the adversary (untrusted tool output, web content, other agents) before choosing mechanisms.

## Related

- [[principle-of-least-privilege]]
- [[trusted-computing-base]]
- [[access-control]]
