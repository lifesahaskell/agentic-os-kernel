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

## 2026-10-25 ingest: recursivemas, aide2

- Sources: `raw/RecursiveAgents.pdf` (*Recursive Multi-Agent Systems*, arXiv 2604.25917 **v1**, 36 pp.)
  and `raw/RecursiveSelfImprovement.pdf` (*Recursive self-improvement of AI research agents*,
  arXiv 2609.26457 **v1**, 28 pp.). Both are first arXiv versions; later versions are not ingested.
- Text: `pdftotext -layout` to `raw/text/recursivemas.txt` and `raw/text/aide2.txt`, plus
  page-marked copies (`*-paged.txt`; 36 and 28 markers, matching `pdfinfo`). No chunking.
- Extraction: one Sonnet subagent per paper, candidates to scratch. Merge and synthesis by Opus.
- Result: 2 source pages, 11 new concept pages (5 from aide2, 6 from recursivemas, consolidated from
  8 candidates each), 1 synthesis page (`agent-recursion-os-implications`). That is more than the
  6-10 planned. Merged into `context-window-management` (aide2 bounded prompts and failure memory).
  Bridge links added from `aios-ostep-mapping`. `AGENTS.md` source notes extended.
- Known inconsistencies in the recursivemas paper, recorded on `recursivemas` and the concept pages
  and not smoothed over: "%" is relative for the 8.3% and absolute points for Mixture's 6.2%; the 1.9x
  and 2.4x speedups do not reproduce from Table 2 (about 1.8x and 2.3x); scaled 3-round values differ
  between Table 2 and Tables 3 and 4; LoopLM is a 2.6B model despite "identical backbones"; no
  latent-communication baseline; training targets rewritten by a 397B model.
- aide2 has no sandbox, threat-model or safety section; the OS content is interpretation.
- Citation check: 8 page-level checks against `raw/text/*-paged.txt` (recursivemas pp. 9, 12, 13, 26;
  aide2 pp. 3, 5, 9, 27) plus earlier spot-checks of the headline figures. One cite was wrong
  (a quote placed on aide2 p. 3 is on p. 4, in `agent-recursion-os-implications`) and is fixed. Checks
  confirmed that the figures and phrases appear on the cited page; they did not re-derive every claim.
- Known gaps: appendices are covered selectively. recursivemas speedup figures are the paper's claim,
  not reproduced. Later arXiv versions of both papers are not ingested.

## 2026-10-25 query: spike candidates

- Filed `docs/architecture/research/recursion-and-cheap-inference-spikes.md` (outside the wiki; web
  sources cannot carry PDF-page cites). No new wiki pages. `index.md` gained a routing block so a query
  reads one named page instead of scanning the groups; the synthesis page links the research doc.

## 2026-10-25 restructure: split the index

- `wiki/index.md` (20.5 KB) is now a router (2.3 KB: where-to-start block, sources, synthesis, group
  links). The seven group sections moved verbatim to `wiki/indexes/index-<group>.md` (1.2-5.5 KB each).
  No concept or source page moved or was renamed; links are slug-only, so no wikilink changed.
- Query cost on the index: 20.5 KB before; 2.3 KB plus one group (3.5-7.8 KB) now. Bytes, not tokens.
- Check: 100 concept entries before and after; lint exits 0 with 115 pages (108 + 7 group indexes), 629
  links, no orphans. `AGENTS.md` layout, ingest, query and lint lines updated to match.
- Not done: entry descriptions are still the full definition sentence (average 182 characters).
