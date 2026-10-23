---
title: Context Manager
type: concept
sources: [aios]
tags: [agentic]
---

# Context Manager

Kernel module responsible for the context given to the LLM and for the generation process under that context.

## Mechanisms

- Two functions: snapshot/restoration and context window management (aios p. 6).
- Context syscalls: get_context, exp_context, clr_context, set_context, gen_snapshot, gen_restore (aios p. 7).
- Ensures suspension of one request loses no progress (aios p. 6).

## Evaluation

- see context-snapshot-restore (aios p. 9).

## OS analogue

- Part of process/context management plus MMU-like window control. Breaks: no hardware enforcement; limit is the model's context length (interpretation).

## Related

- [[context-snapshot-restore]]
- [[context-window-management]]
- [[memory-manager]]
