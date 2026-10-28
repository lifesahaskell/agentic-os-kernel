---
phase: quick-261027-ttd
plan: 01
subsystem: docs/architecture
tags: [adr, scoping, linux, feature-floor, success-criteria]
status: complete
requirements: [QUICK-261027-ttd-SCOPING, QUICK-261027-ttd-ADR]
key-files:
  created:
    - docs/architecture/scoping-linux.md
    - docs/architecture/adr/0004-linux-prototype-first.md
  modified: []
decisions:
  - "ADR 0004 is Proposed; it supersedes ADR 0002 and 0003 in direction only (U1, U2)"
  - "Stock-kernel prototype first, mainline fork as escalation path with triggers (a)-(d) (U4, U6)"
commits: 2
plan_head_before: 875439f66c2a82169dedb3f43f17cccca3506e35
plan_head_after: 0442f129b81b8c521a40233d01ad7594ae0eab04
actuals:
  tasks: 2
  commits: 2
---

# Quick 261027-ttd: Linux-prototype-first scoping and ADR 0004 Summary

Two architecture documents, no code: `scoping-linux.md` (goals restated substrate-neutral, a 12-row by 6-distro feature-floor matrix, nine proposed success criteria S1-S9, five ranked fork candidates) and ADR 0004 (Proposed; stock-kernel prototype first; four named fork triggers tied to S1, S2, S3, S5 and the floor).

## Commits

| Task | Commit | Subject | Files |
|------|--------|---------|-------|
| 1 | 79f9fc8 | docs: add Linux-first scoping with feature floor and proposed criteria | docs/architecture/scoping-linux.md |
| 2 | 0442f12 | docs: add ADR 0004, Linux prototype first | docs/architecture/adr/0004-linux-prototype-first.md |

The three INPUT files were already committed at 875439f, so Task 1 staged nothing for them. No trailers or attribution lines on either commit (global rule). Locked decisions implemented: U1-U7 in the ADR, U8-U10 in the scoping doc, U11 in both.

## Verification

Verified (ran, passed):
- Both plan `<automated>` verify commands, executed as script files, before and after each commit: TASK1-OK and TASK2-OK. This covers headings, matrix shape (12 rows, 8 pipes per row, every data cell carrying VERIFIED or TO-VERIFY), S1-S9 rows each with the status string and the word "non-agent", fork-candidate order, banned terms in section 2, no emoji, no exclamation marks, no hedging words, no wiki links, no copied input line of 60 or more characters, forbidden paths unchanged, commit scope, no deletions, no trailers.
- Combined commit scope: `git log -n 2 --name-only` lists only the two target documents; `--diff-filter=D` prints nothing.
- Cross-document consistency, read by eye: ADR triggers cite S1, S2, S3, S5 and section 3, which exist in scoping-linux.md with the same meaning; ranks cited by the ADR (rank 4 for (a) and (c), rank 1 for (d)) match scoping-linux.md section 5.

Not verified:
- Every factual kernel claim. All values are carried from the three INPUT files with their labels; nothing was re-checked against kernel sources, distro packages or the web. The INPUT files were written by other agents and were not audited here.
- Line-count targets: scoping-linux.md is 256 lines (target 150-230, not gated); the ADR is 105 (target 70-110).
- Whether Landlock ABI 6 is sufficient for the abstract-unix-socket and signal scopes (see below).

## Matrix status (U10)

72 data cells (12 features by 6 columns: first-in-mainline plus five distros).
- 69 cells carry VERIFIED only, taken from the distro-kernels input.
- 1 cell is exactly TO-VERIFY: row 7 (bpf in the default LSM order), first-in-mainline column. The input gives no mainline version for a distro CONFIG_LSM choice.
- 2 cells carry VERIFIED plus a partial TO-VERIFY: row 5 Debian (CONFIG_CGROUP_BPF not recorded in the input) and row 11 first-in-mainline (per-cgroup PSI pressure files have no entry in the input).
- No cell was filled from memory. Ubuntu cells give GA 6.8 and HWE 7.0 inside one column.

## Interpretation and ungrounded items, for the user to accept or reject

Marked *(interpretation)* or "ungrounded" in the documents.

Success criteria. All nine are the planner's proposals; every row is "proposed, awaiting user acceptance".
- S1 threshold 0 is grounded in scoping.md section 2 only; completeness is an adversarial-test result, no input shows LSM hook coverage is complete.
- S2 (median 3%, worst task 8.4%) borrows AgentSight (under 3%, authors' claim) and ActPlane (1.9-8.4%), VERIFIED at abstract level. They are other systems' overheads on their own workloads, a bar and not a measurement of this design.
- S3 (p50 10%, p99 25% added): ungrounded. The gap analysis looked up no BPF-LSM or Landlock overhead numbers.
- S4 (p99 1 ms added): ungrounded.
- S5 threshold 0 rests on gap analysis 1a, 2f and V-src hook names; the test design is the planner's.
- S6 (one in-flight request of overshoot; cgroup killed within 1 s of deadline): numbers ungrounded.
- S7 (mean 1.05x, p99 1.10x at N = 1, 4, 16): numbers ungrounded; AIOS FIFO numbers to be reported alongside.
- S8 (0 over-admission; freeze within 1 s): numbers ungrounded.
- S9 threshold 0 follows scoping.md section 5 rule 1.

Floor tiers and reachability rule (section 3). All proposed, awaiting acceptance.
- Required set (CONFIG_BPF_LSM=y; bpf in active LSM order; Landlock at ABI 6 or higher; memcg; PSI) is *(interpretation)*.
- The Landlock minimum of ABI 6 is borrowed from the ABI Sandlock requires for a feature set that includes IPC scoping (fork-and-prior-art B1, VERIFIED at README level). No input states the ABI at which scopes first appear. That is TO-VERIFY and it decides the Debian and RHEL verdicts.
- Fallbacks for CLONE_NNP (`prctl` no_new_privs in the child) and PIDFD_GET_INFO (`SO_PEERCRED` plus `/proc/PID/cgroup`) are *(interpretation; not in the inputs)*.
- Reachability rule: kernel command-line change counts as reachable, rebuild counts as unreachable, TO-VERIFY counts neither way; trigger (d) at 3 or more of 5 distros VERIFIED unreachable. The 3-of-5 figure is the plan's.
- Ubuntu 24.04 is counted as reachable through its HWE kernel while GA 6.8 is unreachable (Landlock ABI 4); counting HWE is *(interpretation)*. Resulting count for trigger (d): 0 of 5.
- Row splits (BPF_LSM build versus lsm= order; memcg, PSI, dmem as three rows) are *(interpretation)*.

Fork triggers in ADR 0004.
- The "try first" non-fork responses are *(interpretation)*: separate-uid broker for (c) (from the plan); BPF map lookup for (b); an attachable hook on an adjacent path or removing the path from the grant for (a); narrowing the supported distro list for (d). The last three were written by the executor, not named in the plan.
- That no ranked fork candidate targets overhead (trigger (b)) is *(interpretation)*.
- Reading ADR 0001's "native means kernel-enforced, not kernel-resident" as applying again is *(interpretation)*.
- ADR 0003's per-client ledger as a candidate broker design is *(interpretation)*.

## Deviations from Plan

**1. [Rule 1 - consistency] Trigger (b) promotion**
- **Found during:** Task 1, Step 7 against Task 2, trigger (b).
- **Issue:** The plan says in Task 1 that triggers (a), (b) and (c) promote fork candidate 4, but says in Task 2 that for (b) "no ranked candidate targets overhead". The two documents could not both follow the plan literally.
- **Fix:** Followed the Task 2 wording in both documents: (a) and (c) promote rank 4; (b) promotes none, marked *(interpretation)*.
- **Files:** docs/architecture/scoping-linux.md section 5 (item 4), docs/architecture/adr/0004-linux-prototype-first.md trigger (b).
- **Commit:** 79f9fc8

**2. [Executor additions inside plan scope]**
- Added "Try first" lines for triggers (a), (b), (d), because the plan asks each trigger to state a non-fork response and names one only for (c). All are marked *(interpretation)*.
- Added two items to scoping-linux.md section 6 beyond the plan's list (resolve the TO-VERIFY items that decide the floor) and note 9 in section 3 (an open date conflict: the distro input lists Fedora kernel 7.2.13 as "stable 2026-10-28" while its evidence date is 2026-10-27; the fork-and-prior-art input dates 7.2.13 to 2026-10-26). Cell values do not depend on the date.
- scoping-linux.md exceeds the 230-line target (256) because of the 12-row matrix and ten notes.

## Auth gates

None. Signed commits succeeded without ssh-agent recovery.

## Known Stubs

None. Documents only. The WINDOWS.md ledger was not touched (no stubs, skipped tests or unrun verifies).

## Threat Flags

None. The plan's threat register mitigations T-261027-ttd-01, -02 and -03 were enforced by the verify gates listed above.

## Items for the orchestrator

- This SUMMARY.md is uncommitted by instruction; no changes to STATE.md, ROADMAP.md, PLAN.md.
- scoping-linux.md section 1 links ADR 0004; the link resolves from commit 0442f12 on.
- PROJECT.md, ROADMAP.md and REQUIREMENTS.md still describe the archived design; ADR 0004 records that aligning them is a separate task.

## Self-Check: PASSED

- docs/architecture/scoping-linux.md: found.
- docs/architecture/adr/0004-linux-prototype-first.md: found.
- Commits 79f9fc8 and 0442f12: ancestors of HEAD.
- git status clean apart from this SUMMARY.md.
