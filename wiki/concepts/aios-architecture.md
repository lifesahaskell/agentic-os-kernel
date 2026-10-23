---
title: AIOS Architecture and Layers
type: concept
sources: [aios]
tags: [agentic]
---

# AIOS Architecture and Layers

A three-layer design (application, kernel, hardware) in which an LLM-specific kernel sits beside the ordinary OS kernel and exposes LLM services to agents.

## Mechanisms

- Application layer hosts agents (travel, rec, coding, math, narrative) and the AIOS SDK (aios p. 4).
- Kernel layer splits into OS Kernel (non-LLM calls) and LLM Kernel (LLM calls); work concentrates on the LLM kernel without significantly altering the OS kernel (aios p. 4).
- LLM kernel modules: LLM system call interface, agent scheduler, context manager, memory manager, storage manager, tool manager, access manager (aios p. 4).
- Hardware layer: CPU, GPU, memory, disk, peripherals (aios p. 4).
- LLM kernel syscalls never reach hardware directly; they call OS syscalls, which manage hardware (aios p. 4).
- Motivation: Fig. 1 travel agent needs both LLM-level resources (preference retrieval, tool choice, text generation) and OS-level ones (disk, software) (aios p. 2).

## Evaluation

- none specific to the architecture as a whole.

## OS analogue

- Layered monolithic/modular kernel with a driver-like boundary. Breaks: the "LLM kernel" is a user-space-style Python service over an LLM, not privileged code with hardware isolation (interpretation).

## Related

- [[llm-system-calls]]
- [[aios-sdk]]
- [[agent-as-process]]
- [[system-call]]
- [[agent-scheduler]]
