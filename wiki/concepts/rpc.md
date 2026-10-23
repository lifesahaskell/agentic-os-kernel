---
title: Remote procedure call
type: concept
sources: [ostep]
tags: [distribution]
---

# Remote procedure call

Makes a remote function call look like a local one by generating stubs that marshal arguments and a run-time library that moves the bytes.

## Mechanisms

- Stub generator emits client and server stubs that marshal/unmarshal arguments and results (ostep pp. 649-650)
- Server uses a thread pool to serve concurrent requests (ostep p. 650)
- Run-time library handles naming (host/port), transport choice (UDP vs TCP), and at-most-once semantics (ostep p. 651)
- Long-running calls need explicit acks and polling (ostep pp. 651-652)
- Large arguments need fragmentation (ostep p. 652)
- Byte order is normalized (XDR); asynchronous RPC lets the client overlap work (ostep p. 653)

## Trade-offs

- Transparency of the call syntax versus honest exposure of failure, latency, and argument size.

## Agentic OS relevance

- (interpretation) Model tool-calling and MCP are RPC; schema = stub generator, retries and timeouts are the unsolved part.

## Related

- [[distributed-systems-communication]]
- [[idempotency]]
- [[nfs]]
