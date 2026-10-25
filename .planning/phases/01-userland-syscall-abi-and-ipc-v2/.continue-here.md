---
context: phase
phase: 01-userland-syscall-abi-and-ipc-v2
task: 7
total_tasks: 7
status: in_progress
---

# BLOCKING CONSTRAINTS

- [ ] gsd-executor dispatch needs `isolation="worktree"` (isolation-guard hook); merge with worktree.record-agent + worktree.cleanup-wave.
- [ ] No `Co-Authored-By:` / `Claude-Session:` trailers on commits.

<current_state>
Plans 01-01..01-06 merged (01-06 as f94fa31, 66 ktests, check.sh green per executor). No worktrees in flight. Only 01-07 and the verify step remain.
</current_state>

<blockers>
- Flaky boot check `aok: cpu budget check` (~1 in 4), predates 01-05.
- Callers parked on a dead server stay blocked; interrupt waits instead of EINTR (shortcut in `abandon`, 01-06 SUMMARY). Phase 2 kill path is the planned fix.
- REQUIREMENTS.md marks KERN-03 / IPC-01 complete early; verifier must check.
- Untested: ring-3 bulk calls with oversized/kernel-address buffers.
</blockers>

<next_action>
Run `/gsd-execute-phase 1` (resumes at 01-07). Then verify_phase_goal: code review gate, gsd-verifier, `phase.complete 1`.
Leave .planning/config.json, milestone.lock, state.json uncommitted (pre-existing).
</next_action>
