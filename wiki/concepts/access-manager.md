---
title: Access Manager
type: concept
sources: [aios]
tags: [agentic]
---

# Access Manager

Enforces access control between agents using a privilege group per agent, with audit logging.

## Mechanisms

- Each agent has a dedicated privilege group; agents outside it are denied access to its resources such as interaction history (aios p. 7).
- Keeps auditing logs of access requests, agent activities, and changes to access-control parameters, to guard against privilege attacks (aios pp. 7-8).
- Authorizes cross-agent memory access (aios p. 6).
- Not in this paper version: user confirmation for irreversible operations. No such mechanism is described. Table 2 has no access syscalls even though the text says syscalls cover access control (aios p. 8) (interpretation: gap).
- SDK adds authenticateAgent, encryptData, decryptData (aios p. 8).
- Future: defenses against LLM jailbreak and unauthorized memory access; encryption; watermarking (aios p. 10).

## Evaluation

- none; access control is not evaluated.

## OS analogue

- Access control lists / permission groups plus audit log. Breaks: policy is only between agents, not user-to-agent; no enforcement test is reported (interpretation).

## Related

- [[access-control]]
- [[memory-manager]]
- [[storage-manager]]
- [[aios-sdk]]
