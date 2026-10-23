---
title: "AIOS: LLM Agent Operating System"
type: source
sources: [aios]
tags: [agentic]
---

# AIOS: LLM Agent Operating System

Mei, Li, Xu, Ye, Ge, Zhang (Rutgers). arXiv 2403.16971 **v2, 26 March 2024**. Local file:
`raw/aios-rutgers-2026-03.pdf`; the "2026-03" in the name is not the paper's date. Code:
github.com/agiresearch/AIOS (aios p. 1). Later versions of the paper exist and are **not**
ingested. Features found only in later versions, such as text-based vs logits-based snapshots
and user confirmation for irreversible operations, are not in this text.

## Claim

LLM agents suffer from poor scheduling of requests over the LLM, from losing context across
interactions, and from the difficulty of integrating heterogeneous agents (aios p. 1). AIOS puts
an "LLM kernel" next to the OS kernel to manage these as OS resources (aios p. 4).

## Architecture

The design has three layers: application (agents + [[aios-sdk]]), kernel (OS kernel + LLM
kernel), and hardware. LLM-kernel calls reach hardware only through OS syscalls (aios p. 4). See
[[aios-architecture]]. The LLM kernel's modules, behind [[llm-system-calls]]:

- [[agent-scheduler]]: FIFO, round robin (aios p. 5)
- [[context-manager]]: [[context-snapshot-restore]] and [[context-window-management]] (aios p. 6)
- [[memory-manager]]: per-agent short-term memory (aios p. 6)
- [[storage-manager]]: long-term persistence (aios pp. 6-7)
- [[tool-manager]]: 14 tools in 4 categories (aios p. 7)
- [[access-manager]]: privilege groups and audit logs (aios pp. 7-8)

## Evidence

The setup is 3 agents (Math, Narrative, Rec), 3 local backbones (Gemma-2b/7b, LLaMA-2-13b) and
8x RTX A5000 (aios p. 8). At temperature 0, outputs under suspend/resume match single-agent runs
(BLEU = BERTScore = 1.0) (aios p. 9). FIFO scheduling shortens the last agent's wait (LLaMA-2-13b
Rec: 53.86 s to 34.92 s) and lengthens the first agent's (Math turnaround: 18.48 s to 40.02 s)
(aios p. 9). See [[evaluation-aios]].

## Assessment (interpretation)

- The evaluation shows that latency is redistributed. It does not show a speedup. There are no
  throughput, mean-latency or snapshot-cost numbers.
- Access control is between agents only, and Table 2 has no access or tool syscalls (aios p. 7).
  Mediation is therefore advisory.
- The OS analogies are useful vocabulary. Where each one breaks is recorded in
  [[aios-ostep-mapping]].
