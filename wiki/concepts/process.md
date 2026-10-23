---
title: Process
type: concept
sources: [ostep]
tags: [virtualization]
---

# Process

A running program, defined by its machine state: address space, registers (PC, stack pointer), and open files/I/O information.

## Mechanisms

- Machine state = memory (code, data, stack, heap) + registers + persistent-storage info such as open file descriptors (ostep pp. 48-49)
- Creation: load code and static data from disk into the address space (eager in old systems, lazy via paging in modern ones), allocate stack and heap, set up fds, jump to main() (ostep pp. 50-51)
- States: running, ready, blocked; scheduler moves a process ready<->running, I/O moves it running->blocked->ready (ostep pp. 51-53)
- Process list holds one PCB per process; xv6 `struct proc` and saved `struct context` show the contents; zombie state keeps an exited child until the parent waits (ostep pp. 54-55)
- Context fields (pc, sp, registers) are what a switch saves and restores (ostep p. 54)

## Trade-offs

- A richer saved state makes switching and resuming easy but makes every switch and every process more expensive.

## Agentic OS relevance

- (interpretation) An agent's "machine state" is its context window, tool state, and open handles; a PCB analogue records them so an agent can be paused, blocked on a tool call, and resumed.

## Related

- [[process-api]]
- [[context-switch]]
- [[scheduling]]
- [[address-space]]
