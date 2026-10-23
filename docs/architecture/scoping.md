# Scoping: a natively agentic, POSIX-certified OS

Status: draft 0, 2026-10-23. It builds on the wiki (`wiki/index.md`) and links concept pages as
`[[slug]]`. Claims about the sources cite them. Everything else is design reasoning and open to
challenge.

## 1. The ask, restated

Build an operating system where **LLM agents are principals the system itself schedules,
accounts for and constrains**. Agents should not be applications that call a model. The system
must also earn **formal POSIX certification**, so ordinary POSIX software runs on it unchanged
and the product is on the Open Group register.

Two consequences follow before any design work:

- **Certification pins down the substrate more than anything else.** Fees are trivial ($5k).
  The real cost is passing ~34,600 tests in the 1003.1-2016 Base suites, and POSIX.1-2024 has no
  suite yet ([posix-certification.md](posix-certification.md)).
- **"Natively agentic" needs a testable definition**, or it collapses into "AIOS with a nicer
  installer". The definition used here: *the protection boundary knows what an agent is.*
  Every action an agent takes on files, network, tools, other agents or the model passes a
  kernel-enforced check against that agent's capabilities. In AIOS, the access manager is
  advisory, Table 2 has no access or tool syscalls (aios p. 7), and nothing prevents bypass
  ([[aios-ostep-mapping]]).

## 2. Goals, non-goals, and success criteria

**Goals**
1. Agents are first-class principals: identity, capabilities, budgets and lifecycle are managed
   by the OS ([[agent-as-process]], [[process]]).
2. Every side effect an agent causes is mediated and attributable ([[access-control]],
   [[capabilities]]).
3. Model inference is a shared, scheduled, accounted device ([[agent-scheduler]],
   [[scheduling]]).
4. Agent context is accounted memory with admission control ([[working-set-and-thrashing]],
   [[context-window-management]]).
5. The system is certified to IEEE 1003.1-2016 Base, and moves to the Issue 8 standard when a
   suite exists.

**Non-goals (v1)**
- Training or hosting frontier models. The inference device can be a local model or a remote
  API behind the same interface.
- The UNIX trademark (XSI, curses, a C-compiler suite, $25k+/yr). See
  [posix-certification.md](posix-certification.md).
- Making the LLM part of the trusted computing base ([[trusted-computing-base]]).
- A GUI.

**Success criteria.** These need numbers from Corey; see section 9.

| Criterion | Measure | Target | Baseline |
|---|---|---|---|
| POSIX | VSX-PCTS2016 + VSC-PCTS2016 formal run | 0 unresolved FAIL; on register | n/a |
| Mediation completeness | Agent side-effect paths that bypass the broker, found by an adversarial test suite | 0 | AIOS: unmediated |
| Mediation overhead | p50/p99 added latency per mediated syscall | **TBD** | Unmediated Linux |
| Inference scheduling | Mean and p99 agent turnaround under N concurrent agents | **TBD** | AIOS FIFO numbers (aios p. 9), re-measured on our hardware |
| Snapshot cost | Bytes and ms per context snapshot/restore | **TBD** | Not reported by AIOS |

## 3. Agentic primitives

Each primitive pairs an OSTEP mechanism with what changes for agents.

### 3.1 Agent principal
A POSIX process plus a kernel-recognized **agent credential**:
- `agent_id`
- the human principal the agent acts for (`on_behalf_of`)
- a parent agent
- a capability set
- budgets: tokens, wall time, money

Sub-agents get a **subset** of their parent's capabilities, never more. This is the delegation
pattern capabilities handle natively (ostep pp. 733-734; [[capabilities]]). An agent's
non-agent child processes inherit its credential, so shelling out does not escape mediation.

### 3.2 Mediation (the agent trap)
This is the agentic equivalent of [[limited-direct-execution]]. Agents run deprivileged. Some
operations trap to a **broker**, which decides allow, deny or escalate:
- file writes outside the agent's scratch area
- network connects
- process spawns
- tool invocations
- IPC to other agents

Denials surface as `EACCES`/`EPERM`, so POSIX tools behave. The broker is deterministic code in
the TCB, and it never asks an LLM whether an action is allowed.

### 3.3 Escalation and the trusted path
Some actions need a human to confirm them: irreversible, external, or expensive ones. The
confirmation must travel over a **trusted path** that the agent cannot spoof or answer itself:
a broker-owned prompt on a channel the agent has no capability for. *(Interpretation. Later AIOS
versions reportedly add user confirmation; v2 does not describe it — see [[access-manager]].)*

### 3.4 Inference device
Inference is a device with a request queue, like a disk ([[io-devices-and-drivers]]).

- **Scheduling:** start with FIFO and round robin as in AIOS (aios p. 5). An MLFQ-style policy
  ([[mlfq]]) needs a signal for job length, and output length is unknown until generation stops.
  That is an open research item.
- **Preemption:** preempting a request requires [[context-snapshot-restore]], whose cost must be
  measured (criterion above).
- **Accounting:** per agent and per human principal, the way cgroups account CPU.

### 3.5 Context as memory
An agent's context window behaves like physical memory: fixed size, with no MMU and no fault
(see [[aios-ostep-mapping]]).

- **Admission control:** when the context working sets of active agents exceed inference
  capacity, admit fewer agents, as OSTEP does for thrashing (ostep p. 282;
  [[working-set-and-thrashing]]).
- **Eviction:** summarization is lossy eviction (aios p. 6). Eviction to a persistent store with
  exact re-load is the paging analogue ([[swapping]]). Which one to use is a policy decision.

### 3.6 Agent state and storage
Agents' long-term memory lives in ordinary files under a per-agent directory. It is subject to
POSIX semantics and the durability rules of [[fsync-and-durability]] and [[atomic-rename]].
AIOS's storage manager has no consistency model (aios pp. 6-7); ours inherits the file system's.

### 3.7 Tools as capabilities
A tool is a named capability: an executable, socket or remote endpoint, plus an argument
schema. The broker holds the real credential (API key, token) and the agent holds only the
handle. Revoking the handle revokes the tool (ostep pp. 733-734).

## 4. Component map (substrate option A, Linux)

```
 user space ──────────────────────────────────────────────────────────────────────
   POSIX apps (unmediated)    agents (deprivileged, agent credential)
                                   │ syscalls        │ agent API (agentctl / libagent)
                                   ▼                 ▼
   ┌──────────────────────── agent broker (TCB, deterministic) ────────────────────┐
   │ capability table │ policy engine │ trusted-path prompt │ audit log │ budgets  │
   └───────┬───────────────────┬──────────────────────┬────────────────────────────┘
           │                   │                      │
   inference scheduler    tool proxies          context store
   (queue, snapshot,      (hold real creds)     (per-agent files,
    accounting)                                  admission control)
 kernel ──────────────────────────────────────────────────────────────────────────
   Linux + LSM/Landlock policy │ seccomp user-notify → broker │ cgroups │ namespaces
```

The substrate choice is argued in [adr/0001-substrate.md](adr/0001-substrate.md).

## 5. POSIX coexistence rules

1. A process without an agent credential sees an unmodified POSIX system. The certification
   run happens in that mode *and* with the broker running.
2. Agent features are additive: new syscalls or ioctls, a `/dev/inference` device, and the
   `agentctl` utility. They never change the semantics of existing interfaces.
3. Mediation denials map to standard errno values.
4. Agent-visible files are ordinary files, so `ls`, `grep` and `make` work for and on agents.

## 6. Threat model (first cut)

| Principal | Trusted? | Notes |
|---|---|---|
| Human user | Yes, for their own resources | The source of authority for everything an agent does |
| Broker + Linux kernel | Yes (TCB) | Keep the broker small ([[trusted-computing-base]], economy of mechanism, ostep pp. 699-700) |
| Model (local or remote) | **No** | Its output is a function of untrusted input |
| Agent | No | It can be steered by its own inputs |
| Content (web, email, files from others) | No | The prompt-injection vector |
| Tools / remote APIs | Partially | Scoped by capability; real credentials stay in the broker |

**Primary threat: confused deputy.** Untrusted content steers an agent into using the user's
authority. OSTEP's version is attackers fooling a privileged user, where the damage scales with
that user's privilege (ostep p. 702; [[setuid-and-privilege-escalation]]). The mitigations are:
- least privilege per agent ([[principle-of-least-privilege]])
- capability subsetting for sub-agents
- trusted-path escalation
- full audit

Out of scope for v1: side channels between agents sharing an inference device. Note that this
belongs in v2, because batched inference does share hardware state.

## 7. Milestones

Each milestone needs a date, which comes from Corey (section 9).

| | Milestone | Done when | Shows it was the wrong path if |
|---|---|---|---|
| M0 | Knowledge base | Wiki + this doc (done) | n/a |
| M1 | Mediation spike on Linux | Broker with seccomp-unotify + Landlock mediates file/net/exec for one agent; overhead measured | A required rule cannot be enforced, or overhead exceeds target. Revisit ADR 0001. |
| M2 | Pre-certification baseline | Open POSIX Test Suite (LTP) + free VSX/VSC-PCTS2016 OSS licence run on the base image with the broker on | Broker changes test outcomes for non-agent processes |
| M3 | Inference device | Scheduler + snapshot/restore with measured cost; beats the AIOS FIFO baseline on our hardware | Snapshot cost makes preemption a net loss |
| M4 | Multi-agent + delegation | Sub-agent capability subsetting, budgets, trusted-path escalation, red-team suite passes | Red team finds a mediation bypass by design |
| M5 | Certification | Formal 1003.1-2016 Base run, product registered | n/a |

## 8. Prior art to ingest next

These sources are not in the wiki yet; each would be its own ingest:
- Later AIOS versions
- vLLM / PagedAttention (KV-cache paging is the real "context as memory" mechanism)
- Capsicum
- seL4
- Fuchsia handles
- Landlock and seccomp-unotify documentation
- Prompt-injection literature: indirect injection, CaMeL-style capability separation
- The POSIX.1-2017 Base product standard itself

## 9. Open questions for Corey

1. **Deadline and budget.** When should M1 and M5 land, and with how many people? Without these
   numbers the milestones have no schedule.
2. **Baseline to beat.** Is it AIOS on Linux, or "Claude Code in a container with an allowlist"?
   That choice sets the success thresholds in section 2.
3. **Target hardware and model.** A local GPU or a remote API changes what the inference device
   schedules and what the snapshot contains.
4. **Is "kernel-enforced" acceptable as the meaning of "native"**, or must agents be kernel
   objects (option B/C in ADR 0001)? This decides the substrate.
5. **Product or research artifact?** Certification has a cost only if the build is a product.
   For a research artifact, PSE52 or "passes the suites without registering" is far cheaper.
