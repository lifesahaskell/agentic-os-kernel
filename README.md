# agentic-os-kernel

Research toward an operating system where LLM agents are things the kernel itself knows about:
principals with an identity, a set of capabilities, and a budget, that the OS schedules, accounts
for, and constrains. The OS should also run ordinary POSIX software unchanged.

This is early-stage research, not a usable OS. What exists today:

- a **knowledge base** built from two sources, the AIOS paper and *Operating Systems: Three Easy
  Pieces* (OSTEP);
- **architecture documents** that scope the system and argue the main decisions;
- a **kernel spike**: about 1000 lines of Rust that prove out the core primitives in QEMU.

## The idea in one paragraph

On today's systems an "agent" is an application that calls a model, and any sandboxing around it
is advisory. Here the protection boundary knows what an agent is. An agent is a kernel object.
Its authority is exactly the set of capability handles it holds, and the kernel stamps its
identity on every message it sends. Most checks therefore happen once, when a capability is
granted, instead of on every system call. POSIX is provided on top by a libc and userspace
servers, as in Redox, MINIX 3, and Hurd. The goal is to *adhere* to POSIX, measured with free
test suites. Formal certification is not a goal.

## Where things stand

| Milestone | State |
|---|---|
| M0. Knowledge base and scoping | Done |
| B1. Capability broker prototype on Linux | Not started |
| B2. Kernel spike on Asterinas OSTD | Done 2026-10-23: 17/17 tests and the boot-time CPU budget check pass. [Report](docs/architecture/b2-spike-report.md) |
| B3. POSIX core (process manager, VFS, exec, libc, BusyBox shell) | Next |
| B4. Agents running POSIX tools under a grant | Planned |
| B5. Chosen POSIX adherence tier | Planned |

These milestones have no dates or team size yet. Those are open questions in
[scoping.md section 9](docs/architecture/scoping.md#9-open-questions-for-corey).

## Reading order

1. [Scoping](docs/architecture/scoping.md) covers what is being built, the goals and non-goals,
   the threat model, and the milestones.
2. [ADR 0002](docs/architecture/adr/0002-rust-microkernel.md) explains why the system is a new
   Rust capability microkernel and not Linux plus an agent layer.
   [ADR 0001](docs/architecture/adr/0001-substrate.md) is the superseded Linux argument, kept for
   the record.
3. [Option B design](docs/architecture/option-b-microkernel.md) sets out what goes in the kernel,
   how revocation works, how POSIX meets capabilities, and the adherence bar.
4. The [B2 spike report](docs/architecture/b2-spike-report.md) records what was built, what was
   verified and what was not, and what would reverse the decision to build on OSTD.

Background:

- [POSIX certification](docs/architecture/posix-certification.md) covers what certification would
  cost. It is kept for reference only.
- [Microkernel POSIX personalities](docs/architecture/research/microkernel-posix.md) surveys how
  other microkernels provide a POSIX layer.
- [Rust kernels and POSIX measurement](docs/architecture/research/rust-kernels-and-adherence.md)
  covers Rust kernels and how to measure POSIX adherence.

## Repository map

| Path | What it is |
|---|---|
| `docs/architecture/` | Scoping documents, ADRs, research notes, and the spike report |
| `wiki/` | The knowledge base: about 90 concept pages linked with `[[wikilinks]]`. Start at [`wiki/index.md`](wiki/index.md) |
| `kernel/` | The B2 spike, a Rust kernel crate on [Asterinas OSTD](https://github.com/asterinas/asterinas) 0.19.0 |
| `tools/wikigraph.py` | Builds `wiki/graph.json` from the wiki links and reports broken links |
| `raw/` | Source PDFs and their extracted text. Not in git, because the files are copyrighted |

## Running the kernel spike

You need Docker and a Linux host with KVM (`/dev/kvm`). The toolchain, QEMU, and firmware all run
inside a container.

```bash
docker build -t agentic-os-kernel-dev:0.19.0 kernel
```

Then run each of these from `kernel/`:

```bash
./osdk.sh cargo osdk test
```

```bash
./osdk.sh cargo osdk run
```

`test` runs the 17 kernel tests inside QEMU. `run` boots the kernel with its CPU-budget
scheduler, runs two agents with different CPU quotas, prints how many ticks each used, and
powers off with exit 0 only if both stayed within quota and in their own address space.
`osdk.sh` gives the container `/dev/kvm` and nothing broader, and it never uses
`--privileged`.

The spike covers eight things:

- capability spaces, with attenuating derive and per-grant revocation;
- agent objects, whose memory budget is carved out of the parent's;
- IPC that is rights-checked and stamped with the sender's credential;
- interrupting a thread, which aborts a blocked receive, and redirecting it, which is the kernel
  half of POSIX signals;
- copy-on-write address-space cloning, where the agent that faults pays for the copied page;
- a CPU budget per agent, carved from the parent's and enforced by a custom scheduler;
- switching to each task's own address space on every context switch;
- booting in QEMU on x86-64.

The spike does not cover SMP, a real system call surface, or teardown. The spike
report lists each gap.

## Working with the wiki

The wiki can be opened as an [Obsidian](https://obsidian.md) vault to browse the graph. To
rebuild the graph and check for broken links:

```bash
python3 -I tools/wikigraph.py wiki
```

The command exits non-zero if any link is broken.

Every factual claim on a wiki page cites a PDF page, for example `(ostep p. 123)`. Design
reasoning that no source states is marked as interpretation.

Pages are maintained by an LLM, following the conventions and the ingest workflow in
[`AGENTS.md`](AGENTS.md). To add a source, put the PDF in `raw/` and follow that workflow.

On a fresh clone `raw/` is empty, so citations cannot be checked until you supply the PDFs
yourself:

- AIOS: [arXiv 2403.16971 v2](https://arxiv.org/abs/2403.16971v2);
- OSTEP: [pages.cs.wisc.edu/~remzi/OSTEP](https://pages.cs.wisc.edu/~remzi/OSTEP/).

## Open questions

These come from the scoping document:

- What are the deadline and the team size?
- Which baseline should this beat: AIOS on Linux, or an agent in a container with an allowlist?
- Will the model run on local hardware or behind a remote API?
- Is this a product or a research artifact?
