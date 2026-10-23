---
title: "Operating Systems: Three Easy Pieces"
type: source
sources: [ostep]
tags: [virtualization, concurrency, persistence, distribution, security]
---

# Operating Systems: Three Easy Pieces

Remzi H. Arpaci-Dusseau and Andrea C. Arpaci-Dusseau. Local file: `raw/ostep.pdf`, 829 PDF pages,
including Peter Reiher's security chapters and the virtual machine monitor appendix. All
citations in this wiki use **PDF page numbers**.

## Sections ingested (PDF pages)

| Piece | Pages | Concept pages, for example |
|---|---|---|
| Front matter / intro | 1-46 | [[virtualization-and-resource-management]], [[mechanism-vs-policy]] |
| Virtualization | 45-310 | [[process]], [[limited-direct-execution]], [[context-switch]], [[mlfq]], [[address-translation]], [[paging]], [[tlb]], [[swapping]], [[working-set-and-thrashing]] |
| Concurrency | 311-449 | [[thread]], [[lock]], [[condition-variable]], [[semaphore]], [[deadlock]], [[event-based-concurrency]] |
| Persistence | 450-637 | [[file-and-directory-api]], [[inode]], [[crash-consistency]], [[journaling]], [[fsync-and-durability]], [[atomic-rename]] |
| Distribution | 638-690 | [[rpc]], [[nfs]], [[afs]], [[idempotency]] |
| Security | 691-~780 | [[security-goals-and-threat-model]], [[access-control]], [[capabilities]], [[trusted-computing-base]], [[principle-of-least-privilege]] |
| Appendices | ~781-829 | [[virtual-machine-monitor]]; lab appendices were skimmed only |

The book's three pieces map onto three design questions for an agentic OS: what is virtualized
(inference, context), how concurrent agents coordinate (tools, files), and what state outlives
an agent (memory, storage). Security, which OSTEP added later, is the fourth question and the
hardest one for agents. See [[aios-ostep-mapping]].

## Ingest notes

Ingested as concept pages only, with no per-chapter pages. Pages 1-46 (front matter) and the
lab appendices are not covered. Several extractors read parts of their chunk selectively, so
page citations can be off by one. See `log.md`.
