---
quick_id: 261025-kpm
status: complete
---

# Quick task 261025-kpm: spike candidates, recursion and cheap-GPU inference

Wrote `docs/architecture/research/recursion-and-cheap-inference-spikes.md`: source grades, what
recursivemas and aide2 imply for the OS, a 16 GB consumer-GPU hardware table, spikes S0-S7 (host)
and K1-K4 (kernel) with pass/fail criteria, a suggested order, and a verified/not-verified section.

- Wiki routing: `wiki/index.md` gained a "where to start, by question" block; `wiki/log.md` has a
  query entry; `agent-recursion-os-implications` links the doc.
- Nothing added to ROADMAP; the user picks spikes first.
- Not verified: AMD figures (secondary sources only; vendor page timed out twice), the vLLM
  PagedAttention paper (not fetched), any 16 GB result, and every threshold (all marked proposed).
- Follow-on token audit ran separately in worktree `.claude/worktrees/token-audit`
  (branch `audit/token-surface`); it is not part of this task.
- Not committed.
