---
title: Process API (fork, exec, wait, signals)
type: concept
sources: [ostep]
tags: [virtualization]
---

# Process API (fork, exec, wait, signals)

The UNIX interface for creating and controlling processes: fork clones the caller, exec replaces its image, wait blocks until a child exits.

## Mechanisms

- fork() creates a near-identical child; returns 0 to the child and the child's PID to the parent (ostep pp. 60-62)
- wait() makes output deterministic by blocking the parent until the child exits (ostep p. 63)
- exec() loads a new program over the current image and never returns on success (ostep pp. 63-65)
- Split of fork and exec lets the shell run code between them, enabling redirection and pipes (ostep pp. 65-67)
- Signals deliver asynchronous notifications (kill, SIGINT); a process can install handlers (ostep pp. 67-68)
- Users and superuser: control over processes is scoped by user, root can override (ostep pp. 68-69); spawn-style single calls critiqued as less flexible (ostep p. 69)

## Trade-offs

- Fork+exec is flexible and composable but copies state that exec immediately discards.

## Agentic OS relevance

- (interpretation) Spawning a sub-agent is fork: the child inherits context. The gap between spawn and task assignment is where redirection-like setup (tools, permissions, output sinks) can be injected.

## Related

- [[process]]
- [[system-call]]
- [[trap-and-interrupt]]
