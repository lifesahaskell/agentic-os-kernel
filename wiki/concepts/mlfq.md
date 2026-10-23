---
title: Multi-Level Feedback Queue
type: concept
sources: [ostep]
tags: [virtualization]
---

# Multi-Level Feedback Queue

A scheduler with several priority queues that infers job behaviour from history, approximating SJF for short interactive jobs while staying fair to long ones.

## Mechanisms

- Rule 1: higher priority runs; Rule 2: equal priority round-robin (ostep pp. 104-105)
- Rule 3: new jobs enter at top; Rule 4: once a job uses its time allotment at a level it moves down, regardless of I/O yields (ostep pp. 106-111)
- Early Rule 4a/4b let a job retain priority by yielding just before the slice ends, so it can game the scheduler (ostep pp. 108-109)
- Rule 5: periodic priority boost every S prevents starvation and handles phase changes; S is a "voo-doo constant" (ostep pp. 109-112)
- Tuning: short slices at top, long slices at bottom; Solaris TS uses tables with 60 queues; FreeBSD uses decay-usage formulas (ostep p. 112)
- Advice interface (nice) lets users hint priority (ostep pp. 112-113)

## Trade-offs

- Learning behaviour from history avoids needing a job-length oracle but adds tunable constants and an attack surface.

## Agentic OS relevance

- (interpretation) New agent tasks start at high priority and sink as they burn tokens; an agent that fakes interactivity (cheap calls that hand back control) is the gaming analogue, so charge by tokens used, not by yields.

## Related

- [[scheduling]]
- [[proportional-share-scheduling]]
- [[multiprocessor-scheduling]]
