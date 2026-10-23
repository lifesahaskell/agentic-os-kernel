---
title: Distributed system security (authentication, certificates, TLS)
type: concept
sources: [ostep]
tags: [security]
---

# Distributed system security (authentication, certificates, TLS)

Across machines the OS controls only its own host, so security rests on authenticating remote parties and protecting messages on a network the adversary can read, forge, replay, and delay.

## Mechanisms

- Two problems: remote machines may not enforce policy, and the network is hostile (ostep p. 762); authentication only proves who, not that they behave (ostep pp. 763-764)
- Passwords suit many users to one server; public keys suit one server to many users (ostep p. 764)
- Certificates: a CA signs a hash of name plus public key; clients bootstrap from preinstalled CA keys (ostep pp. 765-766); X.509 (ostep p. 767)
- Replay attacks need unique per-message information (ostep p. 767)
- TLS: negotiate ciphers, authenticated Diffie-Hellman gives a fresh symmetric key per connection (ostep pp. 769-772); SSL is insecure, use TLS (ostep p. 769)
- Diffie-Hellman gives a shared key but not identity, enabling man-in-the-middle without authentication (ostep p. 771)
- Cookies, challenge/response, and authentication servers such as Kerberos (ostep pp. 772-774)
- HTTPS/HSTS, and SSH trust-on-first-use man-in-the-middle risk (ostep pp. 775-777)

## Trade-offs

- Reusing a vetted protocol such as TLS is cheap and safe; the residual risk is trusting the authenticated peer, CAs, and users who click through warnings.

## Agentic OS relevance

- (interpretation) Remote tool servers need mutual authentication, replay protection, and a pinned trust root; an authenticated server can still return malicious content.

## Related

- [[authentication]]
- [[cryptography]]
- [[rpc]]
- [[capabilities]]
