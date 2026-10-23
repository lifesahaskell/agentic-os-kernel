---
title: Address Space
type: concept
sources: [ostep]
tags: [virtualization]
---

# Address Space

The OS abstraction of memory: each process sees its own large, private, contiguous space containing code, heap and stack, while the OS maps it onto shared physical memory.

## Mechanisms

- History: single-program machines, multiprogramming, then time sharing; saving all memory to disk on a switch was too slow, so processes stay resident (ostep pp. 147-149)
- Layout: code at the bottom, heap growing one way, stack growing the other (ostep pp. 149-150)
- Virtual address: every address a program prints is virtual (ostep pp. 150-151, 153)
- Three goals: transparency, efficiency, protection (ostep p. 151)
- Isolation principle: one failure must not affect another (ostep p. 152)

## Trade-offs

- A convenient private illusion costs translation hardware and OS bookkeeping on every access.

## Agentic OS relevance

- (interpretation) A context window is the agent's address space: each agent should see only its own, with a defined layout (system prompt as code, history as stack, retrieved data as heap).

## Related

- [[address-translation]]
- [[memory-api]]
- [[isolation-and-protection]]
- [[process]]
