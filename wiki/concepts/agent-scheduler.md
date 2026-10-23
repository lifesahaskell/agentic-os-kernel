---
title: Agent Scheduler
type: concept
sources: [aios]
tags: [agentic]
---

# Agent Scheduler

Kernel module that queues and orders agent requests to the LLM so steps from different agents interleave instead of running agent-by-agent.

## Mechanisms

- Strategies named: FIFO, Round Robin, "and other" algorithms (aios p. 5).
- Sequential baseline: all steps of one agent run first, raising later agents' waiting time (aios p. 5).
- Concurrent timeline example: A1, B1, C1, B2, A2, A3, C2, C3 (aios p. 5).
- Goal: balance waiting and turnaround time; no agent monopolizes the LLM; minimize idle (aios p. 5).
- Round Robin implies time quanta, hence suspension mid-generation, which motivates the context manager (aios p. 6).
- Future: dependency-aware scheduling between requests; schedule locally deployed tool models and their snapshots (aios pp. 5, 10).

## Evaluation

- Only FIFO is evaluated vs non-scheduled sequential (aios p. 9). Gemma-2b: Rec agent wait 4.78 s -> 3.46 s, turnaround 7.68 -> 5.91 s; Math agent wait 0.002 -> 2.50 s (aios p. 9). LLaMA-2-13b: Rec wait 53.86 -> 34.92 s, turnaround 71.62 -> 52.44 s; Math wait 0.003 -> 23.27 s, turnaround 18.48 -> 40.02 s (aios p. 9). Five trials each (aios p. 9).

## OS analogue

- CPU scheduler (FIFO, RR). Breaks: preemption mid-request requires saving LLM generation state; and the scheduled results show it trades first-agent latency for last-agent latency, not a net speedup (interpretation from Table 5).

## Related

- [[scheduling]]
- [[context-switch]]
- [[context-snapshot-restore]]
- [[agent-as-process]]
- [[evaluation-aios]]
