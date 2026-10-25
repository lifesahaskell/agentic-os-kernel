---
title: Recursive Multi-Agent System
type: concept
sources: [recursivemas]
tags: [agentic]
---

# Recursive Multi-Agent System

A multi-agent system cast as one looped latent-space computation: each agent plays the role of a
layer in a recursive language model, and the last agent's latent output is fed back to the first
for another round (recursivemas pp. 1, 2).

## Mechanisms

- Premise: a looped language model reuses the same computation over latent states to deepen
  reasoning; the paper lifts this from one model to a whole system (recursivemas p. 1).
- A recursive LM reuses a Transformer of L blocks n times, with H(0) = E and H(r) = f(H(r-1))
  (recursivemas p. 3).
- Latent generation: instead of projecting the last hidden state to the vocabulary, the hidden
  state is fed straight back as the next input, giving an ongoing latent thought (recursivemas
  p. 3). The inner link performs that feedback (recursivemas p. 5); see [[recursivelink]].
- System model: N agents, each its own Transformer, with a collective latent state made of every
  agent's last-layer representations (recursivemas p. 3). Definition 2.1 describes recursive
  multi-agent evolution as progressive refinement of that state over n rounds (recursivemas p. 3).
- Loop: agent 1 produces latent thoughts and passes them to agent 2 over the outer link, and so on
  to agent N, whose latent output returns to agent 1 (recursivemas p. 5).
- Only agent N decodes text, and only after the final round; every intermediate round is entirely
  latent (recursivemas p. 5). Each round conditions on what earlier rounds produced, so agents can
  reflect on earlier system outputs (recursivemas p. 5).
- Latent thought length: accuracy on Math500 rises from 83.3 with no latent steps to 86.8 at 80
  steps and then saturates, 86.7 at 128 (recursivemas pp. 13, 27).
- Heterogeneous agents are supported; the paper uses Qwen, Llama, Gemma and Mistral-derived models
  (recursivemas pp. 3, 8).
- All agent parameters are frozen and only the links are trained (recursivemas p. 9); see
  [[inner-outer-loop-training]].
- Motivation: prompt-based adaptation cannot improve the agents themselves, training whole agents
  is hard, and text exchange adds latency because each agent waits for the previous one to finish
  generating (recursivemas p. 2). The paper calls this the first attempt to extend recursive
  scaling to the system level (recursivemas p. 14).

## Trade-offs

- The paper has no limitations section. Its gradient-stability result covers only one special case
  of the link (recursivemas p. 21).
- The setup needs access to every agent's hidden states and input embeddings, which the paper does
  not discuss as a limit *(interpretation)*.

## Agentic OS relevance

*(interpretation)* Intermediate rounds leave no human-readable trace. A kernel that stamps agent
identity on messages ([[capabilities]]) and mediates by policy ([[access-control]]) has nothing to
inspect in a latent message: the content is a tensor, and the only checkable facts are who sent it,
who received it and how large it was. Auditing, filtering and per-message permission checks lose
their object. The loop-carried state is also closer to a kernel-managed buffer than to a message.
See [[agent-recursion-os-implications]].

## Related

- [[recursivelink]]
- [[latent-vs-text-agent-communication]]
- [[agent-collaboration-patterns]]
- [[recursion-depth-scaling]]
- [[aios-architecture]]
- [[agent-as-process]]
