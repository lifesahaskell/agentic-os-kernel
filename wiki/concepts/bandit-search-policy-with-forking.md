---
title: Bandit Search Policy with Periodic Forking
type: concept
sources: [aide2]
tags: [agentic]
---

# Bandit Search Policy with Periodic Forking

A search policy that picks which drafting strategy to expand using UCB1 over labeled strategy
arms, and every five steps forks the global best node under a different arm, replacing greedy
best-first search (aide2 p. 9).

## Mechanisms

- Baseline AIDE0 is greedy: the highest-scoring solution is the parent, with no control over
  explore versus refine (aide2 pp. 4, 8).
- Five fixed strategies label the arms: conservative, aggressive_rewrite, ensemble,
  tuned_specialist, robust_simple. Children inherit the parent's label (aide2 pp. 8-9).
- The arm is chosen by UCB1; in 30% of steps an arm is instead sampled by softmax over the arms'
  best scores; the best node carrying the chosen label is expanded (aide2 p. 9).
- Every five search steps the global best node is forked under a different arm. The five-step
  bound avoids interrupting productive refinement (aide2 p. 9).
- On a held-out ALE-Bench task, greedy AIDE0 descends from one draft while AIDE85 keeps several
  arms alive (aide2 p. 9).
- Related Fig. 2 labels: step 2 "bandit search over strategies", step 28 "escapes plateaus by
  forking its champion", step 85 "demotes a failure-prone draft strategy" (aide2 p. 6).
- Alternatives tried and rejected, with the change in selection grade: island populations
  (-0.021), pairwise LLM-judge tournaments (-0.090), stagnation-triggered escalation (-0.040),
  keep-refining versus restart (-0.078), majority-vote ensembles (down to -0.031), variance-adaptive
  UCB-V boosts (-0.080). Explore-rate decay (-0.006) and trend propagation (-0.004) were within
  noise (aide2 pp. 26-27). Ensembling was dropped because it "consumes budget that could otherwise
  fund additional search steps" (aide2 p. 26).

## Trade-offs

- Choosing over arms controls the diversity of approach, not which local optimum is polished
  (aide2 p. 9).
- Small deltas (about -0.004 to -0.007) are within run-to-run variability, and the paper does not
  isolate the bandit's effect from the other six rewrites (aide2 p. 26).

## Agentic OS relevance

*(interpretation)* This is a scheduling policy for an agent's own work queue, separable from the
mechanism that runs candidates, which fits [[mechanism-vs-policy]]. Exploration against
exploitation under a fixed budget is the same shape as the feedback problem in [[mlfq]]
(learn which job is interactive without being told), applied by the agent to itself. The kernel
scheduler ([[scheduling]], [[agent-scheduler]]) sees none of this; it sees only the budget.

## Related

- [[recursive-self-improvement-loop]]
- [[scheduling]]
