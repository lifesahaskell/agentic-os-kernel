# Wiki Log

Append-only. Newest entry last.

## 2026-10-23 ingest: aios, ostep

- Sources: `raw/aios-rutgers-2026-03.pdf` (arXiv 2403.16971v2) and `raw/ostep.pdf` (829 pp.).
- Text: `pdftotext -layout`, split into page-marked chunks: OSTEP pp. 1-180, 181-310, 311-449,
  450-637, 638-829; AIOS whole.
- Extraction: one Sonnet subagent per chunk, writing candidate concepts to scratch. Synthesis,
  merging and the bridge page were done by Opus.
- Result: 90 concept pages (13 AIOS, 76 OSTEP, 1 synthesis) and 2 source pages. `address-translation`
  was merged from two extractors. Two dangling links were remapped: `virtual-memory` to `address-space`,
  `file-system` to `file-system-implementation`.
- Citation check: 6 random citations were verified against `raw/text/`. One more (ostep p. 702) was
  overstated and has been corrected in `setuid-and-privilege-escalation` and `aios-ostep-mapping`.
  Extractors reported that some concurrency and persistence cites are page ranges that may be off by one.
- Known gaps: OSTEP front matter and lab appendices are not ingested. Later AIOS versions are
  not ingested. Topics outside both sources are listed on `aios-ostep-mapping`.
