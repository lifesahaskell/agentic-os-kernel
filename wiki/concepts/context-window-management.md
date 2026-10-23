---
title: Context Window Management
type: concept
sources: [aios]
tags: [agentic]
---

# Context Window Management

Handling contexts longer than the LLM's window.

## Mechanisms

- Supports basic text summarization (aios p. 6).
- Incorporates other window-expansion techniques, citing positional interpolation and YaRN (aios p. 6; refs [59,60] p. 14).
- Syscalls: get_context (length), exp_context, clr_context, set_context (aios p. 7).

## OS analogue

- Virtual memory / working-set control, loosely. Breaks: summarization is lossy; paging is not (interpretation).

## Related

- [[context-manager]]
- [[paging]]
- [[swapping]]
