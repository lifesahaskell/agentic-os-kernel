---
quick_id: 261025-jbm
mode: quick
---

# Ingest RecursiveMAS and AIDE2 into the wiki

## Context

Two PDFs landed in `raw/` on 2026-10-25 and are not in the wiki (only `aios`, `ostep` exist):

| File | Paper | Pages | Proposed slug |
|---|---|---|---|
| `raw/RecursiveAgents.pdf` | *Recursive Multi-Agent Systems*, Yang, Zou et al. (UIUC/Stanford/NVIDIA/MIT), arXiv 2604.25917v1, 28 Apr 2026 | 36 | `recursivemas` |
| `raw/RecursiveSelfImprovement.pdf` | *Recursive self-improvement of AI research agents*, Srikanth et al. (Weco AI), arXiv 2609.26457v1, 22 Sep 2026 (AIDE2) | 28 | `aide2` |

Goal: both sources get a source page, their concepts are merged into `wiki/concepts/`, `index.md`
and `log.md` are updated, and `tools/wikigraph.py` still exits 0 (baseline today: 94 pages,
491 links, exit 0).

These are ML-agent papers, not OS papers. Their value to this project is in the
`## Agentic OS relevance` sections (marked *(interpretation)*), not in the sources themselves.

## Process

Run under `/gsd-quick` (project CLAUDE.md requires a GSD entry point for repo edits).

### 1. Text extraction (raw/, gitignored)

- `pdftotext -layout raw/RecursiveAgents.pdf raw/text/recursivemas.txt`, same for
  `raw/text/aide2.txt`.
- Convert to page-marked form matching `raw/text/aios-paged.txt`: split on form feed (`\f`),
  prefix each page with `=== PDF page N ===`. One `python3 -I -c` one-liner writing
  `raw/text/recursivemas-paged.txt` and `raw/text/aide2-paged.txt`.
- Check: marker count equals `pdfinfo` page count (36 and 28).
- No chunking: both are well under the 100-200 page chunk size.

### 2. Extraction (2 Sonnet subagents, parallel)

One `general-purpose` subagent per paper, `model: sonnet`, single shell session. Each reads its
paged text plus `wiki/index.md` and writes candidates to the scratchpad (not the wiki):
`.../scratchpad/candidates-recursivemas.md`, `.../candidates-aide2.md`. For each candidate:
proposed slug, one-line definition, mechanism bullets with PDF page cites, and whether an
existing wiki page already covers it (merge target).

Expected candidates (extractors confirm or replace):
- recursivemas: recursive multi-agent system, RecursiveLink / latent-state transfer between
  agents, the 4 collaboration patterns, inner-outer loop training, recursion-depth scaling, and
  the efficiency claims (8.3% accuracy, 1.2-2.4x speedup, 34.6-75.6% token reduction).
- aide2: recursive self-improvement loop, self-evaluation on hidden held-out tasks, reward
  hacking (55% to 32%), discovered context-compression memory mechanisms, search policy.

### 3. Merge (Opus, me)

- Update existing pages before creating new ones. Likely merge targets:
  `context-window-management` and `memory-manager` (AIDE2 context compression),
  possibly `agent-scheduler`. Add the new slug to each touched page's `sources:` frontmatter.
- New concept pages, roughly 6-10 total, frontmatter `tags: [agentic]`, body per AGENTS.md
  conventions. Every factual claim cites `(recursivemas p. N)` / `(aide2 p. N)`.
- Source pages `wiki/sources/recursivemas.md` and `wiki/sources/aide2.md`, modeled on
  `wiki/sources/aios.md` (Claim, Architecture/Method, Evidence, Assessment *(interpretation)*).
- One synthesis page, `agent-recursion-os-implications`, all *(interpretation)*, linking to
  existing OS pages. Points to develop:
  - Latent-state transfer between agents carries opaque tensors, which a kernel that stamps
    identity on messages and mediates by capability cannot inspect ([[capabilities]],
    [[access-control]]).
  - A self-modifying agent needs write authority over its own image plus versioned rollback:
    an exec/capability question ([[principle-of-least-privilege]]).
  - Reward hacking means the evaluator must sit outside the agent's authority, which is the
    [[trusted-computing-base]] argument already on `aios-ostep-mapping`.
  - Multi-day autonomous runs need per-principal resource accounting.
- Link the synthesis page from `aios-ostep-mapping` `## Related`.
- `wiki/index.md`: add both sources under `## Sources`, a new group
  `## Agent systems research (N)` for the new concept pages, and the synthesis page under
  `## Synthesis`, with updated counts.
- Append a `## 2026-10-25 ingest: recursivemas, aide2` entry to `wiki/log.md` (files, text
  steps, extractor model, pages created/merged, citation-check result, known gaps).
- Add both to `## Source notes` in `AGENTS.md` (arXiv id and version; filename does not carry
  the slug).

### 4. Lint and verify

- `python3 -I tools/wikigraph.py wiki` exits 0; regenerated `wiki/graph.json`; no new orphans
  absent from `index.md`.
- Citation sample: verify 6 random cites (3 per source) against the paged text; fix any that
  are wrong and record the result in the log entry.
- Check the headline numbers above against the abstract pages directly, since they will be
  repeated on several pages.

## Not done

- No commit unless asked (GSD quick may propose one; I will ask first).
- Appendices beyond what extractors flag as relevant are summarized on the source page, not
  given concept pages.
