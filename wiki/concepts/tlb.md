---
title: Translation Lookaside Buffer
type: concept
sources: [ostep]
tags: [virtualization]
---

# Translation Lookaside Buffer

A small hardware cache of recent VPN-to-PFN translations that makes paging fast by exploiting locality.

## Mechanisms

- Check TLB first; hit avoids the page-table walk, miss walks the table and installs the entry (ostep p. 227-228).
- Spatial and temporal locality drive hit rates, for example the array-walk example at roughly 70% (ostep p. 229-231).
- Hardware-managed (x86) vs software-managed (MIPS/SPARC) misses; software handlers need retry-instruction semantics and wired entries (ostep p. 232-233).
- Context switches: flush the TLB or tag entries with an ASID (ostep p. 235-236).
- Replacement: LRU or random; LRU is pathological on a loop over n+1 pages (ostep p. 237).
- TLB coverage limits (Culler's Law: the TLB is the scarce resource); large pages extend coverage (ostep p. 238-239).
- Measured costs of about 5ns hit versus 20ns and 70ns miss paths, a gap of roughly 14x (ostep p. 241-242).

## Trade-offs

- Tiny and fully associative gives speed, but limited coverage means working sets beyond it pay full walk cost.

## Agentic OS relevance

- (interpretation) A small cache of hot "translations" (resolved tool results, retrieved doc handles, prompt-cache prefixes) in front of an expensive lookup; flush or tag per agent on switch.

## Related

- [[paging]]
- [[address-translation]]
- [[page-replacement-policy]]
