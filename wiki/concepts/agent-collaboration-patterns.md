---
title: Four Agent Collaboration Patterns
type: concept
sources: [recursivemas]
tags: [agentic]
---

# Four Agent Collaboration Patterns

The four multi-agent topologies into which RecursiveMAS is instantiated to show it is
structure-agnostic: Sequential, Mixture, Distillation and Deliberation (recursivemas pp. 3, 4, 10).
The paper does not fix one pattern because multi-agent architectures vary across tasks
(recursivemas p. 3).

## Mechanisms

- **Sequential**: a chain of three agents, Planner, Critic and Solver, that decompose, judge,
  refine and solve (recursivemas p. 4). Light uses Qwen3-1.7B, Llama3.2-1B-Instruct and
  Qwen2.5-Math-1.5B-Instruct; Scaled uses Gemma3-4B-it, Llama3.2-3B-Instruct and Qwen3.5-4B
  (recursivemas p. 8).
- **Mixture**: Math, Code and Science specialists reason over the input in parallel and a
  Summarizer agent aggregates them (recursivemas p. 4). The Summarizer's prompt receives all three
  specialists' latent embeddings (recursivemas p. 28).
- **Distillation**: a larger Expert paired with a smaller, faster Learner, to pass the Expert's
  knowledge to the Learner while keeping generation fast. Learner Qwen3.5-4B, Expert Qwen3.5-9B
  (recursivemas pp. 4, 8).
- **Deliberation**: a Reflector paired with a Tool-Caller that can invoke a Python environment or
  the Tavily search API. They critique each other until consensus, then the Tool-Caller emits the
  final answer (recursivemas pp. 4, 25).
- Results against the strongest single agent of each pattern: Mixture improves by 6.2% (absolute
  points, on recomputation) (recursivemas p. 11); Deliberation by 4.8% (recursivemas p. 11);
  Distillation improves the Learner by 8.0% and keeps a 1.5x end-to-end speed advantage over the
  Expert (recursivemas pp. 11, 26).
- Distillation, Table 6, AIME2026 / GPQA-Diamond / LiveCodeBench / MBPP+ / MedQA: Expert
  90.0 / 72.7 / 46.2 / 73.4 / 86.0; Learner 76.7 / 61.4 / 38.4 / 67.5 / 77.9; RecursiveMAS
  83.3 / 70.0 / 40.1 / 71.9 / 83.0 (recursivemas p. 26).
- Training targets differ per pattern; for Sequential, Planner and Critic targets are rewritten by
  Qwen3.5-397B-A17B (recursivemas p. 24).

## Trade-offs

- In Distillation, RecursiveMAS stays below the Expert on every listed benchmark (recursivemas
  pp. 11, 26).
- Mixture, Deliberation and Distillation are compared only with single agents from the same
  pattern, not with Recursive-TextMAS or the other multi-agent baselines (recursivemas pp. 11, 26).

## Agentic OS relevance

*(interpretation)* The four patterns are a pipeline, a fan-in, a heavy/light pair and a
tool-calling pair. Each is a different message graph over the same loop primitive, which is what a
kernel's message layer must support. Deliberation is the only one with side effects, and the
paper's traces show tool calls only in the final text round (recursivemas pp. 33-36). Whether a
tool could be called during a latent round is not stated, and it is the case a reference monitor
would care about ([[access-control]], [[tool-manager]]).

## Related

- [[recursive-multi-agent-system]]
- [[agent-as-process]]
- [[agent-scheduler]]
