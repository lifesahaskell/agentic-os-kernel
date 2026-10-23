---
title: Working Set, Thrashing, and Admission Control
type: concept
sources: [ostep]
tags: [virtualization]
---

# Working Set, Thrashing, and Admission Control

When the combined active pages of running processes exceed physical memory, the system pages constantly and makes little progress.

## Mechanisms

- Thrashing defined as constant paging under oversubscription (ostep p. 282).
- Working set is the pages a process actively uses; if all fit, progress resumes (ostep p. 282).
- Admission control: run fewer processes so their working sets fit; do less work well (ostep p. 282).
- Linux out-of-memory killer picks and kills a memory-heavy process, which can hit critical ones such as the X server (ostep p. 282).
- VMS caps each process with a resident set size to stop memory hogs (ostep p. 291-292).

## Trade-offs

- Throughput of the whole depends on refusing or killing some work; fairness and utilization pull in opposite directions.

## Agentic OS relevance

- (interpretation) Too many concurrent agents sharing one context or token budget will each re-load and re-summarize constantly; cap concurrency to total working-set size, and give each agent a resident budget.

## Related

- [[swapping]]
- [[page-replacement-policy]]
- [[segmented-fifo-and-vms]]
