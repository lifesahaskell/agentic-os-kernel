---
title: Event-Based Concurrency
type: concept
sources: [ostep]
tags: [concurrency]
---

# Event-Based Concurrency

Handling many concurrent activities in a single-threaded event loop that waits for events and runs short handlers, giving explicit control over scheduling without locks.

## Mechanisms

- Loop: events = getEvents(); for each e, processEvent(e); choosing the next event is scheduling (ostep p. 434-435)
- select()/poll() report which descriptors are ready to read or write, enabling non-blocking servers (ostep p. 435-437)
- No locks needed on a single CPU, since one handler runs at a time (ostep p. 438)
- Rule: no blocking calls in a handler, or the whole server stalls; use asynchronous I/O (aiocb, aio_read, aio_error) (ostep p. 438-439)
- Completion via polling or UNIX signals; hybrids use a thread pool for I/O (ostep p. 440-441)
- State management: no stack to hold context, so "manual stack management" or continuations; harder on multiple CPUs and with implicit blocking like page faults (ostep p. 440-442)

## Trade-offs

- Simplicity and scheduling control versus no blocking allowed and manual state passing.

## Agentic OS relevance

- (interpretation) An orchestrator running many LLM calls as an async event loop avoids locks but must never block on one slow call, and must carry per-request state explicitly across callbacks.

## Related

- [[thread]]
- [[condition-variable]]
- [[race-condition]]
