# Index: Security

Protection, authentication and attacks. Back to [[index]].

- [[access-control-lists|Access control lists]]: Each object stores a list of which principals may do what, checked on each access.
- [[access-control|Access control and the reference monitor]]: Decides whether a subject may perform an access on an object by consulting a policy at a single mediating point.
- [[authentication|Authentication]]: Establishing which principal is acting, from something it knows, has, or is, and binding that identity to a process for its lifetime.
- [[capabilities|Capabilities]]: An unforgeable token that both names an object and grants specific rights to it; possession is the authorization.
- [[cryptography|Cryptography]]: Transforms data with a key so that confidentiality, integrity, and authenticity can be provided over untrusted channels and storage; all benefit rests on key secrecy.
- [[distributed-security|Distributed system security (authentication, certificates, TLS)]]: Across machines the OS controls only its own host, so security rests on authenticating remote parties and protecting messages on a network the adversary can read, forge, replay, and delay.
- [[password-storage|Password storage and guessing resistance]]: Servers keep a salted one-way hash so a stolen file does not reveal passwords, and slow or limit guessing.
- [[principle-of-least-privilege|Security design principles (Saltzer-Schroeder) and least privilege]]: A checklist of design rules for protection mechanisms, of which least privilege (grant only the rights needed, only for as long as needed) is the most operationally important.
- [[rbac-and-mac|RBAC, mandatory access control, and type enforcement]]: Policy attached to roles or labels set by an administrator, not by object owners, so users cannot loosen it.
- [[security-goals-and-threat-model|Security goals, policy, and threat model]]: Security is stated as goals (confidentiality, integrity, availability, plus authenticity and non-repudiation), turned into policies, and enforced by mechanisms; the OS is inside the trusted base.
- [[setuid-and-privilege-escalation|Setuid, sudo, and privilege escalation (nearest material to confused deputy)]]: A program can run with its owner's rights instead of the caller's, which is needed for controlled elevation and is a prime attack target.
- [[trusted-computing-base|Trusted computing base]]: The OS cannot hide data from itself: whatever must be trusted to enforce security is the base, and it must be small and correct.
