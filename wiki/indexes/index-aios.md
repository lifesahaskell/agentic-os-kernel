# Index: AIOS (agentic)

AIOS modules and agent-as-process ideas. Back to [[index]].

- [[access-manager|Access Manager]]: Enforces access control between agents using a privilege group per agent, with audit logging.
- [[agent-as-process|Agent as Process Analogy]]: Agents are treated like applications/processes: they have IDs, status, priority, can be suspended/resumed, and get allocated memory and storage.
- [[agent-scheduler|Agent Scheduler]]: Kernel module that queues and orders agent requests to the LLM so steps from different agents interleave instead of running agent-by-agent.
- [[aios-architecture|AIOS Architecture and Layers]]: A three-layer design (application, kernel, hardware) in which an LLM-specific kernel sits beside the ordinary OS kernel and exposes LLM services to agents.
- [[aios-sdk|AIOS SDK]]: Higher-level library that wraps LLM syscalls for agent developers.
- [[context-manager|Context Manager]]: Kernel module responsible for the context given to the LLM and for the generation process under that context.
- [[context-snapshot-restore|Context Snapshot and Restore]]: Mechanism to save the state of an LLM's partially complete generation when the scheduler suspends a request, and reload it to resume exactly where it stopped.
- [[context-window-management|Context Window Management]]: Handling contexts longer than the LLM's window.
- [[evaluation-aios|AIOS Evaluation (Consistency and Scheduling)]]: Two experiments on whether suspend/resume preserves outputs and whether scheduling improves wait/turnaround balance.
- [[llm-system-calls|LLM System Calls]]: Interface in the LLM kernel exposing basic LLM-call functions across kernel modules, bridging agent requests and module execution.
- [[memory-manager|Memory Manager]]: Holds each agent's short-term interaction history, available only while the agent is active.
- [[storage-manager|Storage Manager]]: Persists agent data (interaction logs, user preferences) beyond any agent's lifetime.
- [[tool-manager|Tool Manager]]: Manages the external API tools agents may call, spanning text and image modalities.
