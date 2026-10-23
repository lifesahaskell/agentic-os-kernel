---
title: Traps, Interrupts and User/Kernel Mode
type: concept
sources: [ostep]
tags: [virtualization]
---

# Traps, Interrupts and User/Kernel Mode

Hardware mechanisms that transfer control to the OS: a trap on system call or exception, an interrupt on a device or timer event, with a privilege bit separating user mode from kernel mode.

## Mechanisms

- Two modes: user mode is restricted, kernel mode is unrestricted (ostep p. 77)
- Trap table is set up at boot by the OS using privileged instructions; the hardware remembers handler addresses (ostep pp. 78, 177)
- On trap, hardware saves registers to a per-process kernel stack, then jumps to the handler (ostep pp. 78-79)
- Timer interrupt fires every few ms, giving the OS control back from a running process (ostep pp. 81-83)
- Disabling interrupts during handling guards against concurrent interrupts (ostep pp. 85-86)
- Illegal instruction or out-of-bounds access also traps; usual response is to kill the process (ostep pp. 175-176)

## Trade-offs

- The OS gets guaranteed control points, paid for with trap overhead and the need to make handlers reentrant-safe.

## Agentic OS relevance

- (interpretation) A budget-exhaustion or wall-clock signal is the agent's timer interrupt; without one, a looping agent never yields the inference slot.

## Related

- [[system-call]]
- [[limited-direct-execution]]
- [[context-switch]]
