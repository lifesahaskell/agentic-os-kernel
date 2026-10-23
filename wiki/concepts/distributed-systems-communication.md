---
title: Distributed systems communication
type: concept
sources: [ostep]
tags: [distribution]
---

# Distributed systems communication

Machines cooperate only by exchanging messages over a network that can lose, corrupt, or reorder packets. Reliability is layered on top of an unreliable substrate.

## Mechanisms

- Packet loss is fundamental, so the design must tolerate it (ostep p. 641)
- UDP is unreliable but carries a checksum to detect corruption (ostep p. 644)
- Acknowledgment plus timeout/retry recovers lost messages (ostep p. 645)
- Sequence counters let the receiver drop duplicate retries, giving exactly-once delivery (ostep pp. 646-647)
- Exponential back-off tunes the retry timeout; TCP packages these techniques (ostep p. 647)
- DSM failed to hide the network because of failure handling and performance (ostep p. 648); end-to-end argument aside (ostep p. 652)

## Trade-offs

- Hiding the network behind familiar abstractions (shared memory) is attractive but fails once partial failure and latency leak through.

## Agentic OS relevance

- (interpretation) Remote tool calls fail, time out, and get retried; an agent runtime needs the same ack/timeout/dedupe discipline.

## Related

- [[rpc]]
- [[nfs]]
- [[idempotency]]
- [[distributed-security]]
