---
title: Latent versus Text Inter-Agent Communication
type: concept
sources: [recursivemas]
tags: [agentic, distribution]
---

# Latent versus Text Inter-Agent Communication

The complexity argument and measured comparison between passing hidden states through a
RecursiveLink and passing decoded text between agents in the same recursive structure
(recursivemas pp. 6, 11).

## Mechanisms

- The comparison baseline is Recursive-TextMAS: the same recursive collaboration structure with
  agents communicating through explicit text (recursivemas pp. 6, 8).
- Proposition 3.1: for N agents, context length at most t and generation length at most m, the
  text-based version costs Theta(N(m|V|d_h + (t+m)d_h^2 + (t+m)^2 d_h)) and RecursiveMAS costs
  Theta(N(m d_h^2 + (t+m)d_h^2 + (t+m)^2 d_h)) (recursivemas pp. 6, 21).
- Since d_h is much smaller than the vocabulary size |V|, the per-step decode cost is replaced by
  the latent transform (recursivemas p. 6). The shared Transformer terms are identical; only decode
  differs (recursivemas p. 21).
- Measured against Recursive-TextMAS: average speedup 1.2x, 1.9x and 2.4x at 1, 2 and 3 rounds, and
  token reduction 34.6%, 65.5% and 75.6% (recursivemas p. 11, Figs. 5 and 6). All measurements use
  the same framework structure and recursion budgets (recursivemas p. 9).
- Reason given: Recursive-TextMAS decodes intermediate text at every round, while RecursiveMAS does
  most of its interaction in latent space (recursivemas pp. 11, 12).
- Token counts, Light Math500 at 1, 2 and 3 rounds: RecursiveMAS 523, 495, 519; Recursive-TextMAS
  1185, 2117, 3059 (recursivemas p. 9).
- Related work on latent communication cited in the appendix includes hidden-embedding transfer,
  internal-state and KV reuse, and agentic latent interfaces (recursivemas p. 26).

## Trade-offs

- The complexity result compares decode cost only (recursivemas p. 21).
- Speedup and token figures are against one baseline (recursivemas p. 9). The 2- and 3-round
  speedups do not reproduce from Table 2: summed time gives about 1.8x and 2.3x
  (recursivemas p. 9) *(interpretation)*.
- The paper does not define whether the latent steps are counted as tokens (not located).

## Agentic OS relevance

*(interpretation)* This is a trade between auditability and throughput that the kernel owns. Text
messages are inspectable and cost O(m|V|d_h) to produce; latent messages are cheap and opaque. If
policy checks, audit logs and prompt-injection filters operate on message content (as in
[[access-manager]]), the latent path bypasses all of them. The kernel can still stamp sender and
receiver and meter size and count, which makes the metadata path the only enforceable one. A
text-only interface remains the mode in which a mediator can look inside. See
[[agent-recursion-os-implications]], [[distributed-systems-communication]] and [[rpc]].

## Related

- [[recursivelink]]
- [[recursive-multi-agent-system]]
- [[context-window-management]]
