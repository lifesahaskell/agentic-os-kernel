---
title: AIOS SDK
type: concept
sources: [aios]
tags: [agentic]
---

# AIOS SDK

Higher-level library that wraps LLM syscalls for agent developers.

## Mechanisms

- Covers agent initialization, lifecycle management, resource monitoring, plan generation (aios p. 8).
- Table 3 functions: initializeAgent, registerAgent (ID and permissions), queueForExecution, generatePlan, terminateAgent, createAgentSnapshot, rollbackAgentState, updateAgentConfig, authenticateAgent, monitorResourceUsage, logEvent, listActiveAgents, queryAgentHistory, encryptData, decryptData (aios p. 8).
- Lets developers focus on agent logic (aios p. 4).
- Described as a long-term, continually expanded effort (aios p. 8).
- The SDK has some functions (createAgentSnapshot, rollbackAgentState) with no matching Table 2 syscall name (interpretation).

## OS analogue

- libc / platform SDK (Android Studio, Xcode cited, aios p. 3). Breaks: several functions (encryption, auth) are security features with no described implementation (interpretation).

## Related

- [[llm-system-calls]]
- [[agent-as-process]]
- [[system-call]]
- [[aios-architecture]]
