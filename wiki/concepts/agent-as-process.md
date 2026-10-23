---
title: Agent as Process Analogy
type: concept
sources: [aios]
tags: [agentic]
---

# Agent as Process Analogy

Agents are treated like applications/processes: they have IDs, status, priority, can be suspended/resumed, and get allocated memory and storage.

## Mechanisms

- Agent syscalls get/set aid, status, priority; suspend_agent, resume_agent (aios p. 7).
- SDK registers, queues, terminates, lists agents (aios p. 8).
- Agent memory is private unless the access manager authorizes (aios p. 6).
- Paper frames agents as apps in an "OS-application ecosystem" (aios p. 3).
- Scheduler interleaves steps of different agents like time-shared tasks (aios p. 5).

## Evaluation

- Table 5 treats per-agent request wait/turnaround like process metrics (aios p. 9).

## OS analogue

- Process/PCB and process lifecycle. Breaks: an agent's unit of scheduling is one LLM request (step), not CPU time; the "CPU" is the LLM (interpretation). The paper never uses the word "process" for agents; the mapping is implied by the syscalls (interpretation).

## Related

- [[agent-scheduler]]
- [[llm-system-calls]]
- [[aios-sdk]]
- [[context-switch]]
- [[scheduling]]
