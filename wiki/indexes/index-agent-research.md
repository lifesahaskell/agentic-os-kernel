# Index: Agent systems research

Recursion, self-improvement, multi-agent communication. Back to [[index]].

From the two 2026 agent papers. Their OS value is in each page's Agentic OS relevance section.

- [[agent-collaboration-patterns|Four Agent Collaboration Patterns]]: Sequential, Mixture, Distillation and Deliberation, the four topologies into which RecursiveMAS is instantiated.
- [[bandit-search-policy-with-forking|Bandit Search Policy with Periodic Forking]]: UCB1 over labeled drafting strategies, plus a fork of the global best every five steps, replacing greedy best-first search.
- [[harness-robustness-mechanisms|Harness Robustness Mechanisms]]: Prompt guards, a selection rule that penalizes lucky scores, and a repair of a broken evaluation script.
- [[inner-outer-loop-training|Inner-Outer Loop Training]]: Warm-start each agent's inner link, then unroll the whole agent loop and train only the outer links with frozen models.
- [[latent-vs-text-agent-communication|Latent versus Text Inter-Agent Communication]]: The decode-cost argument and measured comparison between passing hidden states and passing decoded text.
- [[private-held-out-grading|Private Held-Out Grading]]: The inner agent optimizes a public signal while the outer loop selects on a private score it never sees.
- [[recursion-depth-scaling|Recursion-Depth Scaling]]: Accuracy rises with the number of recursion rounds, at a growing wall-clock cost.
- [[recursive-multi-agent-system|Recursive Multi-Agent System]]: A multi-agent system cast as one looped latent-space computation.
- [[recursive-self-improvement-loop|Recursive Self-Improvement Loop]]: An outer loop rewrites a research agent and keeps a rewrite only if its private grade improves.
- [[recursivelink|RecursiveLink]]: A two-layer residual projection that passes latent states inside an agent and between agents without decoding to text.
- [[reward-hacking-measurement|Reward Hacking Measured by Proxy-to-Downstream Gap]]: A gain on the optimized signal that does not survive downstream, measured on GPU kernels.
