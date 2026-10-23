---
title: Virtualization and the OS as Resource Manager
type: concept
sources: [ostep]
tags: [virtualization]
---

# Virtualization and the OS as Resource Manager

The OS turns a physical resource (CPU, memory) into a more general, easy-to-use virtual form, and manages the resource among competing programs. Two sharing modes recur: time sharing (CPU) and space sharing (memory/disk).

## Mechanisms

- OS as virtual machine / standard library / resource manager (ostep p. 25)
- Time sharing: one entity uses the resource briefly, then another; cost is lower per-entity speed (ostep pp. 47-48)
- Space sharing: divide the resource spatially among entities (ostep pp. 47-48)
- Mechanism (how) separated from policy (which) as a design principle (ostep p. 49)
- Illusion is the product: each process believes it owns the machine (ostep pp. 145-146, 167-168)

## Trade-offs

- Transparent sharing costs efficiency and requires the OS to retain control of the hardware.

## Agentic OS relevance

- (interpretation) Inference capacity and context window are the "CPU" and "RAM" being virtualized; a kernel gives each agent the illusion of a dedicated model and a private context.

## Related

- [[process]]
- [[scheduling]]
- [[address-space]]
- [[limited-direct-execution]]
