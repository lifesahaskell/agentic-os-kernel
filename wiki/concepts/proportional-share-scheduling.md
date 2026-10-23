---
title: Proportional-Share Scheduling (Lottery, Stride, CFS)
type: concept
sources: [ostep]
tags: [virtualization]
---

# Proportional-Share Scheduling (Lottery, Stride, CFS)

Schedulers that guarantee each job a target share of the resource rather than optimising turnaround or response.

## Mechanisms

- Tickets represent share; lottery picks a random winning ticket each slice, walking a job list to find the holder (ostep pp. 116-119)
- Ticket currency, transfer and inflation: per-user sub-currencies, client hands tickets to server, trusted inflation (ostep p. 118)
- Lottery is probabilistic; fairness improves with job length (ostep pp. 120-121)
- Stride: stride = large number / tickets; run lowest pass value and add stride; exact proportions but needs global per-job pass state (ostep pp. 121-122)
- Linux CFS: lowest vruntime runs; slice = sched_latency / n with min_granularity floor; nice maps to weights; red-black tree gives O(log n) pick (ostep pp. 122-126)
- Sleepers are reset to min vruntime on wake to avoid monopoly, at the cost of short sleepers losing share (ostep p. 126)
- Open problems: ticket assignment and poor fit with I/O (ostep pp. 121, 127)

## Trade-offs

- Exact long-run shares versus the hard problem of deciding what each job's share should be.

## Agentic OS relevance

- (interpretation) Tickets map naturally to per-tenant or per-agent token quotas; ticket transfer maps to a parent agent lending its budget to a delegate while it works.

## Related

- [[scheduling]]
- [[mlfq]]
- [[multiprocessor-scheduling]]
