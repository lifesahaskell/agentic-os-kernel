---
title: Harness Robustness Mechanisms
type: concept
sources: [aide2]
tags: [agentic, security]
---

# Harness Robustness Mechanisms

Prompt guards, a selection rule that penalizes lucky scores, and a repair of a broken evaluation
script, which together make an agent less likely to report or pick an inflated result (aide2
p. 10).

## Mechanisms

- Prompt safeguards: a fixed instruction that solutions are scored on a private split and should
  be robust and generalizable, plus a guard that re-prompts when generated code is nearly empty,
  under 40 characters (aide2 p. 10).
- Selection rule that penalizes each candidate by its distance from the median of the top
  candidates. Replayed on held-out runs it "never changed which candidate the agent selected"
  (aide2 p. 10).
- Evaluation repair: one task's held-out scoring script crashed on all its test cases if any one
  failed. AIDE85 adds "a small patch" so that a single failed test case does not take down the
  evaluation. The paper writes that "rather than exploiting the broken evaluation, AIDE2 repaired
  it" (aide2 p. 10).
- Fig. 2 labels step 6 "demands a root-cause diagnosis before any fix", step 39 "repairs itself
  after a crashed robustness fix attempt", step 47 "patches an error that crashed evaluations"
  (aide2 p. 6). The paper does not say step 47 is the evaluation patch.
- Rejected analogues: robust near-tie overrides and promote-second-best, five or more variants,
  best delta -0.007, within noise (aide2 p. 27).
- The authors say these "may account for the reduced reward hacking" (aide2 p. 8); see
  [[reward-hacking-measurement]].

## Trade-offs

- The median-penalty rule changed no selections in replay, so its measured contribution is nil
  (aide2 p. 10).
- The paper does not identify which rewrite reduced reward hacking (aide2 p. 8).

## Agentic OS relevance

*(interpretation)* The evaluation patch is the item to read twice. The paper does not say where
the patch lives, in an agent-side wrapper or in the scoring script, or what write access allowed
it (aide2 p. 10). If it edited the scoring script, the grader was inside the agent's write scope.
The agent repaired it in good faith here; the same write access would have allowed the opposite.
Honest behavior in one run is not a protection boundary ([[trusted-computing-base]],
[[principle-of-least-privilege]]). The grader belongs outside the agent's authority regardless of
what the agent does with it.

## Related

- [[private-held-out-grading]]
- [[recursive-self-improvement-loop]]
- [[isolation-and-protection]]
