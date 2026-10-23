---
title: Virtual machine monitor (hypervisor)
type: concept
sources: [ostep]
tags: [virtualization]
---

# Virtual machine monitor (hypervisor)

A layer beneath the OS that multiplexes the hardware among whole operating systems while each believes it controls the machine; transparency is the goal.

## Mechanisms

- IBM mainframe origin; VMM is an OS for OSes (ostep p. 781); reasons: consolidation, desktop compatibility, test/debug; Disco and VMware (ostep p. 782)
- CPU: limited direct execution; machine switch saves full state including privileged state (ostep p. 782)
- Guest OS runs deprivileged, so privileged instructions and traps go to the VMM, which records the OS trap handlers and forwards syscalls to them (ostep pp. 783-785)
- Guest OS runs in supervisor mode on MIPS, or in user mode with page-table protection otherwise (ostep pp. 785-786)
- Memory: extra level maps "physical" PFN to machine MFN; on TLB-miss the VMM installs VPN-to-MFN instead of the OS mapping (ostep pp. 786-789); software TLB cuts the cost (ostep p. 789)
- Hardware-managed TLB needs shadow page tables (ostep p. 789)
- Information gap (idle loop, double page zeroing); inference or para-virtualization closes it (ostep pp. 790-791)
- VMM exposes the raw hardware interface, not a nicer abstraction; hardware support later removed many tricks (ostep pp. 791-792)

## Trade-offs

- Strong isolation and transparency cost extra indirection (slower syscalls, TLB misses) and an information gap.

## Agentic OS relevance

- (interpretation) A VM or microVM is the strongest sandbox boundary for untrusted agent code, since isolation does not depend on the guest.

## Related

- [[trusted-computing-base]]
- [[access-control]]
- [[principle-of-least-privilege]]
