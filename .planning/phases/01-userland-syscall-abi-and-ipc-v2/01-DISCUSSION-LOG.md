# Phase 1: Userland, syscall ABI and IPC v2 - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-10-24
**Phase:** 1-Userland, syscall ABI and IPC v2
**Areas discussed:** FPU gate rules

---

## Area selection

Offered: FPU gate rules, IPC cost target, User code layout, Syscall ABI shape. Selected: FPU gate rules only.

## FPU gate rules

| Question | Options | Selected |
|----------|---------|----------|
| Effort on the safe route | One plan then unsafe / Source read only / Exhaust safe route | One plan, then unsafe |
| Containment | One module + CI grep / Per-module no cap / Separate crate | One module, one CI grep |
| Proof | XMM across yield + two threads / Criterion 2 as written / Add x87 + MXCSR | XMM across yield + two threads |
| Both routes fail | Stop and ask / Allow OSTD patch / Auto-trigger Redox re-score | Stop and ask me |

**Notes:** No free-text input.

## Claude's Discretion

IPC cost target, user code layout, syscall ABI shape (not selected for discussion).

## Deferred Ideas

None.
