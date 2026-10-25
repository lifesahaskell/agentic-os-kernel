---
title: RecursiveLink
type: concept
sources: [recursivemas]
tags: [agentic]
---

# RecursiveLink

A two-layer residual projection module that maps one embedding space to another. It is used inside
an agent (inner link) and between heterogeneous agents (outer link) so that latent states pass
without being decoded to text (recursivemas pp. 2, 4).

## Mechanisms

- It bridges two transitions: dense-to-shallow, where one step's last-layer embedding becomes the
  next step's input embedding, and cross-model, where one model's new latent representations
  condition another model (recursivemas p. 4).
- Inner link: R_in(h) = h + W2 sigma(W1 h), with sigma = GELU. The residual keeps the original
  latent semantics (recursivemas p. 5).
- Outer link: R_out(h) = W3 h + W2 sigma(W1 h). The extra linear W3 maps the source agent's
  embedding space to the target's, so hidden sizes may differ (recursivemas p. 5).
- Inside an agent, e(t+1) = R_in(h(t)) repeated for m steps gives the latent thoughts. Across
  agents, the next agent is conditioned on its own input embeddings concatenated with the outer
  link's output (recursivemas p. 5).
- The residual form lets the module learn only the difference between distributions rather than a
  full projection, which the authors say makes training more stable (recursivemas p. 5).
- Design ablation, scaled sequential, Math500 / GPQA-Diamond / LiveCodeBench: 1-layer
  84.4 / 63.2 / 40.1; residual with 1 layer 86.7 / 65.3 / 41.4; 2-layer 85.6 / 64.5 / 40.5;
  residual with 2 layers (chosen) 88.0 / 66.2 / 42.9 (recursivemas p. 12).
- Trainable size on the scaled sequential system: 13.12M parameters, 0.31% (recursivemas p. 13).
- Gradient property: with a Kaiming-initialised W1 and W2 and W3 equal to the identity, the
  Jacobian's spectral norm is bounded below by a quantity near 1 with high probability
  (recursivemas pp. 7, 22).
- Per-step cost is an extra Theta(m d_h^2), replacing the Theta(m |V| d_h) cost of decoding to the
  vocabulary (recursivemas p. 21).

## Trade-offs

- The gradient bound is proven only for W3 equal to the identity and Kaiming-normal initialisation
  (recursivemas p. 21).
- The paper does not give the hidden sizes or the intermediate widths of W1 and W2 (not located).

## Agentic OS relevance

*(interpretation)* The links are the only trained state, so they are the portable unit of how a
group of agents talks. One outer link exists per ordered agent pair, so link count follows the
topology, not the agent size. A learned adapter between embedding spaces is also the one place
where a kernel could meter, rate-limit or label inter-agent traffic without reading it, since the
payload is opaque. See [[latent-vs-text-agent-communication]] and [[agent-recursion-os-implications]].

## Related

- [[recursive-multi-agent-system]]
- [[inner-outer-loop-training]]
- [[rpc]]
- [[isolation-and-protection]]
