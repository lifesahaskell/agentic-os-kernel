---
title: Context Snapshot and Restore
type: concept
sources: [aios]
tags: [agentic]
---

# Context Snapshot and Restore

Mechanism to save the state of an LLM's partially complete generation when the scheduler suspends a request, and reload it to resume exactly where it stopped.

## Mechanisms

- Needed because time-quantum scheduling suspends requests before the response is finished (aios p. 6).
- Illustrated with beam search, beam width 1 (aios p. 6).
- Snapshot captures the current beam search tree: intermediate probabilities and explored paths (aios p. 6).
- Restore reloads the saved state so generation continues from the suspension point to the final answer ("Search weather in Paris") (aios p. 6).
- Syscalls gen_snapshot / gen_restore (aios p. 7).
- Not in this paper version: any distinction between text-based and logits-based snapshots. The paper describes only saving search-tree state (interpretation: that is closest to a logits/probability-based approach; a text-only variant is not described).
- Future: faster snapshot/restore and compressing context before snapshotting (aios p. 10).

## Evaluation

- Consistency check, temperature 0: BLEU = BERT = 1.0 across all three agents and three backbones (aios p. 9, Table 4).

## OS analogue

- Context switch (saving/restoring registers and PC). Breaks: state is a decoding search tree, not registers, and its size and cost are model-dependent and unquantified in the paper (interpretation).

## Related

- [[context-switch]]
- [[agent-scheduler]]
- [[context-manager]]
- [[context-window-management]]
