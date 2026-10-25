---
title: Inner-Outer Loop Training
type: concept
sources: [recursivemas]
tags: [agentic]
---

# Inner-Outer Loop Training

A two-stage procedure that first warm-starts each agent's inner link in isolation, then unrolls the
whole agent loop for n rounds and backpropagates one cross-entropy loss through every outer link
(recursivemas pp. 6, 7).

## Mechanisms

- Only the links are trained and the base models are frozen (recursivemas pp. 7, 9).
- Inner loop (model level): runs per agent, in parallel across agents. The target is the ground-truth
  text passed through the agent's own input embedding layer, and the loss is 1 minus the cosine
  similarity between the link's output and that target (recursivemas pp. 6, 7). The aim is to align
  latent thoughts with the input-embedding distribution without decoding and re-encoding
  (recursivemas p. 7).
- Outer loop (system level): unroll the system for n rounds, produce text only in the last round,
  and optimise all outer links with cross-entropy on the final output (recursivemas p. 7).
- Credit assignment: the computation graph is kept along the whole recursive path, so each outer
  link receives a shared credit signal according to its contribution to the final prediction
  (recursivemas p. 7).
- Theorem 4.1 (gradient stability): under stated assumptions, when tokens are confident (entropy at
  most epsilon, small), text-based SFT through the vocabulary has gradient norm at most O(epsilon),
  which vanishes, while the RecursiveLink path has gradient norm bounded below near 1 (recursivemas
  pp. 7, 22).
- Settings: AdamW, learning rate 5e-4, cosine schedule, batch size 4, maximum sequence length 4096
  (recursivemas pp. 9, 24). Data: s1K, m1K, OpenCodeReasoning and ARPO-SFT (recursivemas pp. 9, 24).
- Cost, scaled sequential: RecursiveMAS 15.29 GB peak memory, 13.12M trainable (0.31%), $4.27,
  average accuracy 74.9; LoRA 21.67 GB, 15.92M (0.37%), $6.64, 66.9; full SFT 41.40 GB, 4.21B
  (100%), $9.67, 68.6 (recursivemas p. 13).

## Trade-offs

- The theorem assumes W3 equal to the identity and Kaiming-normal W1 and W2 (recursivemas p. 21).
- Backward memory scales with the unrolled rounds *(interpretation)*; the paper's cost study fixes
  one setting only (recursivemas p. 13).
- The paper does not say whether inner links stay frozen during the outer loop. Its wording
  suggests they are trained there too (recursivemas pp. 2, 7) *(interpretation)*.
- "Shared gradient credit" is ordinary end-to-end backpropagation through an unrolled graph; the
  difference is that the graph is not cut at the text boundary *(interpretation)*.
- Table 5's 4.21B = 100% denominator looks like one agent, not the three-agent system (recursivemas
  p. 13) *(interpretation)*.

## Agentic OS relevance

*(interpretation)* Training here changes how agents talk, not what they know. The artifact that
results is a set of small link weights tied to specific agents' embedding spaces. A system that
swaps an agent invalidates its links. Unlike [[recursive-self-improvement-loop]], no agent code is
rewritten, so the authority question is narrower: what must be writable is the link weights, not the
agents.

## Related

- [[recursivelink]]
- [[recursive-multi-agent-system]]
- [[mechanism-vs-policy]]
