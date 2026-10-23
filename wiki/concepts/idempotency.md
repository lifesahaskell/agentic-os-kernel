---
title: Idempotent operations and retry safety
type: concept
sources: [ostep]
tags: [distribution]
---

# Idempotent operations and retry safety

An operation that gives the same result when applied repeatedly can be retried blindly after a lost reply, which removes the need for server-side memory of requests.

## Mechanisms

- Idempotent NFS operations (LOOKUP, READ, WRITE at explicit offset) make client retry safe (ostep p. 665)
- MKDIR is the counterexample: a retry can fail even though the first succeeded (ostep p. 666)
- Voltaire's Law: do not let the perfect (exactly-once) be the enemy of the good (ostep p. 667)
- Self-contained requests (file handle + offset + count) carry all needed state (ostep p. 660)

## Trade-offs

- Designing every op to be idempotent simplifies recovery but forbids some natural operations.

## Agentic OS relevance

- (interpretation) Agent tool calls that are re-issued after a timeout or context reset should be idempotent or carry a request id.

## Related

- [[nfs]]
- [[rpc]]
- [[distributed-systems-communication]]
