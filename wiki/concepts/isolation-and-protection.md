---
title: Isolation and Protection
type: concept
sources: [ostep]
tags: [virtualization]
---

# Isolation and Protection

The principle that faulty or hostile entities must be unable to read or damage each other or the OS, enforced by hardware and by killing offenders.

## Mechanisms

- Memory protection via bounds checks so loads, stores and fetches stay inside the address space (ostep pp. 151-152, 173)
- Privileged mode and privileged instructions guard the base/bounds registers and trap table (ostep pp. 174-175)
- OS reacts hostilely to violations: terminate the process (ostep pp. 146, 176)
- Scheduling must also be secure against gaming (ostep p. 109); argument validation in syscalls (ostep p. 80)
- Microkernels push isolation inside the OS (ostep p. 152)

## Trade-offs

- Strong isolation costs performance and convenience in sharing.

## Agentic OS relevance

- (interpretation) Agents running untrusted tool output need the same stance: privileged operations behind a narrow gate, per-agent memory boundaries, and a kill path for a misbehaving agent.

## Related

- [[address-translation]]
- [[system-call]]
- [[trap-and-interrupt]]
- [[mlfq]]
