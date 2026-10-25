---
title: Recursion-Depth Scaling
type: concept
sources: [recursivemas]
tags: [agentic]
---

# Recursion-Depth Scaling

The empirical finding that RecursiveMAS accuracy rises with the number of recursion rounds, with
training-time depth and inference-time depth contributing complementary gains (recursivemas
pp. 9, 10).

## Mechanisms

- Evaluated at 1, 2 and 3 rounds (recursivemas p. 9). Across seven math, science and code tasks
  both the Light and Scaled versions improve with depth (recursivemas pp. 9, 10).
- Gain over Recursive-TextMAS in Table 2: up 3.4, 6.0 and 7.2 points at 1, 2 and 3 rounds
  (recursivemas p. 9). The text beside it gives 8.1%, 19.6% and 20.2%, which are relative gains
  (recursivemas p. 10).
- Scaled Math500: RecursiveMAS 86.3, 87.1, 88.2; Recursive-TextMAS 84.2, 84.4, 85.8. Scaled
  AIME2025: RecursiveMAS 80.0, 86.0, 86.7; Recursive-TextMAS 71.3, 70.7, 73.3 (recursivemas p. 9).
  Recursive-TextMAS is non-monotonic on several columns, for example Light Math500 71.9, 72.5, 69.1
  (recursivemas p. 9).
- Scaling claim: more inference depth keeps improving systems trained with fewer rounds, deeper
  training shifts the whole frontier up, and the best results need both (recursivemas p. 10).
  The authors read training recursion as teaching refinement-ready latent states and inference
  recursion as turning them into test-time gains (recursivemas p. 10).
- Representation evidence: a PCA of 500 QA pairs shows generated-answer embeddings shifted from the
  ground truth at round 1 and largely aligned by round 3 (recursivemas p. 12). A case study has a
  wrong MATH500 answer at round 1 corrected at round 2 (recursivemas pp. 30, 31).

## Trade-offs

- Wall-clock time still grows with depth: scaled Math500 takes 1701 s, 1974 s and 2320 s at 1, 2
  and 3 rounds (recursivemas p. 9). The paper notes deeper recursion introduces cost
  (recursivemas p. 11).
- Only up to 3 rounds is reported (recursivemas pp. 9, 10). The average standard deviation over 5
  runs is 0.0041 in accuracy, 26 in runtime and 33 in tokens (recursivemas p. 9).
- The paper does not say whether each row of Table 2 used a model trained at that depth
  *(interpretation)*; Figure 1 treats training and inference depth as separate axes (recursivemas
  p. 1).

## Agentic OS relevance

*(interpretation)* Recursion depth is a knob that converts compute into accuracy at run time. If
agents are kernel objects, depth is a per-principal resource: something to budget, account for and
cap, like CPU time ([[scheduling]]). The paper's cost axis is wall-clock time and decoded tokens;
since latent steps do not appear to count as tokens, a token-denominated budget would not see this
spending. See [[latent-vs-text-agent-communication]].

## Related

- [[recursive-multi-agent-system]]
- [[inner-outer-loop-training]]
