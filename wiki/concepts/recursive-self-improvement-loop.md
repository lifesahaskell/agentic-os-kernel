---
title: Recursive Self-Improvement Loop
type: concept
sources: [aide2]
tags: [agentic]
---

# Recursive Self-Improvement Loop

An outer loop rewrites the code of a research agent, grades each rewrite by running it on tasks
under a fixed budget, and keeps it as the next incumbent only if its private grade improves; each
accepted rewrite becomes the agent that the next step edits and evaluates (aide2 p. 3).

## Mechanisms

- Bi-level structure: the inner loop optimizes code against a task metric; the outer loop
  optimizes the inner-loop agent (aide2 p. 3).
- Inner loop: the agent proposes solutions from prior edits and a public signal until a fixed
  dollar budget is spent, then returns one solution of its choosing (aide2 p. 3).
- Outer loop: an outer agent reads previously proposed agents with their grades and proposes a
  rewrite of the current incumbent, the best agent graded so far (aide2 p. 3).
- Acceptance is a fixed rule, the agent with the highest grade so far; the inner loop's own
  selection policy is editable, the outer rule is not (aide2 p. 4).
- Editable surface includes which candidate to edit next, which to return, and how memory is
  structured (aide2 p. 3). Models are held fixed within each loop (aide2 p. 5).
- Instantiation: inner agent AIDE0, outer agent AIDEhuman on claude opus 4.7, inner evaluations on
  gemini 3 flash (aide2 pp. 4-5).
- One run produced 100 nodes in 8 days with seven accepted rewrites and incumbent grade rising
  from 0.703 to 0.778; two other complete runs accepted two and four rewrites (aide2 p. 5).
- Motivation: cumulative R&D spending shows diminishing returns, and sustained self-improvement is
  offered as a way to counter this (aide2 p. 1). The paper reports no returns curve; this is
  background, not a result.
- Ignition test: gains would turn diminishing returns into accelerating ones only if discovered
  agents drive the loop better than the agent that discovered them. A step-47 agent as outer
  editor reached 0.780 against 0.782 for AIDEhuman, 3 seeds of 50 steps, and the authors call it
  inconclusive (aide2 pp. 10-11).

## Trade-offs

- Noise compounds across both loops, and one noisy comparison can install a false incumbent that
  derails later search (aide2 p. 13).
- Discovered agents are complex and hard to interpret; it is unclear which components drive
  performance and which are leftovers from earlier steps (aide2 p. 13).
- The paper says its trace on the selection grade "is not meant to demonstrate generalization",
  since candidates were selected on it (aide2 p. 5). See [[private-held-out-grading]].
- A definitive test of the ignition hypothesis is called prohibitively costly (aide2 pp. 10-11,
  13).

## Agentic OS relevance

*(interpretation)* The loop is propose, sandbox-evaluate, promote. The paper describes the first
and third steps and leaves the middle one unspecified: nothing says what the proposer could write
to, or whether the grader sat in a separate trust domain (aide2 p. 24 lists only evaluation
hardware). In an OS where agents are kernel objects, "rewrite my own code" is a write capability
on the agent's own image plus a grader the agent holds no handle to. Acceptance without a human
gate (aide2 pp. 1, 4) makes a falsely accepted rewrite, which the authors flag as a real risk
(aide2 p. 13), an unreviewed change to a running principal. See [[agent-recursion-os-implications]],
[[isolation-and-protection]] and [[trusted-computing-base]].

## Related

- [[private-held-out-grading]]
- [[harness-robustness-mechanisms]]
- [[reward-hacking-measurement]]
- [[bandit-search-policy-with-forking]]
- [[agent-as-process]]
