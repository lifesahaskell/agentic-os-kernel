---
quick_id: 261025-kpm
mode: quick
---

# Spike candidates: recursive self-improvement and cheap-GPU inference

Decisions (user, 2026-10-25): inference work covers host-side measurement and kernel-side design at equal
weight; containment is a sandbox plus an external grader (Spike 0); deliverable is a cited research doc
with ranked spike candidates and pass/fail criteria; nothing goes to ROADMAP until the user picks.

1. Research cheap-GPU inference (RTX 5070 Ti 16 GB, AMD RX 9070 XT class) from primary sources.
2. Write docs/architecture/research/recursion-and-cheap-inference-spikes.md citing recursivemas and aide2 by PDF page.
3. Update the wiki for context efficiency (index entry, log, link from the synthesis page).
4. Lint: python3 -I tools/wikigraph.py wiki exits 0.

Out of scope: token audit (separate task, own worktree, asks before moving context).
