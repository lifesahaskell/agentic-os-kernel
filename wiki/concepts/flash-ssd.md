---
title: Flash-based SSD and the FTL
type: concept
sources: [ostep]
tags: [persistence]
---

# Flash-based SSD and the FTL

NAND flash storage with no moving parts but asymmetric operations; a flash translation layer presents a normal block interface while hiding erase-before-program and wear-out.

## Mechanisms

- Operations: read a page (tens of us), erase a block (ms), program a page (hundreds of us); a programmed page cannot be reprogrammed until its block is erased (ostep p. 600-601)
- SLC/MLC/TLC trade performance and cost for bits per cell; wear-out measured in P/E cycles, plus read and program disturbance (ostep p. 598, 602)
- Direct-mapped FTL is a bad idea: read-modify-erase-program per write, severe write amplification, concentrated wear (ostep p. 604)
- Log-structured FTL appends to the next free page and keeps a logical-to-physical mapping table, persisted via out-of-band areas (ostep p. 605-607)
- Garbage collection reads live pages, rewrites them to the log, erases the block; overprovisioning delays it (ostep p. 607-609)
- TRIM tells the device which blocks are dead so GC can skip them (ostep p. 609)
- Mapping size: 1TB with 4KB pages needs ~1GB of table; block-level mapping shrinks it but hurts small writes; hybrid log/data tables use switch, partial, and full merges (ostep p. 609-613)
- Wear leveling migrates cold data so blocks age evenly; SSDs win mostly on random I/O, much less on sequential (ostep p. 614-615)

## Trade-offs

- Fine-grained mapping gives good write performance but needs memory; coarse mapping saves memory but amplifies small writes.

## Agentic OS relevance

- (interpretation) Frequent small overwrites by an agent increase write amplification and wear; append-only, batched writes are friendlier to flash.

## Related

- [[log-structured-file-system]]
- [[hard-disk]]
- [[data-integrity]]
- [[io-devices-and-drivers]]
