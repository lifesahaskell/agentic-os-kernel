---
title: I/O Devices and Device Drivers
type: concept
sources: [ostep]
tags: [persistence]
---

# I/O Devices and Device Drivers

The OS talks to devices through a status/command/data register interface, hidden behind drivers so the rest of the kernel stays device-neutral. The protocol cost drives the design of interrupts and DMA.

## Mechanisms

- Polling protocol: spin on the status register, write data, write command, poll again for completion; wastes CPU while the device works (ostep p. 455)
- Interrupts let the OS sleep the caller and run another process; for fast devices polling can win, and a hybrid or interrupt coalescing limits interrupt overhead (ostep p. 456, 465)
- Livelock: an interrupt flood can leave the OS only servicing interrupts, so polling is sometimes used to regain control (ostep p. 457)
- DMA engine copies data between memory and device so the CPU is not burdened with programmed I/O (ostep p. 458)
- Device access is either explicit I/O instructions (port I/O) or memory-mapped I/O (ostep p. 459, 464)
- Device driver wraps the device protocol; the file system issues generic block requests to a generic block layer, which routes to the driver, while a raw interface bypasses the file system (ostep p. 459-460)

## Trade-offs

- Polling is cheap for fast devices and wasteful for slow ones; interrupts the reverse, so the right choice depends on device latency.

## Agentic OS relevance

- (interpretation) An agent's file write is the top of this stack; latency and failure behavior at the device layer are invisible to the agent unless it asks for durability explicitly.

## Related

- [[hard-disk]]
- [[flash-ssd]]
- [[file-system-implementation]]
- [[fsync-and-durability]]
