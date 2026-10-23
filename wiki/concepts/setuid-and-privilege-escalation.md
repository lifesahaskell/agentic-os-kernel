---
title: Setuid, sudo, and privilege escalation (nearest material to confused deputy)
type: concept
sources: [ostep]
tags: [security]
---

# Setuid, sudo, and privilege escalation (nearest material to confused deputy)

A program can run with its owner's rights instead of the caller's, which is needed for controlled elevation and is a prime attack target. The text never names "confused deputy"; this slug is an interpretation.

## Mechanisms

- Setuid and sudo let ordinary users run specific privileged programs (ostep pp. 738-740)
- Attackers target root-owned programs (ostep p. 739)
- Attackers may try to fool legitimate users into misusing the system; fooling a fully privileged user does more damage than fooling one who can only harm their own assets (ostep p. 702)
- Remedy is least privilege and narrow, audited elevation points (ostep pp. 737-740)

## Trade-offs

- Elevation points make administration possible but concentrate risk in code that trusts its caller.

## Agentic OS relevance

- (interpretation) A tool-using agent holds the user's authority and can be steered by prompt injection into misusing it, the confused deputy; same defence: least privilege plus mediation of tool calls.

## Related

- [[principle-of-least-privilege]]
- [[capabilities]]
- [[trusted-computing-base]]
