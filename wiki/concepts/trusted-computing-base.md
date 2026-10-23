---
title: Trusted computing base
type: concept
sources: [ostep]
tags: [security]
---

# Trusted computing base

The OS cannot hide data from itself: whatever must be trusted to enforce security is the base, and it must be small and correct.

## Mechanisms

- The OS can do anything with the hardware (ostep pp. 695-696)
- Enclaves and TPM move some trust into hardware (ostep p. 696)
- Encryption cannot protect data from the OS using it: trust it or do not (ostep pp. 753-754)
- Economy of mechanism keeps the base small (ostep pp. 699-700)

## Trade-offs

- Shrinking the base reduces risk but pushes complexity to layers above or hardware.

## Agentic OS relevance

- (interpretation) The agent harness and sandbox are the TCB; the model is outside it and must be treated as untrusted.

## Related

- [[security-goals-and-threat-model]]
- [[virtual-machine-monitor]]
