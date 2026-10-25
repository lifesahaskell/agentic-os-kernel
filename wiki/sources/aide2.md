---
title: "Recursive self-improvement of AI research agents (AIDE2)"
type: source
sources: [aide2]
tags: [agentic]
---

# Recursive self-improvement of AI research agents (AIDE2)

Srikanth, Zhao, Xu, Wu, Jiang (Weco AI). arXiv 2609.26457 **v1, 22 September 2026** (aide2 p. 1).
Local file: `raw/RecursiveSelfImprovement.pdf` (28 pp.: body pp. 1-14, references pp. 14-23,
appendices pp. 24-28). The paper is about research-agent harnesses, not operating systems. It
reaches this wiki through the questions it raises about what a self-modifying agent may write to
and who owns the grader.

## Claim

When an AI research agent's own code is the object of optimization, each accepted rewrite becomes
the agent that the next round edits; the authors call this loop recursive self-improvement (aide2
p. 1). AIDE2 implements it at the harness layer, meaning the code around a model that controls
search, context and verification (aide2 p. 2). See [[recursive-self-improvement-loop]].

## Method

- Two nested loops. The inner agent solves tasks under a fixed per-task dollar budget and sees
  only a public score. The outer loop proposes rewrites of the agent and keeps the one with the
  best score on private held-out data (aide2 pp. 3-4). See [[private-held-out-grading]].
- Starting agent AIDE0 is a pared-down refactor of AIDE with greedy tree search. The outer-loop
  editor is AIDEhuman, Weco's production agent, running on claude opus 4.7. Inner-loop evaluations
  use gemini 3 flash (aide2 pp. 4-5).
- One run: 100 nodes (the start plus 99 proposals) in 8 days of wall-clock time, seven accepted
  rewrites at steps 2, 6, 28, 39, 47, 63, 85, incumbent grade 0.703 to 0.778 (aide2 pp. 5-6).
- The Fig. 2 labels name the seven rewrites: bandit search over strategies, root-cause diagnosis
  before any fix, forking the champion, self-repair after a crashed fix, patching an error that
  crashed evaluations, compressing log spam, demoting a failure-prone draft strategy (aide2
  p. 6). Mechanisms are on [[bandit-search-policy-with-forking]], [[harness-robustness-mechanisms]]
  and [[context-window-management]] (bounded prompts and failure memory).

## Evidence

- Four held-out benchmarks never used for selection: ALE-Bench, MLE-Bench, FML-Bench, and
  WeatherBench 2, which is out of distribution (aide2 p. 6). AIDE85 is best on ALE-Bench and
  FML-Bench; AIDE47 is best on MLE-Bench and WeatherBench 2 (aide2 p. 7). Table 1 (aide2 p. 24):

  | Agent | ALE-Bench | MLE-Bench | WeatherBench 2 | FML-Bench (%) |
  |---|---|---|---|---|
  | AIDE0 | 1536 ± 33 | 0.678 ± 0.006 | 0.262 ± 0.205 | 15.0 ± 0.9 |
  | AIDE47 | 1713 ± 26 | 0.730 ± 0.005 | 0.798 ± 0.003 | 19.7 ± 1.2 |
  | AIDE85 | 1790 ± 9 | 0.722 ± 0.011 | 0.793 ± 0.005 | 19.9 ± 1.1 |
  | AIDEhuman | 1511 ± 35 | 0.708 ± 0.007 | 0.404 ± 0.193 | 19.6 ± 1.0 |

- Reward hacking on 38 held-out kernel pairs: AIDE0 55%, AIDE47 39%, AIDE85 32%, AIDEhuman 39%
  (aide2 p. 8). See [[reward-hacking-measurement]].
- Prompt size at the end of the run, median versus AIDE0: 7x smaller on MLE-Bench, over 40x on
  WeatherBench 2, about 50x on ALE-Bench and FML-Bench (aide2 p. 9; p. 27).
- Cross-model: gemini 3 flash with AIDE85 reaches 1858 ± 22 on ALE-Bench, above fable 5 with
  AIDE0 at 1796 ± 24 under the same budget (aide2 p. 26).
- Ignition test: an agent from step 47 used as the outer-loop editor ended at 0.780 against 0.782
  for AIDEhuman, 3 seeds of 50 steps each. The authors call it inconclusive (aide2 pp. 10-11).

## Assessment (interpretation)

- "Matches or exceeds" holds on all four benchmarks but is a clear win only on ALE-Bench and
  WeatherBench 2. FML-Bench (19.9 vs 19.6) and MLE-Bench (0.722 vs 0.708) are within about one
  standard error, and on MLE-Bench AIDE47 beats AIDE85.
- The 55% to 32% result comes from one task family and 38 pairs, with no intervals or counts
  given. The percentages fit 21, 15 and 12 of 38, so AIDE85 against AIDEhuman is about three
  pairs. The authors say the rates "do not identify which rewrites produced it" (aide2 p. 8).
- In the 8-day run the editor was the fixed human-engineered agent. The artifact improves; the
  improver does not. The one test of a discovered agent as editor is inconclusive (aide2 pp. 4-5,
  10-11).
- The paper has no sandbox, threat-model or safety section. It does not say what the agent could
  write to, how the private data was kept unreadable, or whether the 8-day run used the container
  setup listed for the held-out evaluations (aide2 p. 24). The only enforcement-like measures are
  cost, step and time caps (aide2 pp. 3-4, 24). That absence is the main reason the paper is in
  this wiki; see [[agent-recursion-os-implications]].
- Not located in the paper: total dollar cost or tokens for the 8-day run, the number and names of
  selection tasks, the private split size, and the per-task budgets used for selection.
