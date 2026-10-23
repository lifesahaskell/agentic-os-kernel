---
title: Password storage and guessing resistance
type: concept
sources: [ostep]
tags: [security]
---

# Password storage and guessing resistance

Servers keep a salted one-way hash so a stolen file does not reveal passwords, and slow or limit guessing.

## Mechanisms

- Store a hash, not the password; "avoid storing secrets" tip (ostep p. 710)
- Use a cryptographic hash such as SHA-3 (ostep p. 711)
- Dictionary attacks defeated by rate limits and strong choices (ostep pp. 711-712)
- Salt defeats precomputed tables and shared-hash detection (ostep pp. 712-713)

## Trade-offs

- Hash cost and salting resist offline cracking but do nothing against weak passwords or phishing.

## Agentic OS relevance

- (interpretation) API keys in agent configs are stored secrets; hold them in a vault, never in prompts or logs.

## Related

- [[authentication]]
- [[cryptography]]
