---
title: "Recursive Multi-Agent Systems (RecursiveMAS)"
type: source
sources: [recursivemas]
tags: [agentic]
---

# Recursive Multi-Agent Systems (RecursiveMAS)

Yang, Zou, Pan, Qiu, Lu, Diao, Jiang, Tong, Zhang, Buehler, He, Zou (UIUC, Stanford, NVIDIA, MIT).
arXiv 2604.25917 **v1, 28 April 2026** (recursivemas p. 1). Local file: `raw/RecursiveAgents.pdf`
(36 pp.: body and references pp. 1-19, appendices pp. 20-36). Project page:
recursivemas.github.io (recursivemas p. 1). The paper has no limitations section.

## Claim

Looped language models reuse one computation over latent states to deepen reasoning. The paper
extends that from one model to a multi-agent system, so that agent collaboration is scaled by
recursion (recursivemas p. 1). See [[recursive-multi-agent-system]].

## Method

- Agents are connected in a loop by RecursiveLink, a lightweight residual projection module,
  inside an agent (inner) and between agents (outer). Only the final recursion round decodes text
  (recursivemas pp. 2, 5). See [[recursivelink]] and [[latent-vs-text-agent-communication]].
- All agent weights stay frozen and only the links are trained, in two stages: each agent's inner
  link is warm-started alone, then the whole loop is unrolled and one loss is backpropagated
  through every outer link (recursivemas pp. 7, 9). See [[inner-outer-loop-training]].
- Four topologies show the method is structure-agnostic: Sequential, Mixture, Distillation and
  Deliberation (recursivemas pp. 4, 8). See [[agent-collaboration-patterns]].
- Accuracy and latency are reported at 1, 2 and 3 recursion rounds (recursivemas p. 9). See
  [[recursion-depth-scaling]].

## Evidence

- Nine benchmarks: MATH500, AIME2025, AIME2026, GPQA-Diamond, MedQA, LiveCodeBench-v6, MBPP Plus,
  HotpotQA, Bamboogle. AIME is scored Pass@10 (recursivemas p. 8). HotpotQA and Bamboogle appear
  only in the Deliberation results (recursivemas p. 26).
- Baselines: single agent with LoRA, single agent with full SFT, Mixture-of-Agents, TextGrad,
  LoopLM, and Recursive-TextMAS (same loop, text between agents) (recursivemas pp. 8, 24).
- Headline claims, all in the abstract (recursivemas p. 1):

  | Claim | Measured against |
  |---|---|
  | 8.3% average accuracy gain | the strongest baseline on each benchmark, Table 3, 3 rounds, six benchmarks (recursivemas p. 10) |
  | 1.2x-2.4x end-to-end speedup | Recursive-TextMAS at 1 and 3 rounds (recursivemas pp. 9, 11) |
  | 34.6%-75.6% fewer tokens | Recursive-TextMAS at 1 and 3 rounds (recursivemas pp. 9, 11) |

- Table 3, 3 rounds, RecursiveMAS against the best baseline per column: MATH500 88.0 vs 85.8,
  AIME2025 86.7 vs 73.3, AIME2026 86.7 vs 76.7, GPQA-Diamond 66.2 vs 62.8, LiveCodeBench 42.9 vs
  39.8, MedQA 79.3 vs 77.2 (recursivemas p. 10).
- Training cost on the scaled sequential system: 13.12M trainable parameters (0.31%), 15.29 GB
  peak GPU memory, $4.27, against $9.67 for full SFT (recursivemas p. 13).

## Assessment (interpretation)

Checked by recomputing from the paper's own tables:

- The 8.3% reproduces as a mean *relative* gain over the per-column best baseline (5.70 absolute
  points). The Mixture result of 6.2% (recursivemas p. 11) reproduces as absolute points, so "%"
  changes meaning between claims.
- The token reductions reproduce exactly from Table 2 (34.6%, 65.5%, 75.6%). The speedups at 2 and
  3 rounds do not: Table 2 gives about 1.8x and 2.3x from summed time, against the stated 1.9x and
  2.4x (recursivemas pp. 9, 11). Figure 5 may average a wider set than Table 2.
- RecursiveMAS values for the scaled 3-round system differ between Table 2 (88.2, 86.0, 42.8) and
  Tables 3 and 4 (88.0, 86.7, 42.9) (recursivemas pp. 9, 10, 12).
- The paper says baselines use identical backbones (recursivemas p. 10) but LoopLM is Ouro-2.6B
  (recursivemas p. 24). The Light setting is described as "sub-1.5B" (recursivemas p. 1) while
  Table 1 includes a 1.7B model (recursivemas p. 8).
- No latent-communication baseline is compared, though the appendix cites several (recursivemas
  p. 26). "Better than text MAS" is not "better than other latent MAS".
- Training targets for the Planner and Critic are rewritten by a 397B model (recursivemas p. 24),
  which confounds comparison with single agents trained on the original answers.
- The paper does not define whether the latent steps count as tokens. Token usage stays nearly
  flat in the number of rounds (recursivemas p. 9), which suggests they do not.
- For this wiki the paper matters for one property: between rounds the agents exchange continuous
  vectors that no one can read. See [[agent-recursion-os-implications]].
