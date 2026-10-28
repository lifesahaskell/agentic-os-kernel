# Phase 8: Reference environment, early falsifiers and threshold gate - Pattern Map

**Mapped:** 2026-10-28
**Files analyzed:** 14 new or modified
**Analogs found:** 9 exact or role-match, 3 partial, 2 none (of 14)
**Tracked-source gate:** every analog below was listed by `git ls-files` (tools/, docs/architecture/, scripts/). No gitignored mirror paths appear.

## Corrections to the expected file list

1. RESEARCH.md "Recommended Project Structure" puts BPF sources and the root wrapper in a new `tools/bpflsm/` directory (`bpflsm.sh`, `bpflsm.bpf.c`, `bpflsm_cg.bpf.c`, `bpfadv.bpf.c`). Names are Claude's discretion (CONTEXT). Use `tools/bpflsm/`: it keeps the `tools/linux-baseline/Makefile` header "gcc and glibc only" true (D-07).
2. Add `tools/aok-adv/.gitignore` (or one line in root `.gitignore`) for `target/`. Root `.gitignore` only has `userland/target/`; `kernel/.gitignore` has `/target`; `tools/linux-baseline/.gitignore` has `bin/`, `.work/`, `results/smoke-*/`. A new crate without one commits `target/`.
3. `check_report.py` is NOT reusable for the Phase 8 report. It hard-codes the 12 headings of linux-baseline.md (`HEADINGS`, lines 15-28) and the `STATUS` string. A Phase 8 report needs its own heading list or no script gate. RESEARCH "Validation Architecture" cites `check_report.py <run dir>` as if it checked a run dir; it takes `<report.md> <run-dir>` (usage line 4). The run-dir gate that exists is `summarize.py --check RUN_DIR` (summarize.py:533-541).
4. The repo's self-test flag spelling is `--self-test` (`tools/unsafe_check.py`, `tools/pin_check.sh`). `summarize.py` uses `--check`. The brief and RESEARCH say `r1.py --selftest`. Pick `--self-test` to match, or accept both.
5. `scripts/check.sh` (and `.github/workflows/check.yml`, which just calls it) does not run anything under `tools/linux-baseline/`. Adding `tools/aok-adv` and `r1.py` gates there is a choice, not a fix. CI is `ubuntu-latest` in Docker with `--no-kvm` fallback: no root, no `bpf` LSM. Only `cargo test` and `r1.py --self-test` could ever go in it.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `tools/linux-baseline/run.sh` (write_env keys) | config / driver | batch | itself, `write_env` lines 98-146 | exact (extend) |
| `tools/bpflsm/bpflsm.sh` (root wrapper) | script (privileged setup + launcher) | batch | `tools/linux-baseline/run.sh` | partial (same style; no root analog) |
| `tools/bpflsm/*.bpf.c` | BPF program | event-driven (hook) | `docs/architecture/research/linux-baseline.md` section 8 sketch (lines 424-470) | partial (doc sketch, not tracked source) |
| build for the BPF objects | build | transform | `tools/linux-baseline/Makefile` | partial |
| `tools/linux-baseline/r1.py` | utility | transform | `tools/linux-baseline/summarize.py` + `tools/unsafe_check.py` (self-test) | exact |
| `tools/aok-adv/` Rust crate | test tooling | request-response | `tools/linux-baseline/broker.c` + `cg.c` (behaviour), `tools/unsafe_check.py` rules (unsafe discipline) | none for Rust; role-match in C |
| `tools/aok-adv/src/stub.rs` (stub broker) | service | request-response | `tools/linux-baseline/broker.c` `broker_ident` lines 158-186 | role-match |
| `tools/aok-adv/src/sys.rs` | utility (unsafe confinement) | syscall wrappers | `tools/linux-baseline/uapi_compat.h` + `cg.c` | role-match |
| adversarial cases / report JSON | test | request-response | none | no analog |
| `.planning/phases/08-.../08-GATE.md` | doc (decision sheet) | n/a | scoping-linux.md section 4 table + linux-baseline.md section 11 table | partial |
| `docs/architecture/scoping-linux.md` sec 3 / sec 4 Status cells | doc edit | n/a | itself | exact |
| `docs/architecture/adr/0004-linux-prototype-first.md` status line | doc edit | n/a | itself (line 3) | exact |
| run report under `docs/architecture/research/` | doc | n/a | `docs/architecture/research/linux-baseline.md` | exact |
| `.gitignore` for `tools/aok-adv/target` | config | n/a | `tools/linux-baseline/.gitignore` | exact |

## Pattern Assignments

### `tools/linux-baseline/run.sh` (write_env extension)

**Analog:** itself. Read of the whole file done; line numbers are exact.

**Header and strict mode** (lines 1-13): shebang `#!/usr/bin/env bash`, comment block with usage, `set -euo pipefail`, then `cd "$(dirname "${BASH_SOURCE[0]}")/../.."`. Tabs for indentation throughout.

**kv helper and writer** (lines 84, 98-146):
```bash
kv() { printf '%s=%s\n' "$1" "$2" >>"$RUNDIR/env.txt"; }
...
	kv uname_r "$(uname -r)"
	...
	if [ -e /dev/kvm ]; then kv kvm_dev present; else kv kvm_dev absent; fi
	kv clocksource "$(cat /sys/devices/system/clocksource/clocksource0/current_clocksource 2>/dev/null || echo unreadable)"
	if [ -r /proc/config.gz ]; then
		kv config_lsm "$(zcat /proc/config.gz | grep '^CONFIG_LSM=' || echo absent)"
	else
		kv config_lsm unreadable
	fi
	"$BIN/micro" --env >>"$RUNDIR/env.txt"
```
Conventions to copy for new keys (F13 list in RESEARCH):
- `key=value`, snake_case keys, one per line, no quoting, no spaces around `=`.
- Present/absent as the words `present`/`absent`; unreadable as `unreadable`; yes/no as `yes`/`no`. Fallback `|| echo unreadable` after every probe so `set -e` does not abort on a missing file.
- Floor keys: `floor_<row>=present|absent`, one per scoping-linux.md section 3 row (12 rows, listed in section 3 table).
- `write_env` runs only when `env.txt` is empty (lines 148-150), so a second invocation on the same `--run-id` does not rewrite it.
- `landlock_abi`, `so_peerpidfd`, `pidfd_get_info`, `active_lsms`, `timer_res_ns` are appended by `"$BIN/micro" --env` (line 145), not by bash. New keys that need a syscall (Landlock per-feature probes) would go in `micro.c`, which D-07 says stays unchanged: do them in bash or in the wrapper.

**Do not copy / hazards:**
- `write_env` runs as the unprivileged test uid. Keys that need root (`bpftool_version`, `attach_*`, `lsm_hook_fires`, counter reads) cannot be written by `run.sh`. Either the wrapper appends them with the same `printf '%s=%s\n'` form before calling `run.sh` (then `env.txt` is non-empty and `write_env` is skipped entirely, so the wrapper must also write the existing keys), or `run.sh` writes the unprivileged keys and the wrapper appends the rest after. This ordering decision is the planner's. The second route is simpler: wrapper appends after `run.sh` returns.
- `harness_dirty` (lines 102-106) will read `yes` if the wrapper dir is under `tools/linux-baseline/`; another reason to keep `tools/bpflsm/` separate (git status is limited to `$TOOLS`).
- `micro.c` and `sandbox.c` stay unchanged (D-07). Only `run.sh` changes.

**`--tag` semantics** (lines 60-65, 169-172, 199-201): valid with `--suites micro` only, exit 64 otherwise. Tag turns config `C3` into label `C3-<tag>` and passes `--label`. Tag charset is not validated beyond the run-id case (line 57-59 applies to RUN_ID only); use `[A-Za-z0-9._-]` in tag names (`bpflsm-file_open-miss`).
**Exit codes:** 64 usage, 1 failure, 3 duplicate (round, label) rows (lines 203-206).
**Two-invocation constraint (RESEARCH F11):** `baseline_of("micro", c)` in `summarize.py:176-178` returns `None if config == "C0" else "C0"`, so every tagged label is compared with untagged C0 in the same run dir. Run C0/C3 untagged in invocation 1, tagged C3 in invocation 2 with the same `--run-id`. Untagged C3 with programs loaded is not clean C3; the wrapper skeleton in RESEARCH flags this as Open Question 6.
**Run layout:** `tools/linux-baseline/results/<RUN_ID>/{env.txt,run.log,micro.csv,selfcheck.txt,summary.csv,summary.md,cgroup-probe.txt,...}`; `smoke-*` dirs are recreated and gitignored. Run ids in the repo: `20261028T034700Z` (UTC timestamp, `date -u +%Y%m%dT%H%M%SZ` shape). Reuse that shape for the HARN-01 id; use a different suffix or a second dir for tagged runs only if the C0 baseline is in the same dir.

**`run_pinned` must NOT be copied into the wrapper** (lines 287-294):
```bash
run_pinned() {
	if [ "$DELEG" = yes ]; then
		systemd-run --user --wait --pipe --collect --quiet --same-dir -p Delegate=yes -- \
			taskset -c "$CPU" "$@"
	else
		taskset -c "$CPU" "$@"
	fi
}
```
It moves the process into a transient `user@UID.service` cgroup, out of any cgroup the wrapper created. Only broker/deadline/cgroup suites use it. `run_micro` (lines 208-211) calls plain `taskset -c "$CPU" "$BIN/micro"`, which inherits the wrapper's cgroup: this is what makes the per-cgroup variant work (RESEARCH Pattern 3). `systemd-run --user` also needs the user's login session and D-Bus, which a root wrapper does not have (F14): HARN-01 for the three non-micro suites is launched from the user's own login shell.
**`cd` behaviour:** `run.sh` does `cd` to repo root itself (line 11), so the wrapper can call it by any path. It uses `git rev-parse HEAD` (line 101), so the wrapper must not run it from a non-git copy.

---

### `tools/bpflsm/bpflsm.sh` (root wrapper)

**Analog:** `tools/linux-baseline/run.sh` for style. No tracked root-privileged script exists; `tools/pin_check.sh` and `scripts/check.sh` are the only other shell files.

**Copy:**
- Shebang, comment-block usage lines printed by `usage()` via `sed -n '2,8p' "$0" >&2; exit 64` (run.sh lines 23-26).
- `set -euo pipefail` (run.sh:9). `pin_check.sh` and `check.sh` use `#!/bin/sh` and `set -eu`; the wrapper needs bash arrays and `local`, so match `run.sh`.
- `while [ $# -gt 0 ]; do case "$1" in --flag) VAR="$2"; shift 2 ;; *) usage ;; esac; done` (run.sh:28-39).
- `[ -n "$X" ] || usage`; `log()` writing through `tee -a "$LOG"` to stderr (run.sh:82); `trap cleanup EXIT` (run.sh:93) for detach and sysctl restore (RESEARCH Security: restore `ptrace_scope`/governor in a trap).
- `kv` helper reuse for wrapper-written env keys (same `printf '%s=%s\n'`).
- Exit code conventions 64/1/3.

**Do not copy:** `systemd-run`, `probe_cgroup` (lines 220-283; user-owned delegated dirs under `user.slice`). The wrapper creates root-owned cgroups under `/sys/fs/cgroup/aok-phase8/{timed,sibling,probe}` (RESEARCH F8) and does `echo $$ > .../cgroup.procs`, then `setpriv --reuid ... --regid ... --init-groups` (no `--no-new-privs`; baseline `env.txt` has `shell_nonewprivs=0`).
**Skeleton to follow:** RESEARCH "Wrapper skeleton" (lines 587-598) and the architecture diagram (steps 1-10). Corrections already in RESEARCH: `prog loadall ... pinmaps`, not `prog load`; a deny probe keyed on a separate probe cgroup id.
**Verification form:** `bash -n` on the wrapper is the stated per-commit check (RESEARCH Sampling Rate).

---

### `tools/bpflsm/bpflsm.bpf.c`, `bpflsm_cg.bpf.c`, `bpfadv.bpf.c`

**Analog:** none tracked. Nearest: the sketches inside `docs/architecture/research/linux-baseline.md` section 8 (lines 424-470), plus RESEARCH Code Examples (lines 550-585). Prose, not compiled source.

**Excerpt to start from** (linux-baseline.md lines 424-456):
```c
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
char LICENSE[] SEC("license") = "GPL";
struct { __uint(type, BPF_MAP_TYPE_HASH); __uint(max_entries, 1024);
         __type(key, __u64); __type(value, __u32); } cg_policy SEC(".maps");
static __always_inline int check(void) { ... bpf_get_current_cgroup_id() ... return 0; }
SEC("lsm/file_open")
int BPF_PROG(h_file_open, struct file *file) { return check(); }
```
Extensions the sketch lacks (RESEARCH F7, F9, Pattern 4): per-CPU array `hook_count` with plain add; act on the lookup value (hit variant returns -13, i.e. `-EACCES`); `socket_sendmsg` program; `lsm_cgroup` variant returns 1 allow / 0 deny with `bpf_set_retval(-13)` first if EACCES is wanted; non-sleepable `SEC("lsm/...")` only; no more than 10 distinct `lsm_cgroup` hooks.
**Style:** kernel tab indent (the sketches use tabs), `char LICENSE[] SEC("license") = "GPL";`. Project convention otherwise: match the file. There is no `.clang-format` in the repo to check against.
**Must NOT:** deny keyed on the timed cgroup (deny keys on the probe cgroup id only); `bpf_probe_read_kernel` in the ADV-03 programs (RESEARCH F10, A8); `lsm.s/` sleepable sections.

---

### Build for BPF objects

**Analog:** `tools/linux-baseline/Makefile` (26 lines). How it builds today:
```make
CC      = gcc
CFLAGS  = -O2 -g -std=gnu11 -Wall -Wextra -Werror -D_GNU_SOURCE
BIN     = bin
all: $(BIN)/micro $(BIN)/broker $(BIN)/deadline $(BIN)/cgbench
$(BIN):
	mkdir -p $(BIN)
$(BIN)/micro: micro.c sandbox.c sandbox.h common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ micro.c sandbox.c
...
clean:
	rm -rf $(BIN)
.PHONY: all clean
```
Header comment (line 1): "Linux baseline benchmark harness. gcc and glibc only, no other library." Output goes to `tools/linux-baseline/bin/` (gitignored by `tools/linux-baseline/.gitignore`). `run.sh` checks `[ -x "$BIN/$b" ]` for micro, broker, deadline, cgbench only (line 67), so adding anything to the Makefile that run.sh does not need is safe, but D-07 says do not.

**Copy:** per-target `| $(BIN)` order-only dir, explicit prerequisite lists, `.PHONY: all clean`, `-Werror`. **Do not** add clang/BPF targets to this Makefile. Either a second Makefile in `tools/bpflsm/` or a build step in the wrapper (`bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h; clang -O2 -g -target bpf -c X.bpf.c -o X.bpf.o`, exact commands at linux-baseline.md lines 396-401). `vmlinux.h` is generated per host and must be gitignored.

---

### `tools/linux-baseline/r1.py`

**Analog:** `tools/linux-baseline/summarize.py` (550 lines) for CSV reading and layout; `tools/unsafe_check.py` for the `--self-test` pattern.

**Conventions to copy:**
- Shebang `#!/usr/bin/env python3`, module docstring with `Run with: python3 -I tools/linux-baseline/r1.py ...` and `Check:` lines (summarize.py:1-14), stdlib only, `import csv, os, sys`.
- `main(argv)` returning an int, printing `__doc__` and returning 64 on bad usage, `sys.exit(main(sys.argv))` (summarize.py:533-550). Manual argv parsing, no argparse.
- Check output tokens: `CHECK-OK` / `CHECK-FAIL: ...` (summarize.py:535-541); `REPORT-OK` / `REPORT-FAIL: ...` (check_report.py:32-34). A self-test should print one such token and exit 1 on mismatch.
- Self-test: `unsafe_check.py --self-test` (line 3, `python3 -I tools/unsafe_check.py --self-test`) and `pin_check.sh --self-test`. Wired into `scripts/check.sh` lines 10-17. If `r1.py --self-test` is added to check.sh, it needs the committed run dir `tools/linux-baseline/results/20261028T034700Z/summary.csv`, which is tracked.
- Eligibility vocabulary from `summarize.py` docstring: stable below 10 percent spread, unstable 10 to 25, noise-dominated above 25; `within_noise`.

**Input format** (`summary.csv` header, verified by `head -3`):
`suite,config,bench,rounds,n_total,min_ns,med_p50_ns,spread_p50_pct,med_p90_ns,med_p99_ns,spread_p99_pct,med_p999_ns,max_ns,baseline,added_p50_ns,added_p50_pct,added_p99_ns,added_p99_pct,within_noise,stability,errors`
Rows: `micro,C0,getppid,5,5000000,110.000,130.000,0.00,...`. Empty cells for C0 added columns. Tagged configs appear as `C3-<tag>` (run.sh:171). The CSV only has the spread of the config row and the spread of the baseline row via separate rows; R1 needs both (baseline = C0 row of the same bench).

**Target numbers for the self-test** (linux-baseline.md lines 559-567, section 11):
`openat_tmpfs` p50: `25.01 + 2 x 9.64 = 44.29`, rounded up to 45 (spreads 9.64 / 0.45). `openat_rootfs` p50: `17.48 + 2 x 0.71 = 18.90` -> 19. `unix_connect` p50 keep 10 (4.16), p99 keep 25 (9.87). `unix_dgram_pingpong` p50 14% (0.31 + 2 x 6.61 = 13.53). Rows with spread >= 10 on the constrained quantile print `none: unstable on this host`. The self-test must match every R1 figure in that table, not only the worked example.
**Rule text to implement verbatim** (lines 551-557): revised = `max(current bound, C3 added% + 2 x max(spread_config, spread_baseline))`, rounded up to a whole percent, p50 and p99 separately; ineligible if either spread >= 10 on that quantile. Note: the spread printed in section 11 as "9.64 / 0.45" is (config row / baseline row) spread of the same quantile.
**Do not copy** `summarize.py` internals wholesale; it is 550 lines of aggregation that already exists. `r1.py` reads `summary.csv` only.

---

### `tools/aok-adv/` (Rust crate: suite plus stub broker)

**Analog:** none for a Rust crate under `tools/`. Nearest partials:
- Unsafe discipline: `tools/unsafe_check.py` rules (a)-(e) and `docs/architecture/unsafe-ledger.md` describe what the kernel/userland trees must do. `rs_files` roots (lines 100-102): `kernel/src/**/*.rs` and `userland/*/src/**/*.rs`. `crate_roots` (lines 105-108): `kernel/src/lib.rs`, `userland/*/src/lib.rs`, `userland/tests/src/bin/*.rs`. `tools/aok-adv/**` is outside all of them by construction, so D-13 needs no change to `unsafe_check.py` and no ledger entry. Verified by reading both functions; the self-test run was not repeated. Rule (e) would fire if the crate were ever moved under `userland/`.
- Toolchain pinning style: `kernel/rust-toolchain.toml` and `tools/pin_check.sh` lines 12-30 (`toolchain='channel = "nightly-2026-07-21"'`). The aok-adv crate pins stable explicitly with its own `rust-toolchain.toml` (RESEARCH F15). `pin_check.sh` checks the kernel pins only; it needs no change unless the user wants the suite pinned there too.
- Layout/behaviour reference for cgroup handling: `tools/linux-baseline/cg.c` and `cg.h`.

**Crate conventions (RESEARCH F15, verbatim intent):**
```rust
#![deny(unsafe_code)]            // crate root
#[allow(unsafe_code)]
mod sys { /* every unsafe block preceded by // SAFETY: <reason> */ }
```
`Cargo.toml`: `libc = "=0.2.191"` (exact pin; matches the `=0.19.0` exact-pin style in `kernel/Cargo.toml` per `pin_check.sh:11`). Hand-define in `sys`: `CLONE_INTO_CGROUP: u64 = 0x2_0000_0000` (libc types it `c_int`), the 72-byte `PidfdInfo`, `PIDFD_GET_INFO` ioctl number, Landlock structs, io_uring structs.

**Single source of truth for the wire layouts** is the C header the baseline already validated at runtime. Copy field order and constants from `tools/linux-baseline/uapi_compat.h:70-98`:
```c
#define AOK_PIDFD_INFO_CGROUPID (1ULL << 2)
struct aok_pidfd_info { __u64 mask; __u64 cgroupid; __u32 pid; __u32 tgid; __u32 ppid;
	__u32 ruid; __u32 rgid; __u32 euid; __u32 egid; __u32 suid; __u32 sgid;
	__u32 fsuid; __u32 fsgid; __s32 exit_code; __u32 coredump_mask; __u32 __spare1; };
#define AOK_PIDFS_IOCTL_MAGIC 0xFF
#define AOK_PIDFD_GET_INFO _IOWR(AOK_PIDFS_IOCTL_MAGIC, 11, struct aok_pidfd_info)
#define AOK_SO_PEERPIDFD 77
```
Match the 72-byte layout: the ioctl number encodes the size (RESEARCH F15). A smaller libc struct yields a different ioctl number.

**No existing test conventions to copy for `cargo test`.** The kernel crate's Rust tests are ktests under OSDK, which do not apply.
**Formatting:** `cargo fmt --check` / `cargo clippy -- -D warnings` is what `scripts/check.sh` runs for `userland/` (line 22); mirror that for the crate by hand, not by editing `check.sh`, unless the user adds it.

---

### `tools/aok-adv/src/stub.rs` (stub broker)

**Analog:** `tools/linux-baseline/broker.c` `broker_ident` (lines 158-186). Same mechanism the stub mirrors (D-15).

**Core identity pattern** (broker.c:158-186, abbreviated to the load-bearing calls):
```c
struct ucred uc; socklen_t l = sizeof uc;
getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &uc, &l);          // uid, pid
int pidfd = -1; l = sizeof pidfd;
getsockopt(fd, SOL_SOCKET, AOK_SO_PEERPIDFD, &pidfd, &l);
struct aok_pidfd_info info; memset(&info, 0, sizeof info);
info.mask = AOK_PIDFD_INFO_CGROUPID;
int rc = ioctl(pidfd, AOK_PIDFD_GET_INFO, &info);
close(pidfd);
id->cgid = info.cgroupid; *info_pid = info.pid;
```
Per-request read of the cgroup id (RESEARCH F16: identity is read at request time, not connect time). Probe/selfcheck for availability is at broker.c:102-125 (`g_have_pidfd_info`).
**SCM_RIGHTS:** `send_fd` at broker.c:189-204 shows the cmsg construction; the Rust version needs `libc::CMSG_*` helpers (present in libc per F15).
**Wire framing:** the C broker uses a fixed `struct msg` over `SOCK_SEQPACKET`. Phase 10's real broker is a different language/decision (D-14); the suite talks to the broker by pid and socket path only. Keep length-prefix and bounds checks (RESEARCH Security V5).
**Do not copy:** broker.c's benchmark loop, variant table (`direct`/`broker_noid`/...), or timing code. The stub is a correctness oracle, not a benchmark.

---

### `tools/aok-adv/src/sys.rs` and cgroup moves

**Analog:** `tools/linux-baseline/cg.c` and `cg.h`. API shape to reimplement (D-13 / CONTEXT "Reusable Assets"):
- `cg_self_path` (cg.c:~33-54): read `/proc/self/cgroup`, find `0::`, join with `/sys/fs/cgroup`.
- `cg_setup`, `cg_make_leaf(name, path_out, n)` ("EEXIST is not an error"), `cg_write` (write plus read-back, EIO on mismatch), `cg_put` (write, no read-back, for `cgroup.procs`, `cgroup.kill`), `cg_read`, `cg_key` (cg.h:18-40).
- `cg_clone3_into` for `CLONE_INTO_CGROUP` (D-15 candidate route (iv), RESEARCH F10).
- Error convention: return -1 with errno set (cg.h:7-8); in Rust return `io::Result` and record `raw_os_error()` in every report row (RESEARCH Don't Hand-Roll).
**Hazard (RESEARCH Pitfall 6):** `cg.c` has no guard against writing `cgroup.kill` to a wrong path. The Rust suite must assert the path is under the suite-created cgroup and the pid was spawned and pidfd-opened by the suite before every destructive write; never `kill(0/-1)`.

---

### Adversarial cases and per-case JSON report

**Analog:** none. Nearest partial: the harness CSV output (`micro.csv`, `summary.csv`) and `selfcheck.txt` lines (a "restriction is active before timing" selfcheck, CONTEXT "Established Patterns"). The report row shape is specified in CONTEXT D-16/D-17 and RESEARCH Validation Architecture: one object per case or route with `class`, `oracle`, `baseline_ok`, agent result, errno, deny counter delta, `closable_by` (empty). Route tables: RESEARCH F10 (includes candidate route (iv), `CLONE_INTO_CGROUP`, which D-16 omits and RESEARCH recommends adding).

---

### `08-GATE.md`

**Analog:** none for a decision sheet. Partial analogs, both tracked:
- `docs/architecture/scoping-linux.md` section 4 table (lines 169-185). Columns: `ID | Criterion (goal) | Measure | Proposed threshold | Baseline | Source/basis | Status`. One row per S1..S9; Status cell is currently the literal `proposed, awaiting user acceptance`.
- `docs/architecture/research/linux-baseline.md` section 11 table (lines 568+): `ID | Leg | Current proposed threshold | Measured here | Rule | Revised proposal | Status`, with the same Status literal. Use this for the evidence-in-hand column.
**Block shape (D-20):** measure, proposed threshold, baseline, evidence in hand (with run directory named), recommendation, user mark. One block each for S1-S9, POSX-02 (not an S row; text is REQUIREMENTS.md line 160), and DEC-03 (D-21). S7 and S8 blocks pre-fill "deferred to Phase 14" (D-19). DEC-02 block names option 1 and the run dirs it rests on (D-18; REQUIREMENTS DEC-02 Test clause lines ~85-91).
**Heading style to follow:** numbered `## N.` headings in docs; phase artifacts under `.planning/phases/08-.../` use `08-CONTEXT.md` / `08-RESEARCH.md` naming and a `# Phase 8: ... - X` title with a bold date/status line.
**Machine-checkable marks:** RESEARCH asks for `grep`-checkable Status cells (DEC-01 row). Put the user's mark on a line of fixed shape, e.g. `Mark: accepted | changed <value> | rejected | deferred <phase>`, so the transcription step can be verified by script.

---

### `docs/architecture/scoping-linux.md` Status cells and section 3 labels

**Analog:** itself.
**Status cell today** (section 4, lines 177-185): last column literal `proposed, awaiting user acceptance`. Format for the edit per D-20: dated; one of accepted / changed with value / rejected / deferred with phase. Keep the cell in the same pipe-table row; do not add columns. S7 and S8 become "deferred to Phase 14, <date>".
**Section 3 "proposed" labels** (to update from the DEC-03 marks): `Floor tiers (proposed, awaiting user acceptance):` (line 130) and `Reachability rule (proposed, awaiting user acceptance):` (line 150). Lines 130-168 hold the tiers, the reachability rule, and per-distro verdicts (e.g. line 160 "Debian 13: reachable; the deciding row is Landlock ABI, exactly at the proposed minimum of 6").
**Evidence-label style to preserve:** VERIFIED, TO-VERIFY (in section 3), ASSUMED, NOT MEASURED, *(interpretation)*. Table cells state value then label: `GA present; HWE present VERIFIED`. The Ubuntu HWE row for BPF_LSM_CGROUP and `bpf` in default LSM order will gain a reference-host observation; cite the Phase 8 run dir and the `env.txt` keys, do not overwrite the VERIFIED distro-kernels input value.
**Also section 4 preamble line 171:** "Every threshold below was proposed by the author of this document. None is accepted." This sentence goes stale after the gate; edit it in the same transcription step.

### `docs/architecture/adr/0004-linux-prototype-first.md` status line

**Analog:** itself, line 3:
`- Status: **Proposed** (direction chosen by Corey 2026-10-27; it moves past Proposed only when the user accepts the success-criteria thresholds in scoping-linux.md section 4, which are unset)`
D-19/DEC-01: stays Proposed while S7 and S8 are deferred; the edit updates the parenthetical to say which rows are decided and which are deferred, with the date. Line 4 `- Date: 2026-10-27` is the document date; leave it. The header uses `- Key: value` bullets; keep that shape.

---

### Run report under `docs/architecture/research/`

**Analog:** `docs/architecture/research/linux-baseline.md` (exact for tone and structure).
**Conventions to copy:**
- Title `# <Topic>: <claim>`; second line `Status: draft 1, 2026-10-28 (run date, UTC).`; opening paragraph names which scoping-linux.md rows it grounds, links the committed run directory by relative path (`../../../tools/linux-baseline/results/<id>/summary.md`), and says "Nothing here accepts or changes a threshold".
- Numbered `## N. Title` sections: Verdict, Host, Method, results, Not measured, Verified and assumed, Caveats, Proposed revisions, `## Sources`. Mirror this order (the 12 headings are at check_report.py:15-28).
- Verdict leads with a "Partially stable" style finding, then "One-line answers" bullets per S-row (lines 14-42).
- Numbers carry their unit and precision exactly as the CSV prints them (`19116.000 ns`, `25.01 percent`). Write "percent" in prose, `%` appears in tables.
- Every number is tied to a run directory; ASSUMED items are named in an explicit list (section 9).
- Style: no emoji, no exclamation points.
**For this phase the report must add:** HARN-01 side-by-side table (C3-vs-C0 added percent and broker/deadline p99, both hosts, with stability marks), HARN-02/03 rows with N, p50, p99, spreads, stability, counter delta, R1 with the BPF share (informational under D-18), ADV count of successes per route, the counter-increment upward-bias statement (D-09), the `bpf`-in-LSM-list confound note (RESEARCH F12), and the Yama `ptrace_scope` decision.
**Check script:** `check_report.py` cannot be reused as is (see Corrections 3). If a gate is wanted, copy its structure: `HEADINGS` list, `sections()` splitter (lines 38-52), `strip_code()` (lines 33-36), `fail()` printing `REPORT-FAIL: ...`.

---

### `.gitignore` for the crate

**Analog:** `tools/linux-baseline/.gitignore` (contents: `bin/`, `.work/`, `results/smoke-*/`). Add `tools/aok-adv/.gitignore` containing `/target`, matching `kernel/.gitignore` (`/target`). Also gitignore generated `vmlinux.h` and `*.bpf.o` under `tools/bpflsm/`.

## Shared Patterns

### Evidence labels and bound-with-baseline rule
**Source:** CONTEXT.md "Established Patterns"; `docs/architecture/scoping-linux.md` section 4; `docs/architecture/research/linux-baseline.md`.
**Apply to:** the run report, `08-GATE.md`, all Status cell edits.
Labels: VERIFIED, ASSUMED, NOT MEASURED, TO-VERIFY (section 3 only), *(interpretation)*. Each bound comes with a baseline and names the run directory it rests on. Wiki-style page citations do not apply here (no wiki edits).

### Bash script style
**Source:** `tools/linux-baseline/run.sh` lines 1-13, 23-39, 82-95.
**Apply to:** `bpflsm.sh`, any env-probe helper.
Tab indent, `set -euo pipefail`, `usage()` from the header comment via `sed -n '2,8p' "$0"`, `kv`/`log` helpers, `trap cleanup EXIT`, exit 64 for usage errors, `|| echo unreadable` after probes.

### Python utility style
**Source:** `tools/linux-baseline/summarize.py` lines 1-14, 533-550; `tools/unsafe_check.py` lines 1-12; `tools/linux-baseline/check_report.py` lines 1-8, 32-34.
**Apply to:** `r1.py`.
Docstring with usage, stdlib only, run with `python3 -I`, manual argv, `main(argv) -> int`, PASS/FAIL tokens on stdout, `--self-test`.

### Run directory and results layout
**Source:** `run.sh` lines 71-84; `tools/linux-baseline/results/20261028T034700Z/`.
**Apply to:** HARN-01 and HARN-02/03 outputs.
`results/<RUN_ID>/` with `env.txt`, `run.log`, `<suite>.csv`, `selfcheck.txt`, `summary.csv`, `summary.md`. Committed (except `smoke-*`). Wrapper-produced extras (counter before/after, attach log, deny-probe result) belong in the same dir as plain text `key=value` or log files, not in a new tree.

### Kernel-stamped peer identity
**Source:** `tools/linux-baseline/broker.c` lines 158-186; `uapi_compat.h` lines 70-98.
**Apply to:** `stub.rs`; the Phase 10 broker later (ADV-05).

### Unsafe confinement
**Source:** `tools/unsafe_check.py` rules (a)-(e), lines 4-11; `docs/architecture/unsafe-ledger.md`; AGENTS/CLAUDE constraint "`#![deny(unsafe_code)]` stays crate-wide ... confined to small named modules".
**Apply to:** `tools/aok-adv/`. The suite is deliberately outside the gate (D-13): same discipline (one `sys` module, `// SAFETY:` per block), no ledger rows, no `unsafe_check.py` change.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `tools/bpflsm/*.bpf.c` | BPF program | event-driven | No BPF source is tracked; only the doc sketches in linux-baseline.md section 8 |
| `tools/aok-adv/` crate (Cargo.toml, rust-toolchain.toml, src/) | test tooling | request-response | No Rust under `tools/`; the only Rust is `kernel/` and `userland/` (different toolchain, `no_std`, ledgered) |
| adversarial case runner and JSON report | test | request-response | Nothing runs adversarial attempts today |
| `08-GATE.md` | decision doc | n/a | No existing gate sheet; section 4 table and section 11 table are partial format analogs |
| root-privileged wrapper | script | batch | No root script exists; style analog only (`run.sh`) |

## Answers to the specific questions

**How `tools/linux-baseline/Makefile` builds today.** Plain GNU make, gcc, `-O2 -g -std=gnu11 -Wall -Wextra -Werror -D_GNU_SOURCE`, four binaries into `tools/linux-baseline/bin/` (micro from `micro.c sandbox.c`; broker from `broker.c`; deadline from `deadline.c cg.c`; cgbench from `cgbench.c cg.c`), `clean` removes `bin/`. `run.sh` refuses to start unless those four exist. Invoked as `make -C tools/linux-baseline` (run.sh line 2 and line 68). No CI step builds it.

**Where `tools/unsafe_check.py` roots stop.** `rs_files()` globs `kernel/src/**/*.rs` and `userland/*/src/**/*.rs` (lines 100-102). `crate_roots()` adds `kernel/src/lib.rs`, `userland/*/src/lib.rs`, `userland/tests/src/bin/*.rs` (lines 105-108). Anything under `tools/` is outside, including `tools/aok-adv/`. D-13 holds with no edit, provided the crate is not placed under `userland/`.

**Existing files that list tool directories (candidates for a new entry).**
- `scripts/check.sh` lines 10-17: lists `tools/unsafe_check.py` and `tools/pin_check.sh`; no `tools/linux-baseline` entry. Add `r1.py --self-test` and `cargo test` for `tools/aok-adv` only if the user wants them gated; CI has no root or `bpf`.
- `.github/workflows/check.yml`: calls `scripts/check.sh`; lists no tool dirs.
- `AGENTS.md` Layout table: lists `tools/wikigraph.py` only; `tools/linux-baseline` and the new dirs are not documented there. AGENTS.md is project instructions; changing it is out of scope for the pattern map and is flagged here only.
- `.gitignore` (root: `userland/target/`, `bin/`, `.work/`) and `tools/linux-baseline/.gitignore` (`bin/`, `.work/`, `results/smoke-*/`): the new crate and BPF build products need their own entries.
- `tools/pin_check.sh`: pins kernel toolchain and OSTD only; no change unless the suite toolchain is meant to be pinned there.
- `docs/architecture/research/linux-baseline.md` section 8 commands (lines 389-420) name `run.sh --run-id bpflsm-<id> --suites micro --configs C0 --tag bpflsm` and a `bpftool prog load` form. Section 8 step 5 and step 4 are wrong (RESEARCH F7); the new report should supersede, not edit, that section.

## Metadata

**Analog search scope:** `tools/`, `scripts/`, `.github/workflows/`, `docs/architecture/` (scoping-linux.md, adr/0004, research/linux-baseline.md), `kernel/.gitignore`, root `.gitignore`.
**Files read:** `run.sh` (full), `Makefile`, `summarize.py` (head and `main`), `check_report.py` (head), `unsafe_check.py` (head and root functions), `pin_check.sh` (head), `scripts/check.sh` (head), `broker.c` 150-204, `cg.c` 1-60, `cg.h` 1-40, `common.h` 1-40, `uapi_compat.h` 68-100, `env.txt`, `summary.csv` header, linux-baseline.md sections 8 and 11, scoping-linux.md sections 3 and 4, ADR 0004 header.
**Not read:** `micro.c`, `sandbox.c`, `cgbench.c`, `deadline.c` (unchanged by this phase), `unsafe-ledger.md`.
**Not run:** nothing was executed; `python3 -I tools/unsafe_check.py --self-test` was not re-run.
**Pattern extraction date:** 2026-10-28
