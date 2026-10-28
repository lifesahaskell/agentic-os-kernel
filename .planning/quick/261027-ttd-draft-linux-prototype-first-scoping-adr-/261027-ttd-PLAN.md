---
quick_id: 261027-ttd
mode: quick
phase: quick-261027-ttd
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - docs/architecture/scoping-linux.md
  - docs/architecture/adr/0004-linux-prototype-first.md
  - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md
  - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md
  - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md
autonomous: true
requirements: [QUICK-261027-ttd-SCOPING, QUICK-261027-ttd-ADR]

estimate:
  tokens: 75000
  raw_tokens: 75000
  tasks: 2
  confidence: low

must_haves:
  truths:
    - "docs/architecture/scoping-linux.md restates goals 1-4 of scoping.md without vocabulary tied to the archived design, and states that goal 5 (POSIX) now comes from Linux (U8)"
    - "Every data cell of the feature-floor matrix carries VERIFIED or TO-VERIFY, and every cell value comes from 261027-ttd-INPUT-distro-kernels.md or is TO-VERIFY (U10)"
    - "The success-criteria table has exactly nine rows S1-S9, each with a numeric threshold, a baseline that is the same call by a non-agent process on the same kernel, and the status 'proposed, awaiting user acceptance' (U8)"
    - "Fork candidates appear in the gap analysis's rank order 1-5 (U8)"
    - "docs/architecture/adr/0004-linux-prototype-first.md has Status Proposed, supersedes ADR 0002 and 0003 in direction only, names the archive tag and commit 1f1333e, records the fork advice exactly once, and lists fork triggers (a)-(d) (U1-U6)"
    - "No path outside docs/architecture/ and the quick directory changes, no existing file under docs/architecture/ changes, and nothing is deleted (U11)"
  artifacts:
    - path: docs/architecture/scoping-linux.md
      provides: "Substrate-neutral goals, feature-floor matrix, proposed success criteria S1-S9, ranked fork candidates"
      contains: "## 3. Feature floor"
    - path: docs/architecture/adr/0004-linux-prototype-first.md
      provides: "ADR 0004, Proposed: stock-kernel prototype first, fork as escalation path, triggers (a)-(d)"
      contains: "## What would show this was wrong"
  key_links:
    - from: docs/architecture/adr/0004-linux-prototype-first.md
      to: docs/architecture/scoping-linux.md
      via: "Triggers (a)-(d) cite rows S1, S2, S3, S5 and section 3 of scoping-linux.md by ID"
      pattern: "scoping-linux.md"
    - from: docs/architecture/scoping-linux.md
      to: .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md
      via: "Every matrix cell value and label is carried from this input"
      pattern: "261027-ttd-INPUT-distro-kernels.md"
    - from: docs/architecture/scoping-linux.md
      to: docs/architecture/adr/0004-linux-prototype-first.md
      via: "Section 1 links the ADR"
      pattern: "adr/0004-linux-prototype-first.md"
---

# Draft the Linux-prototype-first scoping: ADR 0004 and scoping-linux.md (documents only)

## Locked by the user (2026-10-27, via the orchestrator; cite the U-number in commits and the SUMMARY)

- **U1** ADR 0004 Status is **Proposed**, because the success-criteria numbers are unset.
- **U2** ADR 0004 supersedes ADR 0002 and ADR 0003 in direction only.
- **U3** Context: on 2026-10-27 the user dropped the OSTD/B2 microkernel path, archived at git tag `archive/ostd-b2-spike` (commit `1f1333e`), and chose Linux integration.
- **U4** Decision: a stock-kernel prototype first (cgroup v2 per agent; BPF-LSM including `BPF_LSM_CGROUP`; Landlock; seccomp; a userspace broker for token, money and wall-time budgets and for escalation). A mainline fork is the escalation path: the user accepted it as permissible and did not choose it first.
- **U5** Record once, in ADR 0004, that the user was advised a fork is unjustified until measured.
- **U6** ADR 0004 has a "What would show this was wrong" section with named fork triggers: (a) the adversarial suite finds a bypass BPF-LSM cannot close; (b) p99 mediation overhead above the bound; (c) the broker is killable or impersonable by an agent with the same uid; (d) the feature floor is unreachable on target distro kernels.
- **U7** ADR 0004 follows the shape and tone of docs/architecture/adr/0002-rust-microkernel.md.
- **U8** docs/architecture/scoping-linux.md: goals 1-4 restated substrate-neutral, goal 5 (POSIX) now free from Linux; a feature-floor matrix; a PROPOSED success-criteria table with numeric thresholds and non-agent baselines, every row labelled "proposed, awaiting user acceptance"; the open fork candidates ranked as in the gap analysis.
- **U9** Inputs: cite them by file name; copy no long passages; carry their evidence labels over; never upgrade an ASSUMED, SNIPPET, V-search or A claim to a fact.
- **U10** The matrix is built from 261027-ttd-INPUT-distro-kernels.md. A cell absent from it is TO-VERIFY. No cell is ever filled from memory.
- **U11** Do not touch kernel/, userland/, .planning/ROADMAP.md, .planning/PROJECT.md, .planning/REQUIREMENTS.md, .planning/STATE.md, or the Phase 2 UAT (.planning/phases/02-process-objects-teardown-and-timers/02-UAT.md). Delete nothing. Do not edit any existing file under docs/architecture/. Commit messages: short imperative subject, no trailers and no attribution lines of any kind (user's global rule; it overrides any template). Document voice: no emoji, no exclamation marks, no hedging words, no sycophancy; every uncertainty is a labelled uncertainty; reasoning no input states is marked *(interpretation)*. Wiki citation rules do not apply to these architecture docs.

<objective>
Write two architecture documents, no code: docs/architecture/scoping-linux.md (QUICK-261027-ttd-SCOPING, implements U8, U9, U10) and docs/architecture/adr/0004-linux-prototype-first.md (QUICK-261027-ttd-ADR, implements U1-U7). One atomic commit per document.

Purpose: record the 2026-10-27 change of direction with its evidence, and give the user concrete, numeric success criteria to accept or reject before ADR 0004 can move past Proposed.

Output: the two documents; the three INPUT files committed unchanged so the citations resolve in git.

Task order: scoping-linux.md first, because ADR 0004's triggers cite its row IDs (S1-S9) and section numbers. The ADR link from scoping-linux.md dangles for one commit; that is expected.

No tracer task: this plan has one layer (documents) and the user fixed a two-commit split.
</objective>

<execution_context>
@~/.claude/gsd-core/workflows/execute-plan.md
@~/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md
@.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md
@.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md
@docs/architecture/adr/0002-rust-microkernel.md
@docs/architecture/adr/0001-substrate.md
@docs/architecture/scoping.md

Evidence vocabularies in the inputs (carry each claim's label exactly as its input states it):
- 261027-ttd-INPUT-gap-analysis.md: V-src (source read at tag v7.3), V-doc (fetched page, summarised by a small model; second-hand), V-search (search snippet or news page only), A (ASSUMED).
- 261027-ttd-INPUT-fork-and-prior-art.md: VERIFIED (page fetched and read), SNIPPET (search summary only), ASSUMED.
- 261027-ttd-INPUT-distro-kernels.md: VERIFIED / TO-VERIFY (produced by a separate web-capable agent; read its own legend first, it is the authority for its labels).

Facts checked at planning time (2026-10-27): tag `archive/ostd-b2-spike` resolves to 1f1333e616e5b2d527aabf6799a973dc0670cf4d. Commits are SSH-signed (`commit.gpgsign=true`, `gpg.format=ssh`). The three INPUT files are untracked. At HEAD neither target document exists, and every forbidden path is clean.
</context>

<tasks>

<task type="auto">
  <name>Task 1: Write docs/architecture/scoping-linux.md (goals, feature floor, proposed criteria, fork candidates) and commit it</name>
  <files>docs/architecture/scoping-linux.md, .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md, .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md, .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md</files>
  <precondition>All three INPUT files exist under .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/ in this checkout; the distro-kernels file comes from a separate agent, so if it is absent, halt and report rather than build the matrix without it.</precondition>
  <read_first>
    - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-distro-kernels.md (read FIRST, in full, including its label legend, per U10)
    - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md (full)
    - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md (full)
    - docs/architecture/scoping.md sections 1, 2, 3, 5, 6 and 9 (the goals, primitives, coexistence rules, threat model and open questions being restated)
    - docs/architecture/adr/0002-rust-microkernel.md (voice and density to match)
  </read_first>
  <action>
Step 0. Assert the precondition with test -f on each of the three INPUT paths. If any is missing, stop, write nothing, and report which one.

Step 1. Read the distro-kernels INPUT first and in full (U10). Note its legend, the kernel version it records for each distro, and which features it covers.

Step 2. Create docs/architecture/scoping-linux.md with exactly these level-2 headings, in this order, spelled exactly: "## 1. The ask, restated", "## 2. Goals", "## 3. Feature floor", "## 4. Success criteria (proposed)", "## 5. Fork candidates", "## 6. Open questions", "## Sources". Title line: "# Scoping: Linux prototype first". Under the title, a status line in the style of scoping.md: "Status: draft 1, 2026-10-27. Companion to ADR 0004 (link adr/0004-linux-prototype-first.md). Supersedes the direction of scoping.md, which stays unedited as the record of the microkernel scoping." Target length 150-230 lines.

Citation and label rules for the whole document (U9). Cite an input inline by its file name plus the row or section, followed by the input's own label, for example "(261027-ttd-INPUT-gap-analysis.md 2d, V-search)". Never copy a table row or a passage longer than one sentence; paraphrase and cite. Never raise a label: a V-search, SNIPPET, A or ASSUMED claim keeps that label wherever it is repeated. Reasoning that no input states carries *(interpretation)*. Use plain markdown links, not the wiki's double-bracket link syntax. No HTML comments. Use ASCII comparison operators in thresholds (for example `<=`). Voice per U11: no emoji, no exclamation marks, no hedging words (the banned list is in this task's verify), no sycophancy; state each uncertainty as a labelled uncertainty.

Step 3. Section 1 (The ask, restated): three to six sentences. Same ask as scoping.md section 1, with the substrate changed to stock Linux on 2026-10-27 (ADR 0004; archived work at tag archive/ostd-b2-spike, commit 1f1333e). Carry over scoping.md's test for "natively agentic" (the protection boundary knows what an agent is), and state *(interpretation)* that on Linux that boundary is the kernel's LSM layer keyed on a per-agent cgroup.

Step 4. Section 2 (Goals). Restate goals 1-4 of scoping.md section 2 without any vocabulary tied to the archived design (the banned terms are in this task's verify): goal 1, agents are principals whose identity (agent_id, on_behalf_of, parent), authority, budgets (tokens, money, wall time) and lifecycle the OS holds, with sub-agents getting a subset of the parent's authority and non-agent children inheriting the agent's identity (from scoping.md 3.1); goal 2, every side effect an agent causes is mediated by a deterministic check the agent cannot bypass or disable, attributed to agent_id and on_behalf_of, with denials mapped to standard errno values and escalation over a trusted path (3.2, 3.3); goal 3, inference is a shared, scheduled, accounted device, local model or remote API behind one interface (3.4); goal 4, agent context is accounted memory with admission control (3.5). Goal 5: POSIX now comes from Linux; agent features stay additive and change no POSIX semantics for non-agent processes (scoping.md section 5); it is measured by row S9, not by an adherence tier. Then a subsection "### 2.1 Prototype mechanisms" with one bullet per goal naming the stock mechanism and the gap-analysis rows it rests on (goal 1: rows 1a, 1c, 1d, 1e; goal 2: 2a, 2b, 2c, 2d, 2e, 2f, 2g, 2h; goal 3: 3a, 3b, 3e; goal 4: 4a, 4b, 4c), with labels. Then "### 2.2 Trust boundary on Linux": a five-to-eight-line delta from scoping.md section 6: the TCB becomes the Linux kernel plus loaded BPF programs, the privileged BPF loader, and the broker (ADR 0001 rated option A's mediation strength 2 for this reason); model, agent and content stay untrusted; new threats are the same-uid agent acting against the broker (row S5, ADR 0004 trigger c), io_uring as a seccomp bypass (2d), and side effects through API keys an agent holds directly (2h, closed only by a credential-holding broker).

Step 5. Section 3 (Feature floor). Exactly one markdown table in this section, the matrix; everything else in section 3 is prose or bullets. Header row, seven columns: "| Feature | First in mainline | Ubuntu 24.04 LTS | Debian 13 | Fedora (current) | RHEL 10 family | Arch |". Header cells may append the kernel version the distro INPUT records for that distro. Exactly these twelve data rows, in this order, one feature per row: CLONE_INTO_CGROUP; CLONE_PIDFD_AUTOKILL; CLONE_NNP; PIDFD_GET_INFO (with PIDFD_INFO_CGROUPID); BPF_LSM_CGROUP attach type; CONFIG_BPF_LSM=y; bpf in the default LSM order (else the lsm= boot parameter must add it); Landlock ABI level; io_uring task-level restrictions; memcg (cgroup v2 memory controller); PSI per-cgroup pressure files; dmem cgroup controller. Rows 6 and 7 split the user's single "CONFIG_BPF_LSM plus lsm=bpf" item because the build option and the boot-time LSM order are separate facts per distro *(interpretation)*; row 10-12 split "memcg/PSI/dmem" for the same reason.

Cell rules (U10), applied to all six data columns: (i) take the value and label from the distro-kernels INPUT; keep VERIFIED and TO-VERIFY exactly as it states them; (ii) if the INPUT uses any other label for a cell, write the value followed by TO-VERIFY and the original label in parentheses; (iii) if the INPUT has no entry for a cell, the cell is exactly TO-VERIFY; (iv) never fill or correct a cell from memory, from a web lookup, or from the other two inputs. Facts the gap analysis or fork-and-prior-art state about a row (for example the gap analysis's 7.0 date for io_uring task-level restrictions, V-search, or the Sandlock README's Linux 6.12+ for Landlock ABI v6, VERIFIED in fork-and-prior-art B1) go in a numbered "Notes" list under the table with their own labels; the cell stays as the distro INPUT has it. Where the distro INPUT and another input disagree, keep the distro INPUT value in the cell and record the disagreement in Notes as an open conflict. If the distro INPUT covers more features than the twelve, mention them in Notes; do not add rows.

After the Notes, a bullet list "Floor tiers (proposed, awaiting user acceptance)" marking each row Required or Fallback available, as follows. Required *(interpretation)*: CONFIG_BPF_LSM=y; bpf in the active LSM order (by default or via the lsm= boot parameter); Landlock at the ABI that has abstract-unix-socket and signal scopes (state the ABI number only as the inputs give it, with its label); memcg; PSI. Fallback available, each with its fallback and source: CLONE_INTO_CGROUP, fork then move into the cgroup then exec, which leaves the spawn-time window the flag removes (gap analysis 1a); CLONE_PIDFD_AUTOKILL, a broker timer plus cgroup.kill (1e); CLONE_NNP, setting no_new_privs with prctl in the child before exec *(interpretation; not in the inputs)*; PIDFD_GET_INFO, SO_PEERCRED plus reading /proc/PID/cgroup, racy against pid reuse *(interpretation; not in the inputs)*; BPF_LSM_CGROUP, one global BPF-LSM program keyed on bpf_get_current_ancestor_cgroup_id (2a); io_uring task-level restrictions, the sysctl kernel.io_uring_disabled=2 (2d); dmem, needed only for local-GPU VRAM limits, not for remote-API inference (3e).

Then a "Reachability rule (proposed, awaiting user acceptance)" paragraph: the floor is reachable on a distro when every Required row is present in its stock kernel; a kernel command-line change counts as reachable, a kernel rebuild counts as unreachable; a TO-VERIFY cell is unknown and counts neither way; ADR 0004 trigger (d) fires when 3 or more of the 5 target distros are VERIFIED unreachable. Then one bullet per distro, derived from the matrix: reachable, unreachable, or unknown, naming the deciding row.

Step 6. Section 4 (Success criteria (proposed)). One table, header "| ID | Criterion | Measure | Proposed threshold | Baseline | Grounding | Status |", exactly nine data rows S1-S9. Every Status cell is exactly "proposed, awaiting user acceptance". Every Baseline cell names the same call or workload by a non-agent process on the same kernel and contains the word "non-agent". Every Grounding cell cites its input and label, or says "ungrounded: no input measures this" plus *(interpretation)*. The rows (proposed by the planner; write them as given):
- S1 Mediation completeness (goal 2). Measure: adversarial-suite cases in which an agent causes a side effect its policy denies. Threshold: 0. Baseline: the same suite run by a non-agent process on the same kernel, where every case's side effect occurs, which shows each case reaches a real path. Grounding: scoping.md section 2 (target 0); gap analysis 2a and "Not found" (no source shows LSM hook coverage is complete; completeness is an adversarial-test result).
- S2 Workload overhead (goal 2). Measure: end-to-end wall time of an agent task suite under full mediation. Threshold: median across tasks at most 3%, worst task at most 8.4%. Baseline: the same tasks run by a non-agent process on the same kernel. Grounding: AgentSight under 3% (authors' claim) and ActPlane 1.9-8.4%, both VERIFIED at abstract level in fork-and-prior-art B1; add *(interpretation)* that these are other systems' overheads on their own workloads, a bar and not a measurement of this design.
- S3 Per-call overhead on kernel-enforced paths (goal 2). Measure: added latency per mediated call for openat, connect, execve and unix-socket sendmsg microbenchmarks with BPF-LSM, Landlock and seccomp active. Threshold: p50 at most 10% added, p99 at most 25% added. Baseline: the same call by a non-agent process on the same kernel. Grounding: ungrounded (gap analysis "Not found": no BPF-LSM or Landlock overhead numbers looked up); revise after the first baseline run.
- S4 Broker round trip (goals 2 and 3). Measure: added latency for a tool invocation and for an egress connect routed through the broker. Threshold: p99 at most 1 ms added. Baseline: the same call made directly by a non-agent process on the same kernel. Grounding: ungrounded; broker-routed egress per gap analysis 2c.
- S5 Broker integrity (goal 1). Measure: attempts by an agent running with the broker's uid to kill, signal, ptrace, or impersonate the broker (bind its socket name, or present another agent's identity on its socket) that succeed. Threshold: 0. Baseline: the same attempts by a non-agent process with that uid on the same kernel, which succeed, showing the protection is agent-specific and that the test reaches the target. Grounding: gap analysis 1a (agents unprivileged in a delegated subtree), 2f (SO_PASSPIDFD plus PIDFD_INFO_CGROUPID), goal 2 table (task_kill and ptrace_access_check hooks, V-src).
- S6 Budget enforcement (goal 1). Measure: overshoot after a token, money or wall-time budget is exhausted. Threshold: tokens and money, at most one in-flight request per agent (its max_tokens and its price); wall time, the agent's cgroup killed within 1 s of its deadline. Baseline: the same workload by a non-agent process on the same kernel, which has no budget, so overshoot is unbounded. Grounding: gap analysis 1d, 1e; numbers ungrounded.
- S7 Inference scheduling (goal 3). Measure: mean and p99 request turnaround through the broker's inference queue at N = 1, 4 and 16 concurrent agents. Threshold: mean at most 1.05x and p99 at most 1.10x the baseline at each N. Baseline: the same requests sent directly to the same backend by N non-agent processes on the same kernel. Grounding: gap analysis 3a; scoping.md section 2 named AIOS FIFO numbers (aios p. 9) as its baseline, so report them alongside, not as the threshold; numbers ungrounded.
- S8 Admission control (goal 4). Measure: (i) agents admitted while summed context capacity is exceeded; (ii) time from a memory.pressure trigger to freezing the lowest-priority agent. Threshold: (i) 0; (ii) at most 1 s. Baseline: the same workload by non-agent processes on the same kernel with no admission control, reporting peak memory.pressure and OOM kills. Grounding: gap analysis 4b; numbers ungrounded.
- S9 POSIX non-interference (goal 5). Measure: tests whose result differs between a non-agent run and a run inside an agent with a full grant. Threshold: 0. Baseline: the same suite run by a non-agent process on the same kernel. Suite: os-test, as scoping.md section 2 names it. Grounding: scoping.md section 5 rule 1 (the scores must match).

Under the table, a bullet list "Rows of scoping.md section 2 not carried": POSIX adherence tier (Linux supplies POSIX; replaced by S9); POSIX layer cost (no personality layer on Linux; S2 and S3 measure the added cost instead); snapshot cost (an inference-runtime property per gap analysis 3d; no proposal here, listed in section 6).

Step 7. Section 5 (Fork candidates). A numbered list in the gap analysis's rank order ("Where a fork buys something real, ranked"), one item per rank, each item's first line starting with these bold names exactly: "1. **Backports for an LTS base**", "2. **Audit records with agent identity**", "3. **DRM scheduling cgroup controller**", "4. **In-kernel agent credential and budget controller**", "5. **`SO_PEERCGROUPID`**". Each item states what it buys, the gap rows it closes (1: 2d and the 6.18-versus-7.x base choice; 2: 2e; 3: 3c, local GPU only; 4: 1b, 1f; 5: 2f), its label, and which ADR 0004 trigger would promote it ((d) promotes 1; (a), (b) and (c) promote 4; 3 depends on scoping.md open question 3, local GPU or remote API). After the list: "Carrying cost" bullets from fork-and-prior-art A1 and A2 with labels (Asahi's 858 patches and about 83,000 lines at 6.18.8 with weekly rebase hours, VERIFIED; cost driven by touched existing files rather than line count, ASSUMED there; pin one LTS, 6.18 or 6.12, both EOL Dec 2028 VERIFIED, rebase yearly as a policy that input labels ASSUMED). Then one sentence: fork-and-prior-art B3 names a kernel-recognised credential, a trusted escalation path and kernel-enforced token and money budgets as candidates (ASSUMED there); the gap analysis places the credential and budget controller at rank 4 and the trusted path in userspace (2g); this document keeps the gap analysis ranking.

Step 8. Section 6 (Open questions), numbered: accept, change or reject each of S1-S9 (ADR 0004 stays Proposed until then); accept or change the floor tiers and the reachability rule, and confirm the five target distros; local GPU or remote API (scoping.md open question 3), which decides whether dmem and fork candidate 3 matter; whether to add a snapshot-cost criterion; which LTS a fork would pin if a trigger fires.

Step 9. Section Sources: markdown links, relative to docs/architecture/, to the three INPUT files (../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/ followed by each file name), to scoping.md, adr/0001-substrate.md, adr/0002-rust-microkernel.md and adr/0004-linux-prototype-first.md, plus one line per input restating its label legend.

Step 10. Run this task's verify. Fix every failure in the document and re-run until it passes.

Step 11. Commit. Stage only docs/architecture/scoping-linux.md and the three INPUT files (adding an INPUT file that the orchestrator already committed is a no-op). Commit with the subject "docs: add Linux-first scoping with feature floor and proposed criteria" and no body trailers or attribution lines (U11). If the signed commit fails with an ssh_askpass error, run ssh-agent-init, then source ~/.ssh/agent.env, and retry the same commit; if that asks for a passphrase, stop and report it to the user rather than work around it. Then run the commit-scope part of verify again.
  </action>
  <verify>
    <automated>F=docs/architecture/scoping-linux.md; Q=.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-; test -f "$F" && for h in '^# Scoping: Linux prototype first' '^## 1\. The ask, restated' '^## 2\. Goals' '^### 2\.1 Prototype mechanisms' '^### 2\.2 Trust boundary on Linux' '^## 3\. Feature floor' '^## 4\. Success criteria \(proposed\)' '^## 5\. Fork candidates' '^## 6\. Open questions' '^## Sources'; do grep -qE "$h" "$F" || { echo "missing heading $h"; exit 1; }; done && M=$(awk '/^## 3\. Feature floor/{f=1;next} /^## /{f=0} f && /^\|/' "$F") && echo "$M" | head -1 | grep -q 'Ubuntu 24.04' && echo "$M" | head -1 | grep -q 'Debian 13' && echo "$M" | head -1 | grep -q 'Fedora' && echo "$M" | head -1 | grep -q 'RHEL 10' && echo "$M" | head -1 | grep -q 'Arch' && test "$(echo "$M" | tail -n +3 | wc -l)" -eq 12 && echo "$M" | tail -n +3 | awk -F'|' 'NF!=9 {print "wrong column count: " $0; bad=1} {for(i=3;NF>i;i++) if ($i !~ /VERIFIED|TO-VERIFY/) {print "cell without label: " $0; bad=1}} END{exit bad}' && for t in CLONE_INTO_CGROUP CLONE_PIDFD_AUTOKILL CLONE_NNP PIDFD_GET_INFO BPF_LSM_CGROUP CONFIG_BPF_LSM 'lsm=' Landlock io_uring memcg PSI dmem; do echo "$M" | grep -q -- "$t" || { echo "matrix lacks $t"; exit 1; }; done && C=$(awk '/^## 4\. Success criteria/{f=1;next} /^## /{f=0} f && /^\| S[1-9] /' "$F") && test "$(echo "$C" | wc -l)" -eq 9 && echo "$C" | awk '!/proposed, awaiting user acceptance/ || !/non-agent/ {print "row lacks status or non-agent baseline: " $0; bad=1} END{exit bad}' && for s in S1 S2 S3 S4 S5 S6 S7 S8 S9; do echo "$C" | grep -q "^| $s " || { echo "missing row $s"; exit 1; }; done && grep -q '8\.4' "$F" && grep -q '1\.9' "$F" && R=$(awk '/^## 5\. Fork candidates/{f=1;next} /^## /{f=0} f' "$F") && echo "$R" | grep -q '^1\. \*\*Backports for an LTS base' && echo "$R" | grep -q '^2\. \*\*Audit records with agent identity' && echo "$R" | grep -q '^3\. \*\*DRM scheduling cgroup controller' && echo "$R" | grep -q '^4\. \*\*In-kernel agent credential and budget controller' && echo "$R" | grep -q '^5\. \*\*`SO_PEERCGROUPID`' && grep -q 'Floor tiers' "$F" && grep -q 'Reachability rule' "$F" && grep -q '261027-ttd-INPUT-gap-analysis.md' "$F" && grep -q '261027-ttd-INPUT-fork-and-prior-art.md' "$F" && grep -q '261027-ttd-INPUT-distro-kernels.md' "$F" && grep -q 'adr/0004-linux-prototype-first.md' "$F" && ! awk '/^## 2\. Goals/{f=1;next} /^## /{f=0} f' "$F" | grep -qiE 'OSTD|microkernel|capability handle|capability space|kernel object' && ! LC_ALL=C.UTF-8 grep -nP '[\x{1F000}-\x{1FAFF}\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{FE0F}]' "$F" && ! grep -n '!' "$F" && ! grep -niwE 'perhaps|seems|arguably|possibly|probably|might|likely' "$F" && ! grep -n 'UNVERIFIED' "$F" && ! grep -n '\[\[' "$F" && ! bash -c "grep -Fxqh -f <(grep -E '^[^|].{59,}' $F) $Q/261027-ttd-INPUT-*.md" && git diff --quiet -- kernel userland .planning/ROADMAP.md .planning/PROJECT.md .planning/REQUIREMENTS.md .planning/STATE.md .planning/phases/02-process-objects-teardown-and-timers/02-UAT.md docs/architecture/scoping.md docs/architecture/adr && N=$(git show --name-only --format= HEAD) && test -n "$N" && test -z "$(echo "$N" | grep -vE '^(docs/architecture/scoping-linux\.md|\.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/.*)$')" && echo "$N" | grep -qx 'docs/architecture/scoping-linux.md' && DEL=$(git show --diff-filter=D --name-only --format= HEAD) && test -z "$DEL" && B=$(git log -1 --format=%B) && ! echo "$B" | grep -qiE 'co-authored|claude-session|generated with' && S=$(git log -1 --format=%s) && echo "$S" | grep -q '^docs: add Linux-first scoping' && echo TASK1-OK</automated>
  </verify>
  <done>docs/architecture/scoping-linux.md exists with the seven required sections and two subsections; the matrix has 12 rows and 7 columns and every data cell carries VERIFIED or TO-VERIFY taken from the distro-kernels input or defaulted to TO-VERIFY; the criteria table has rows S1-S9, each with a numeric threshold, a non-agent baseline and the status "proposed, awaiting user acceptance"; fork candidates 1-5 follow the gap analysis order; style gates pass; one commit touches only scoping-linux.md and the quick directory, deletes nothing, and carries no trailers. Baselines at HEAD: the file is absent, so every positive gate fails before the work.</done>
</task>

<task type="auto">
  <name>Task 2: Write docs/architecture/adr/0004-linux-prototype-first.md (Proposed, triggers a-d) and commit it</name>
  <files>docs/architecture/adr/0004-linux-prototype-first.md</files>
  <precondition>Task 1's commit exists and docs/architecture/scoping-linux.md contains rows S1-S9 and section 3, which this ADR cites by ID.</precondition>
  <read_first>
    - docs/architecture/adr/0002-rust-microkernel.md (shape, header bullets, section rhythm and tone to follow, per U7)
    - docs/architecture/adr/0003-server-cpu-exemption.md (header and Context only)
    - docs/architecture/adr/0001-substrate.md (option A text, its seccomp correction, and the scores)
    - docs/architecture/scoping-linux.md (as written by Task 1: row IDs, section numbers, fork-candidate ranks)
    - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-gap-analysis.md (Headline; "Where a fork buys something real, ranked" and its closing counter-argument)
    - .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-INPUT-fork-and-prior-art.md (A1, A2, B1, B3)
  </read_first>
  <action>
Write docs/architecture/adr/0004-linux-prototype-first.md following ADR 0002's shape and tone (U7): title, header bullets, short sections, plain declarative sentences, *(interpretation)* where reasoning is ours. Target 70-110 lines. Same voice, label and citation rules as Task 1 (U9, U11): inputs cited by file name with their labels, no label raised, no passage longer than one sentence copied, no wiki double-bracket links, no HTML comments, no emoji, no exclamation marks, no hedging words (banned list in this task's verify).

1. Title: "# ADR 0004: Linux prototype first, mainline fork as the escalation path".

2. Header bullets, in this order: "- Status: **Proposed** (direction chosen by Corey 2026-10-27; it moves past Proposed only when the user accepts the success-criteria thresholds in scoping-linux.md section 4, which are unset)" per U1; "- Date: 2026-10-27"; "- Supersedes: [ADR 0002](0002-rust-microkernel.md) and [ADR 0003](0003-server-cpu-exemption.md), in direction only. Both stay as records, unedited." per U2; "- Scoping: [scoping-linux.md](../scoping-linux.md)"; "- Archived work: git tag `archive/ostd-b2-spike`, commit `1f1333e`" per U3; "- Inputs:" followed by the three INPUT file names (links relative to docs/architecture/adr/, that is ../../../.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/ followed by the file name).

3. "## Context": the user decided on 2026-10-27 to drop the OSTD/B2 microkernel path and pursue Linux integration (U3). ADR 0002 chose the microkernel on a hard requirement (agents are kernel objects, not processes with policy attached), not on score: its own re-score left option A highest at 25. ADR 0003 settled how microkernel servers are charged for client work. State the gap analysis headline with its scope: across goals 1-4 it found no gap that must be a kernel patch; the PATCH items are tamper-resistance, atomicity or overhead improvements, plus per-process GPU time sharing on a local GPU (261027-ttd-INPUT-gap-analysis.md, Headline); it measured nothing.

4. "## Why stock before fork": three short bullets. (i) The gap analysis closing argument: scoping.md's mediation-overhead and layer-cost criteria have no targets, a stock prototype yields those baselines cheaply, and a fork should answer a measured miss. (ii) Carrying cost: Asahi carries about 858 patches and 83,000 lines with weekly rebase hours (fork-and-prior-art A1, VERIFIED); long-lived out-of-tree efforts either shrank by upstreaming or had a funded team (A2, interpretation there). (iii) Prior art: Sandlock, ActPlane and AgentSight build agent confinement and observation on unmodified kernels (B1, VERIFIED at abstract or README level); warning carried from B3: AgentCgroup depends on the unmerged memcg_bpf_ops series (README VERIFIED, RFC status SNIPPET), so a no-fork prototype can still end up carrying patches.

5. "## Decision" per U4: build a stock-kernel prototype first, as a numbered list: (1) one cgroup v2 cgroup per agent, sub-agents as child cgroups, agent identity as the cgroup id readable from the pidfd, spawned with CLONE_INTO_CGROUP where the floor provides it (gap analysis 1a and cross-cutting table); (2) BPF-LSM as the deny-by-default mediation point, with per-agent attach through BPF_LSM_CGROUP (2a, 2b); (3) Landlock as monotone-narrowing self-restriction inherited by children (1c, 2i); (4) seccomp as a syscall allowlist for defence in depth, never the enforcement point for pointer-argument decisions (ADR 0001 correction; 2b); (5) a userspace broker that holds token, money and wall-time budgets and enforces them with cgroup.kill, cgroup.freeze or handle revocation (1d, 1e), owns escalation over a trusted path the agent holds no fd for (2g), holds real credentials for egress (2c, 2h), and runs the inference queue (3a). Then: a mainline fork is the escalation path; the user accepted it as permissible and did not choose it first; it is taken only when a trigger below fires. Then exactly one sentence recording the advice (U5), containing the phrase "unjustified until measured", for example: "Corey was advised on 2026-10-27 that a fork is unjustified until measured, because no goal 1-4 gap requires a patch and no criterion has a baseline yet." Do not repeat the advice anywhere else in the ADR.

6. "## What would show this was wrong" per U6: open with one sentence stating that each trigger reads a row or section of scoping-linux.md and cannot fire until the user accepts that row. Then exactly four bullets, each starting with its bold label exactly as written here, each stating what is measured, the firing condition, the fork candidate it would promote (by rank in scoping-linux.md section 5), and the non-fork response to try first:
   - "- **(a) Unclosable bypass.**" Reads S1. Fires when the adversarial suite finds a side-effect path that no attachable LSM hook, Landlock right or cgroup-BPF program closes; a seccomp-only closure does not count, because seccomp is not an enforcement point (ADR 0001 correction). Note the attach-disabled hook set the gap analysis lists (goal 1 table, V-src). Promotes rank 4, or a patch adding the hook.
   - "- **(b) Mediation overhead above the bound.**" Reads S2 (worst task) and S3 (p99). Fires when either exceeds its accepted bound on the reference kernel and profiling attributes the excess to the kernel mediation path rather than the broker, which a fork does not change. *(interpretation)* No ranked candidate targets overhead; the profile picks the change.
   - "- **(c) Killable or impersonable broker.**" Reads S5. Fires when S5 is above 0 with the BPF-LSM task_kill, ptrace_access_check and unix-socket hooks in place. Non-fork alternative to test first: run the broker under its own uid *(interpretation)*; the trigger concerns the same-uid design. Promotes rank 4 (agent credential held in the kernel, gap analysis 1b).
   - "- **(d) Feature floor unreachable.**" Reads scoping-linux.md section 3 (floor tiers and reachability rule). Fires when 3 or more of the 5 target distros are VERIFIED unreachable. Promotes rank 1 (choose an LTS base and carry backports).

7. "## Consequences", bullets: ADR 0002 and 0003 stay as records; the microkernel work is frozen at the archive tag; kernel/ and userland/ stay in the tree and this ADR deletes nothing. ADR 0002's hard requirement no longer excludes option A: on the prototype an agent is a cgroup plus BPF-LSM, Landlock and seccomp policy, the shape ADR 0002 excluded, so ADR 0001's meaning of native (kernel-enforced, not kernel-resident) applies again until a fork trigger fires *(interpretation)*. Goal 5 comes from Linux, and S9 replaces the adherence tier. The TCB becomes the whole Linux kernel plus BPF programs, the privileged loader and the broker (ADR 0001 rated A's mediation strength 2 for this). ADR 0003's problem recurs for the broker and inference daemon: memcg charges the allocating task's cgroup (gap analysis 4c and goal 4 table, A), and its per-client ledger is a candidate design for the broker *(interpretation)*. PROJECT.md, ROADMAP.md and REQUIREMENTS.md describe the archived design, including the core-value statement about capability handles; this ADR does not change them, and aligning them is a separate task.

8. Run this task's verify; fix every failure and re-run until it passes.

9. Commit only docs/architecture/adr/0004-linux-prototype-first.md with the subject "docs: add ADR 0004, Linux prototype first" and no trailers or attribution lines (U11). Same ssh-agent recovery as Task 1 if signing fails. Then re-run the commit-scope part of verify.
  </action>
  <verify>
    <automated>F=docs/architecture/adr/0004-linux-prototype-first.md; Q=.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-; test -f "$F" && grep -q '^# ADR 0004: Linux prototype first' "$F" && grep -q '^- Status: \*\*Proposed\*\*' "$F" && ! grep -q 'Status: \*\*Accepted' "$F" && grep -qE '^- Supersedes:.*0002.*0003.*in direction only' "$F" && grep -q 'archive/ostd-b2-spike' "$F" && grep -q '1f1333e' "$F" && grep -q '\.\./scoping-linux\.md' "$F" && for h in '^## Context' '^## Why stock before fork' '^## Decision' '^## What would show this was wrong' '^## Consequences'; do grep -qE "$h" "$F" || { echo "missing heading $h"; exit 1; }; done && D=$(awk '/^## Decision/{f=1;next} /^## /{f=0} f' "$F") && for t in 'cgroup v2' BPF_LSM_CGROUP Landlock seccomp broker scalation; do echo "$D" | grep -q -- "$t" || { echo "decision lacks $t"; exit 1; }; done && test "$(grep -o 'unjustified until measured' "$F" | wc -l)" -eq 1 && W=$(awk '/^## What would show this was wrong/{f=1;next} /^## /{f=0} f' "$F") && test "$(echo "$W" | grep -E '^- \*\*\([a-z]\) ' | wc -l)" -eq 4 && echo "$W" | grep -q '^- \*\*(a) Unclosable bypass' && echo "$W" | grep -q '^- \*\*(b) Mediation overhead above the bound' && echo "$W" | grep -q '^- \*\*(c) Killable or impersonable broker' && echo "$W" | grep -q '^- \*\*(d) Feature floor unreachable' && for r in S1 S2 S3 S5 'section 3'; do echo "$W" | grep -q "$r" || { echo "triggers lack $r"; exit 1; }; done && grep -q 'PROJECT.md' "$F" && grep -q '261027-ttd-INPUT-gap-analysis.md' "$F" && grep -q '261027-ttd-INPUT-fork-and-prior-art.md' "$F" && ! LC_ALL=C.UTF-8 grep -nP '[\x{1F000}-\x{1FAFF}\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{FE0F}]' "$F" && ! grep -n '!' "$F" && ! grep -niwE 'perhaps|seems|arguably|possibly|probably|might|likely' "$F" && ! grep -n '\[\[' "$F" && ! bash -c "grep -Fxqh -f <(grep -E '^[^|].{59,}' $F) $Q/261027-ttd-INPUT-*.md" && git diff --quiet -- kernel userland .planning/ROADMAP.md .planning/PROJECT.md .planning/REQUIREMENTS.md .planning/STATE.md .planning/phases/02-process-objects-teardown-and-timers/02-UAT.md docs/architecture/scoping.md docs/architecture/adr/0001-substrate.md docs/architecture/adr/0002-rust-microkernel.md docs/architecture/adr/0003-server-cpu-exemption.md && N=$(git show --name-only --format= HEAD) && test "$N" = "docs/architecture/adr/0004-linux-prototype-first.md" && B=$(git log -1 --format=%B) && ! echo "$B" | grep -qiE 'co-authored|claude-session|generated with' && S=$(git log -1 --format=%s) && echo "$S" | grep -q '^docs: add ADR 0004' && echo TASK2-OK</automated>
  </verify>
  <done>ADR 0004 exists with Status Proposed, supersedes ADR 0002 and 0003 in direction only, names tag archive/ostd-b2-spike and commit 1f1333e, links scoping-linux.md, has the five required sections, records the fork advice exactly once, and lists exactly four triggers (a)-(d) that cite S1, S2, S3, S5 and section 3; style gates pass; one commit touches only the ADR file and carries no trailers. Baseline at HEAD: the file is absent.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| subagent INPUT files to architecture docs | Unreviewed research output, with mixed evidence labels, becomes a durable decision record |
| executor to repository | The executor commits; forbidden paths and deletions must not ride along |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-261027-ttd-01 | Tampering | scoping-linux.md feature-floor matrix | high | mitigate | Cells only from the distro-kernels input, absent cells TO-VERIFY (U10); Task 1 verify fails on any matrix cell without VERIFIED or TO-VERIFY and on the string UNVERIFIED |
| T-261027-ttd-02 | Repudiation | evidence labels carried from the inputs | medium | mitigate | Label-carry rule in both actions (U9); copied-line gate fails on any line of 60 or more characters that appears verbatim in an input |
| T-261027-ttd-03 | Tampering | forbidden paths (kernel/, userland/, planning files, Phase 2 UAT, existing ADRs, scoping.md) | high | mitigate | Each task's verify runs git diff --quiet on them and checks that the task commit names only its allowed paths and deletes nothing |
| T-261027-ttd-04 | Information disclosure | committed INPUT files | low | accept | They contain local scratchpad paths and public URLs only; no secrets |
| T-261027-ttd-SC | Tampering | npm/pip/cargo installs | high | accept | This plan installs no packages |
</threat_model>

<verification>
After both tasks, from the checkout root:
- Both task verify commands print TASK1-OK and TASK2-OK.
- Combined commit scope (raise -n if more than two commits were made by this plan): git log -n 2 --name-only --format= lists only docs/architecture/scoping-linux.md, docs/architecture/adr/0004-linux-prototype-first.md and files under .planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/; git log -n 2 --diff-filter=D --name-only --format= prints nothing.
- Cross-document consistency, read by eye: every trigger in ADR 0004 names a row or section that exists in scoping-linux.md with the same meaning, and the fork-candidate ranks ADR 0004 cites match scoping-linux.md section 5.
- The SUMMARY lists which matrix cells are VERIFIED and how many are TO-VERIFY, and every *(interpretation)* the planner supplied (floor tiers, the CLONE_NNP and PIDFD_GET_INFO fallbacks, the separate-uid alternative in trigger c, the S3-S8 numbers) so the user sees what is ungrounded.
</verification>

<success_criteria>
- Two documents exist and are committed in two commits with short imperative subjects and no trailers.
- The user can read off: which kernel features the prototype needs, per target distro, with each cell VERIFIED or TO-VERIFY; nine numeric criteria awaiting acceptance; four named fork triggers tied to those criteria; the ranked fork candidates.
- No file outside docs/architecture/ and the quick directory changed; no existing docs/architecture/ file changed; nothing deleted.
</success_criteria>

<output>
Create `.planning/quick/261027-ttd-draft-linux-prototype-first-scoping-adr-/261027-ttd-SUMMARY.md` with `status: complete` in its frontmatter when done. Do not commit the SUMMARY; the orchestrator commits it.
</output>
