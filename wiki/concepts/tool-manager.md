---
title: Tool Manager
type: concept
sources: [aios]
tags: [agentic]
---

# Tool Manager

Manages the external API tools agents may call, spanning text and image modalities.

## Mechanisms

- Integrates tools from OpenAGI, LangChain, RapidAPI (aios p. 7, refs [7,62,63]).
- Categories: Search (google_search, bing_search), Computation (currency_converter, wolframalpha), Database Query (sql_query, wikipedia_query, arxiv_search, words_api, urban_dictionary), Image Processing (image_denoising, image_deblurring, image_classification, object_detection, face_rect) (aios p. 7, Table 1).
- Table 1 records input->output formats per tool (aios p. 7).
- Motivation: multiple agents may call the same tool, so call ordering matters (aios p. 2).
- No tool syscalls appear in Table 2; the paper does not specify tool-call arbitration (interpretation).
- Future: schedule locally deployed tool models, including status and snapshots (aios p. 10).

## OS analogue

- Device driver / device manager. Breaks: tools are remote APIs and local models with no uniform driver interface described (interpretation).

## Related

- [[agent-scheduler]]
- [[system-call]]
- [[llm-system-calls]]
