---
title: CPU Scheduling Policies and Metrics
type: concept
sources: [ostep]
tags: [virtualization]
---

# CPU Scheduling Policies and Metrics

Policies that choose which ready job runs next, judged mainly by turnaround time and response time, with fairness trading against both.

## Mechanisms

- Metrics: turnaround = completion - arrival; response = first run - arrival (ostep pp. 92, 96)
- FIFO suffers the convoy effect behind a long job (ostep pp. 93-94)
- SJF minimises average turnaround when all arrive together (ostep p. 94); STCF adds preemption for late arrivals (ostep pp. 95-96)
- Round robin with a time slice optimises response; slice length amortises switch cost (ostep pp. 97-98)
- Turnaround and response conflict; fair policies are bad at turnaround (ostep pp. 98-99)
- Overlap: treat each CPU burst between I/Os as a separate job to keep CPU and disk busy (ostep pp. 99-100)
- No oracle for job length, which motivates learning from history (ostep p. 100)

## Trade-offs

- Optimising turnaround (run short jobs first) harms response and fairness, and vice versa.

## Agentic OS relevance

- (interpretation) Agents have unknown runtime like jobs; a scheduler needs a cheap predictor of task length, and must pick between finishing short tasks fast and giving every user quick first tokens.

## Related

- [[mlfq]]
- [[proportional-share-scheduling]]
- [[multiprocessor-scheduling]]
- [[context-switch]]
