---
title: Memory API (malloc, free, brk, mmap)
type: concept
sources: [ostep]
tags: [virtualization]
---

# Memory API (malloc, free, brk, mmap)

The user-level interface for dynamic memory: stack memory is automatic, heap memory is allocated and freed explicitly via library calls built on a few system calls.

## Mechanisms

- Stack memory is managed by the compiler; heap memory by malloc/free (ostep pp. 156-157)
- malloc returns a pointer or NULL; free takes a pointer and the library tracks size itself (ostep pp. 157-159)
- Error classes: unallocated pointer, buffer overflow, uninitialised read, leak, dangling pointer, double free, invalid free (ostep pp. 159-162)
- Two levels of management: the process heap and the OS, which reclaims all pages at exit (ostep p. 162)
- malloc is a library call; brk/sbrk move the heap end, mmap makes anonymous regions; calloc and realloc are conveniences (ostep p. 163)
- Compiling or running once does not imply correct (ostep p. 160); valgrind and purify find errors (ostep p. 162)

## Trade-offs

- Explicit lifetime control gives performance and flexibility but a large class of memory bugs.

## Agentic OS relevance

- (interpretation) Explicit context management (what to keep, summarise, or drop) mirrors malloc/free; a leak is context that is never evicted, a dangling pointer is a stale reference to content that was compacted away.

## Related

- [[address-space]]
- [[address-translation]]
- [[system-call]]
