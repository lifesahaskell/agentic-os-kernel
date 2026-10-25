---
title: Context Window Management
type: concept
sources: [aios, aide2]
tags: [agentic]
---

# Context Window Management

Handling contexts longer than the LLM's window.

## Mechanisms

- Supports basic text summarization (aios p. 6).
- Incorporates other window-expansion techniques, citing positional interpolation and YaRN (aios p. 6; refs [59,60] p. 14).
- Syscalls: get_context (length), exp_context, clr_context, set_context (aios p. 7).
- Bounded prompts (aide2): the starting agent AIDE0 puts the full concatenated history of candidates and execution output in every draft and improve prompt, so prompts grow with the run (aide2 pp. 8, 27). The discovered AIDE85 instead reads "a compact summary of the root and recent candidates", and per-step prompt size stays roughly constant (aide2 p. 9). Median prompt size against AIDE0 is 7x smaller on MLE-Bench, over 40x on WeatherBench 2 and about 50x on ALE-Bench and FML-Bench (aide2 p. 9; p. 27).
- Failure memory (aide2): when at least 15% of candidates are buggy, up to three recurring error signatures (deduplicated final error lines of recent buggy candidates) are injected into the prompt; otherwise the memory stays dormant. The authors call the 15% threshold a design choice (aide2 p. 9).
- The failure it removes: AIDE0 terminates when its prompt exceeds the model's context window, which ended 5 FML-Bench runs and 48 ALE-Bench runs at the larger budget; AIDE85, AIDE47 and AIDEhuman had none (aide2 pp. 24, 27).
- No ablation isolates compression from the other rewrites; the authors say it is unclear which components drive performance (aide2 p. 13).

## OS analogue

- Virtual memory / working-set control, loosely. Breaks: summarization is lossy; paging is not (interpretation).
- The AIDE2 mechanisms are closer to an explicit working-set policy: a bounded resident set (root plus recent candidates) with older history evicted, and a small error-signature cache that is consulted only under pressure (a high bug rate). See [[working-set-and-thrashing]] (interpretation).
- Unlike the AIOS syscalls above, both mechanisms are chosen by the agent's own harness code, not by a kernel context manager. In a system where an agent can rewrite that code, context policy is agent-owned unless the kernel holds the budget ([[memory-manager]], [[recursive-self-improvement-loop]]) (interpretation).

## Related

- [[context-manager]]
- [[memory-manager]]
- [[working-set-and-thrashing]]
- [[paging]]
- [[swapping]]
- [[harness-robustness-mechanisms]]
