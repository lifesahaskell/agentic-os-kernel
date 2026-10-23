---
title: Copy-on-Write
type: concept
sources: [ostep]
tags: [virtualization]
---

# Copy-on-Write

Share a page between address spaces read-only and copy it only when one side writes.

## Mechanisms

- On copy request, map the same frame into the target and mark it read-only in both (ostep p. 293).
- A write traps to the OS, which allocates a new page, fills it, and maps it privately (ostep p. 293).
- Shared libraries are mapped copy-on-write across processes (ostep p. 293).
- fork() followed by exec() would otherwise copy a whole address space that is immediately discarded (ostep p. 293-294).
- Same family as demand zeroing; "be lazy" principle (ostep p. 293-294).

## Trade-offs

- Fast logical copies with a one-time fault and copy cost paid only on divergence.

## Agentic OS relevance

- (interpretation) Forking sub-agents from a parent context can share the parent's prefix (and its prompt cache) and diverge only on new tokens, instead of duplicating the full context.

## Related

- [[demand-paging]]
- [[paging]]
- [[segmented-fifo-and-vms]]
