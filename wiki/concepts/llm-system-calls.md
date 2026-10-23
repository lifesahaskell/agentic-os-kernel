---
title: LLM System Calls
type: concept
sources: [aios]
tags: [agentic]
---

# LLM System Calls

Interface in the LLM kernel exposing basic LLM-call functions across kernel modules, bridging agent requests and module execution.

## Mechanisms

- Analogous to OS syscalls; spans agent management, context, memory, storage, access control (aios p. 8).
- Table 2 categories and calls: Agent (get_aid, set_aid, get_status, set_status, get_priority, set_priority, suspend_agent, resume_agent); Context (gen_snapshot, gen_restore, get_context, exp_context, clr_context, set_context); Memory (mem_write, mem_read, mem_clear, mem_alloc); Storage (sto_write, sto_read, sto_delete, sto_alloc) (aios p. 7).
- Most take an agent id; set_priority also takes an int priority (aios p. 7).
- List is expected to expand (aios p. 8).
- Calls go to OS syscalls for hardware (aios p. 4).

## OS analogue

- System call interface. Breaks: no trap or privilege-mode transition described; set_status doc text says "get" (typo in paper, aios p. 7).

## Related

- [[system-call]]
- [[aios-sdk]]
- [[aios-architecture]]
