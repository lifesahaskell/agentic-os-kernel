---
title: AIOS Evaluation (Consistency and Scheduling)
type: concept
sources: [aios]
tags: [agentic]
---

# AIOS Evaluation (Consistency and Scheduling)

Two experiments on whether suspend/resume preserves outputs and whether scheduling improves wait/turnaround balance.

## Mechanisms

- Setup: Python 3.9, PyTorch 2.0.1, CUDA 11.8, Ubuntu 22.04, 8x RTX A5000 (aios p. 8).
- Backbones: Gemma-2b-it, Gemma-7b-it, LLaMA-2-13b-chat-hf; local deployment for accurate latency (aios p. 8).
- Agents: Math, Narrative, Rec (restaurant recommendations); 2-3 LLM requests each (aios p. 8).
- Consistency: outputs of parallel vs single-agent runs compared with BLEU and BERT score, temperature 0 (aios p. 9).
- Performance: FIFO scheduling vs sequential order Math, Narrative, Rec; metrics waiting time and turnaround time averaged per request; five trials (aios p. 9).

## Evaluation

- Table 4 all 1.0 (aios p. 9). Table 5 (aios p. 9): Gemma-7b Rec wait 14.21 -> 9.45 s, turnaround 18.64 -> 13.61 s; Math wait 0.002 -> 5.92 s. Paper says benefit is most evident for later-submitted requests with larger LLMs (aios p. 9).

## OS analogue

- Scheduler benchmarking on latency fairness. Breaks: no throughput, no mean-latency comparison; tiny agent count. Interpretation: Math agent latency worsens, e.g. 13b turnaround 18.48 -> 40.02 s, so overall improvement is not shown (aios p. 9). Consistency at temperature 0 does not test sampling-enabled determinism (interpretation).

## Related

- [[agent-scheduler]]
- [[context-snapshot-restore]]
