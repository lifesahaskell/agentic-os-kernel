---
title: Cryptography
type: concept
sources: [ostep]
tags: [security]
---

# Cryptography

Transforms data with a key so that confidentiality, integrity, and authenticity can be provided over untrusted channels and storage; all benefit rests on key secrecy.

## Mechanisms

- Plaintext, key, ciphertext; P = D(C, K) (ostep p. 744); benefit depends entirely on key secrecy (ostep p. 745)
- Use a standard cipher such as AES, never your own (ostep pp. 745-746)
- Public key: sign with private key, encrypt with public key, costs far more, key distribution is hard (ostep pp. 746-749)
- Cryptographic hashes; encrypt the hash for integrity; Tripwire; SHA-1 broken in 2017 (ostep pp. 749-750)
- Failures come from key flaws: Heartbleed, WEP, weak seeds, shared embedded keys (ostep pp. 751-753); use real entropy such as /dev/random (ostep p. 752)
- At rest: full disk encryption guards a stolen device, not a running one (ostep pp. 755-757); archives, cloud, app-level (ostep pp. 757-758)
- Hold keys in as few places, for as short a time, and by as few parties as possible (ostep p. 760)

## Trade-offs

- Strong algorithms are cheap; key management is where systems break.

## Agentic OS relevance

- (interpretation) Signing agent actions and tool manifests gives integrity and provenance; secrets in context windows are the weak key store.

## Related

- [[password-storage]]
- [[capabilities]]
- [[distributed-security]]
