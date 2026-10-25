---
title: Private Held-Out Grading
type: concept
sources: [aide2]
tags: [agentic, security]
---

# Private Held-Out Grading

The inner agent optimizes a public task signal while the outer loop selects on a separate score
computed from private held-out data that the inner agent never sees (aide2 p. 3).

## Mechanisms

- Two signals: the public score the agent observes, and a private score aggregated as the mean over
  tasks, each task run several times and averaged (aide2 p. 3).
- The separation "prevents the inner-loop agent from directly optimizing the criterion used for
  outer-loop selection" (aide2 p. 4).
- Every agent gets the same per-task budget, so gains must come from a better algorithm rather
  than more compute (aide2 p. 4).
- The selection benchmark spans three task families (ML engineering, heuristic algorithm
  engineering, harness engineering) to push toward general mechanisms over task-specific tricks
  (aide2 pp. 3, 5).
- A second layer, held-out evaluation, uses four external benchmarks that never affected
  candidate selection (aide2 p. 6). Results are on [[aide2]].
- About a quarter of rejected graded rewrites scored higher than the incumbent on the public
  signal and were rejected on the private grade (aide2 p. 26).
- The discovered agent AIDE85 carries a fixed prompt line saying solutions are scored on a private
  split it cannot see (aide2 p. 10).

## Trade-offs

- Selecting on the grade means the grade cannot show generalization; separate benchmarks are
  needed (aide2 p. 5).
- Noise in agent seeds and evaluation can accept a rewrite falsely (aide2 p. 13).
- Selection-benchmark size and the public/private split are not given in the text.

## Agentic OS relevance

*(interpretation)* The closest OS analogue is a reference monitor whose policy input the subject
cannot read or edit ([[access-control]]). The paper enforces the hiding by protocol and by a
prompt reminder, and describes no isolation mechanism that makes private labels unreadable to the
agent. The prompt line is a request to the model, not an enforcement point. The same pattern in
this project would be an evaluator that runs as a separate principal and is reachable by the
agent only through a message endpoint, never through a handle to its data ([[capabilities]]).

## Related

- [[recursive-self-improvement-loop]]
- [[reward-hacking-measurement]]
- [[trusted-computing-base]]
