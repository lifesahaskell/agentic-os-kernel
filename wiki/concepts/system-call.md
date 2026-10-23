---
title: System Call
type: concept
sources: [ostep]
tags: [virtualization]
---

# System Call

A controlled entry from user code into the kernel to perform a privileged operation; it looks like a procedure call but is implemented as a trap.

## Mechanisms

- Library stub places syscall number and arguments in agreed registers/stack, then executes the trap instruction (ostep pp. 77, 80)
- Hardware raises privilege to kernel mode and jumps to a handler registered in the trap table (ostep pp. 77-78)
- Kernel dispatches on the syscall number; validating arguments (user pointers) is mandatory (ostep p. 80)
- return-from-trap restores user registers and drops to user mode (ostep pp. 77-78)
- Caller cannot name a kernel address; it can only name a number, which limits what it can reach (ostep p. 80)

## Trade-offs

- A narrow numbered interface gives safety and auditability at the cost of a mode switch per request.

## Agentic OS relevance

- (interpretation) Tool calls are the agent's syscalls: a fixed, numbered, argument-validated surface between untrusted model output and privileged side effects.

## Related

- [[trap-and-interrupt]]
- [[limited-direct-execution]]
- [[process-api]]
