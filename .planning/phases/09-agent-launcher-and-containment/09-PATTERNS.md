# Phase 9: Agent launcher and containment - Pattern Map

**Mapped:** 2026-11-05
**Files analyzed:** 14 new/modified
**Analogs found:** 13 / 14 (all analog paths are git-tracked; verified with `git ls-files`)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `tools/aok-launch/Cargo.toml` | config | n/a | `tools/aok-adv/Cargo.toml` | exact |
| `tools/aok-launch/rust-toolchain.toml` | config | n/a | `tools/aok-adv/rust-toolchain.toml` | exact |
| `tools/aok-launch/.gitignore` | config | n/a | `tools/aok-adv/.gitignore` (`/target`) | exact |
| `tools/aok-launch/src/sys.rs` | utility (sole unsafe module) | request-response (raw syscalls) | `tools/aok-adv/src/sys.rs` | exact |
| `tools/aok-launch/src/main.rs` | CLI entry | request-response | `tools/aok-adv/src/main.rs` | role-match |
| `tools/aok-launch/src/lib.rs` | library root | n/a | none in aok-adv (bin only); module layout from RESEARCH | partial |
| `tools/aok-launch/src/cgroup.rs` | service | file-I/O (cgroupfs) | `tools/linux-baseline/cg.c` (lines 105-170) | role-match (C to Rust) |
| `tools/aok-launch/src/landlock.rs` | service | transform | `tools/linux-baseline/sandbox.c` 206-259 | role-match (C to crate) |
| `tools/aok-launch/src/seccomp.rs` | service | transform | `tools/linux-baseline/sandbox.c` 160-182 | role-match |
| `tools/aok-launch/src/identity.rs` | service | request-response + file-I/O | `tools/aok-adv/src/sys.rs` `pidfd_identity`/`peer_identity` (380-430) | role-match |
| `tools/aok-launch/tests/*.rs` | test | request-response (spawns binary) | `tools/aok-adv/tests/topology_and_lifecycle.rs`, `harness_failures.rs` | exact |
| `tools/aok-launch/tests/common/` (delegated-scope fixture) | test util | file-I/O | `tools/aok-adv/tests/*` `fresh_dir` + `cg.c` `cg_kill_and_rmdir` | partial |
| external seccomp shim (D-13) | test wrapper | transform | `tools/linux-baseline/sandbox.c` `build_allowlist` (as BPF builder) | partial |
| `tools/linux-baseline/run.sh` (additive `--wrap`) | script | batch | itself (`--tag` mechanism) | exact |
| `tools/linux-baseline/results/<run-id>/` (depth runs) | results | batch | `results/20261028T034700Z/` | exact |

No true "no analog": a daemon-less registry file has no in-repo precedent (see bottom).

## Pattern Assignments

### `tools/aok-launch/Cargo.toml` and `rust-toolchain.toml` (config)

**Analog:** `tools/aok-adv/Cargo.toml` (whole file, 11 lines) and `rust-toolchain.toml` (2 lines)
```toml
[package]
name = "aok-adv"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "aok-adv"
path = "src/main.rs"

[dependencies]
libc = "=0.2.191"
```
```toml
[toolchain]
channel = "1.99.1"
```
Copy with name `aok-launch`; add a `[lib]` (path `src/lib.rs`) plus the `[[bin]]`; exact-pin (`=`) every dep: `libc = "=0.2.191"`, `landlock = "=0.4.7"`, `seccompiler = "=0.5.0"`. Commit `Cargo.lock` (aok-adv does). `.gitignore` is the single line `/target`. Toolchain pin is checked by `tools/pin_check.sh` for the kernel crate only; aok-adv is not in it, and `tools/unsafe_check.py` does not mention aok-adv (grep: no match), so the new crate is likewise outside it (D-11).

### `tools/aok-launch/src/sys.rs` (the one unsafe module)

**Analog:** `tools/aok-adv/src/sys.rs` (910 lines). All primitives to copy exist here.

**Module header and crate attribute pattern** (`sys.rs:1-12`, `main.rs:1,13`):
```rust
//! The single `unsafe` module in this crate ... Every unsafe block below
//! carries a `// SAFETY:` comment. `main.rs` declares this module
//! `#[allow(unsafe_code)] mod sys;` ...
```
```rust
#![deny(unsafe_code)]          // main.rs:1 (and lib.rs)
#[allow(unsafe_code)]
mod sys;                        // main.rs:13-14
```
In a lib+bin crate put `#![deny(unsafe_code)]` and the `#[allow(unsafe_code)] mod sys;` in `lib.rs`; `main.rs` uses the lib.

**Imports** (`sys.rs:13-20`): `std::ffi::CStr, std::io, std::os::fd::{FromRawFd, RawFd}, std::ptr, AtomicU32, Duration`.

**Compile-time ABI probes** (`sys.rs:22-35`) — keep, they guard the pidfd ioctl against a libc bump:
```rust
const _PROBE_PIDFD_GET_INFO: libc::Ioctl = libc::PIDFD_GET_INFO;
const _: () = assert!(core::mem::size_of::<libc::pidfd_info>() == 64);
const _: () = assert!(core::mem::offset_of!(libc::pidfd_info, cgroupid) == 8);
```
**Width gotcha** (`sys.rs:37-47`): libc 0.2.191's `CLONE_INTO_CGROUP` is `c_int` and cannot hold 33 bits; redefine `pub const CLONE_INTO_CGROUP: u64 = 0x2_0000_0000;`.

**clone3 into cgroup** (`sys.rs:854-892`, copy verbatim; `ForkResult` enum is defined in the same file):
```rust
pub fn clone3_fork_into_cgroup(cgroup_fd: RawFd) -> io::Result<ForkResult> {
    let mut child_pidfd: libc::c_int = -1;
    // SAFETY: `args` is zero-initialized ...
    let mut args: libc::clone_args = unsafe { core::mem::zeroed() };
    args.flags = CLONE_INTO_CGROUP | (libc::CLONE_PIDFD as u64);
    args.exit_signal = libc::SIGCHLD as u64;
    args.cgroup = cgroup_fd as u64;
    args.pidfd = &mut child_pidfd as *mut libc::c_int as u64;
    // SAFETY: ...
    let rc = unsafe { libc::syscall(libc::SYS_clone3, &mut args as *mut libc::clone_args,
                                    core::mem::size_of::<libc::clone_args>()) };
    if rc < 0 { return Err(io::Error::last_os_error()); }
    if rc == 0 { Ok(ForkResult::Child) } else { Ok(ForkResult::Parent { child_pid: rc as libc::pid_t, child_pidfd }) }
}
```
Also copy (line numbers in aok-adv `sys.rs`): `pidfd_identity` 380, `peer_identity` (SO_PEERCRED/SO_PEERPIDFD route) 407, `pidfd_open` 444, `pidfd_send_signal` 461, `owned_fd` 501, `close_fd` 512, `poll_readable` 523 (pidfd exit wait), `send_fd`/`recv_fd` 754/798, `waitpid_reap` 899. Drop the io_uring/ptrace/process_vm/Landlock-struct items (ADV-only). Remove `#[allow(dead_code)]` markers as they get used. Add new here: `prctl(PR_SET_NO_NEW_PRIVS)` and (if a raw path is needed) nothing for Landlock/seccomp since the crates own those.

**Convention:** every `unsafe {}` gets a `// SAFETY:` comment stating why pointers are live and ownership holds; public wrappers return `io::Result`, never raw errno.

### `tools/aok-launch/src/cgroup.rs` (service, cgroupfs file-I/O)

**Analog:** `tools/linux-baseline/cg.c` (170 lines); rewrite in safe Rust `std::fs`.

**Evacuate root, then enable controllers** (`cg.c:133-145`; RESEARCH Pattern 1):
```c
int cg_setup(void) {
	if (cg_self_path(g_S, sizeof g_S) != 0) return -1;
	if (cg_make_leaf("self", self, sizeof self) != 0) return -1;
	if (cg_put(self, "cgroup.procs", "0") != 0) return -1;
	if (cg_put(g_S, "cgroup.subtree_control", "+memory +pids") != 0) return -1;
```
**Name guard** (`cg.c:124-128`): `ENAMETOOLONG` on over-long path; Rust version also rejects `..`, `/`, empty, NUL (RESEARCH ASVS V5).
**mkdir tolerant of EEXIST** (`cg.c:127`): `mkdir(path, 0755) != 0 && errno != EEXIST`.
**Key lookup in `cgroup.events`** (`cg.c:105-117`, `cg_key`): parse `populated N` / `frozen N` lines; reuse logic for LNCH-05.
**Kill then rmdir with EBUSY retry** (`cg.c:160-170`): write `1` to `cgroup.kill`, loop `rmdir` up to 200 x 5 ms while `EBUSY`. Copy as the test teardown and as the "drained" wait (or watch `populated 0`).
**Layout (D-07):** `<root>/<name>/` (no procs) + `<root>/<name>/main` (leaf). Open `main` with `O_PATH|O_DIRECTORY` for the clone3 cgroup fd. Never run `cgroup.kill` on a cgroup containing the launcher (RESEARCH Pitfall 2).

### `tools/aok-launch/src/landlock.rs` (service, transform)

**Analog:** `tools/linux-baseline/sandbox.c:206-259` (`apply_landlock`), reference only; implement with the `landlock` 0.4.7 crate per RESEARCH. Shape to mirror:
```c
attr.handled_access_fs  = AOK_LANDLOCK_ACCESS_FS_ALL;
attr.handled_access_net = AOK_LANDLOCK_ACCESS_NET_CONNECT_TCP;
attr.scoped = AOK_LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | AOK_LANDLOCK_SCOPE_SIGNAL;   // 214-219
...
ll_add_path(rfd, "/usr", EXECUTE|READ_FILE|READ_DIR)   // 233-236
ll_add_path(rfd, "/etc", READ_FILE|READ_DIR)           // 237-239
prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)                 // 248
landlock_restrict_self(rfd, 0)                         // 250
```
Path rules open with `O_PATH|O_CLOEXEC` (`sandbox.c:208-217`). The launcher adds: the ABI floor constant read from config (D-14), refuse with a named errno below it, `CompatLevel::BestEffort` above, one ruleset per spawn, `E2BIG` at layer 17 reported as is (D-18), and a grant for exec of `aok-launch` plus write on the agent's own directory (D-12). Existing `LandlockRulesetAttr` struct at `aok-adv/src/sys.rs:50-58` and `uapi_compat.h:15-46` are the raw fallback if the crate path is dropped.

### `tools/aok-launch/src/seccomp.rs` (service, transform)

**Analog:** `tools/linux-baseline/sandbox.c:160-182`, `build_allowlist` (shown above). Behaviours the launcher changes:
- Line 168: `SECCOMP_RET_KILL_PROCESS` on arch mismatch becomes `SECCOMP_RET_ERRNO|EPERM` (D-05).
- Lines 179/180: miss returns `ERRNO|EPERM`, hit returns `ALLOW`. Keep. Table is deny-list-shaped in the launcher (D-03 wide allow; denied set returns `EPERM`; number above table max returns `ENOSYS`, D-01).
- Load only `arch` then `nr`; never `args[]` (D-04). Note `sandbox.c:165-167` has an optional `args[0]` load (`with_args0`); do NOT carry it over.
- `seccompiler` 0.5.0 builds the program; `clone3` stays allowed (D-02). Install order: `NO_NEW_PRIVS` first, then filter.

### `tools/aok-launch/src/identity.rs` (service, request-response)

**Analog:** `tools/aok-adv/src/sys.rs:380-430` (`pidfd_identity(pidfd) -> io::Result<PeerIdentity>`, `peer_identity(sock_fd)`); the raw ioctl struct is `tools/linux-baseline/uapi_compat.h:76-96`. Chain per D-09: pidfd -> `PIDFD_GET_INFO`(`PIDFD_INFO_CGROUPID`) -> `/proc/PID/cgroup` check -> drop `main` -> nearest registered root. `PIDFD_GET_INFO` = `0xc040ff0b`. Fallback: `peer_identity` (SO_PEERCRED + `/proc/PID/cgroup`); the test must report the pid-reuse race. Registry: no in-repo analog; write to `$XDG_RUNTIME_DIR/aok-launch/registry.<boot_id>`, key `(boot_id, cgroup_id)`, validate every line on parse.

### `tools/aok-launch/src/main.rs` (CLI)

**Analog:** `tools/aok-adv/src/main.rs:1-20` (header doc, `mod` list, imports `std::process::{Command, Stdio}`, `Duration`). aok-adv's pattern of re-exec'ing itself into hidden subcommands (`attempt`, `scm-helper`) so the attacker is always a separate process is the model for `aok-launch`'s root-vs-sub dispatch (D-12). Usage errors exit 64 (run.sh convention).

### `tools/aok-launch/tests/*.rs` (integration tests)

**Analog:** `tools/aok-adv/tests/topology_and_lifecycle.rs`, `harness_failures.rs`.
```rust
fn bin() -> &'static str { env!("CARGO_BIN_EXE_aok-adv") }       // -> aok-launch

fn fresh_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aok-adv-it-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
```
Drive the real binary via `Command::new(bin())`, poll readiness with a deadline (`Instant::now() + Duration::from_secs(5)` loop), and parse `/proc/<pid>/stat` for ppid (`rsplit_once(')')` because comm may hold spaces/parens). Unit tests sit in-module under `#[cfg(test)]` with names that are sentences (`routes.rs:788+`). Test names to use are in RESEARCH Validation (`spawn_into_cgroup`, `identity_from_pidfd`, `xattr_forgery`, ...). New: `tests/common/` fixture that makes its own leaf inside the delegated subtree and tears it down with the `cg_kill_and_rmdir` loop; run with `systemd-run --user --scope -p Delegate=yes -- cargo test -p aok-launch -- --test-threads=1`.

### `tools/linux-baseline/run.sh` (additive `--wrap`/`--tag` edit)

Current file is 409 lines; last touched by `df2d6b2` (08-01). Plan 08-05 also edits it, so edit by anchors, not line numbers alone.

**Option parsing**
- Header usage doc: lines 3-8 (`usage()` prints `sed -n '2,8p'`, so new option text must stay within lines 2-8 or `usage` range must be widened).
- Defaults: lines 16-23 (`RUN_ID SUITES CONFIGS ROUNDS CPU SMOKE TAG`). Add `WRAP=""`.
- `while/case` parse: lines 29-40; `--tag) TAG="$2"; shift 2 ;;` at line 38. Add `--wrap) WRAP="$2"; shift 2 ;;` next to it.
- Validation: `--tag` valid with `--suites micro` only, lines 62-67; mirror it for `--wrap` and require `-x "$WRAP"`.
- Required binaries check: lines 69-71.

**env.txt writer**: `kv()` at line 97 (`printf '%s=%s\n' "$1" "$2" >>"$RUNDIR/env.txt"`); `write_env` lines 108-148 begins with `: >"$RUNDIR/env.txt"` and is only run when env.txt is empty (check around line 150). Record `kv wrap "$WRAP"` and layer depth inside `suite_begin` (lines 168-179), which already adds `requested_micro_configs` with the `-$TAG` suffix applied (lines 172-176).

**Micro-process launch site** (`run_micro`, lines 192-221):
```bash
label=$c; extra=()
if [ -n "$TAG" ]; then label="$c-$TAG"; extra=(--label "$label"); fi          # 199-204
...
taskset -c "$CPU" "$BIN/micro" --config "$c" --round "$r" \                   # 211
	--tmpfs-dir "$WORK_TMPFS" --root-dir "$WORK_ROOTFS" \
	--out "$RUNDIR/micro.csv" --selfcheck "$RUNDIR/selfcheck.txt" \
	"${SMOKE_ARG[@]}" "${extra[@]}" 2>>"$LOG" || { ... exit 1; }
```
Additive change: build `wrap=()`; `[ -n "$WRAP" ] && wrap=("$WRAP" ...depth args... --)`, then `taskset -c "$CPU" "${wrap[@]}" "$BIN/micro" ...`. With `WRAP` empty the command line is byte-identical (the Phase 8 contract). `micro` reads `--label` so C0 rows and wrapped rows coexist in one `micro.csv`; `have_rows` (around line 66 in the function, ~line 205) keys on label so a distinct `--tag lldepth-N` avoids the exit-3 duplicate guard. Other `taskset` sites to leave alone: `run_pinned` 289-300, broker ~314, deadline/cgroup suites.

**Dispatch**: lines 398-408 (`micro) run_micro ;;`). `summarize.py` is invoked in `suite_end` (line 183 area, `python3 -I "$TOOLS/summarize.py" "$RUNDIR"`); no change needed. `.gitignore` in `tools/linux-baseline` ignores `bin/`, `.work/`, `results/smoke-*/`; real run dirs are committed.

### `tools/linux-baseline/results/<run-id>/` layout (depth reads)

Analog: `results/20261028T034700Z/`, tracked files: `env.txt`, `run.log`, `micro.csv`, `selfcheck.txt`, `summary.csv`, `summary.md`, plus per-suite `broker.csv`, `deadline.csv`, `cgroup.csv`, `cgroup-probe.txt`, `cgroup-behaviour.txt`. For the depth reads use `--suites micro` so only `micro.csv`, `selfcheck.txt`, `summary.*`, `env.txt`, `run.log` appear. `env.txt` is `key=value` per line (`date_utc`, `harness_rev`, `harness_dirty`, `uname_r`, `cpu_pinned`, `wsl2`, `requested_micro_configs`, `loadavg_before_micro_1-5`, `elapsed_s_*`, plus `micro --env` and `floor` output). Run id charset `[A-Za-z0-9._-]`; `smoke-*` ids are wiped and not committed. Tagged labels look like `C2a-lldepth-2` and are checked by `check_report.py` and `r1.py` unchanged.

## Shared Patterns

### Unsafe confinement and SAFETY comments
**Source:** `tools/aok-adv/src/sys.rs` (all blocks); `main.rs:1,13`. Apply to every aok-launch module: only `sys.rs` may hold `unsafe`; all other modules call safe wrappers.

### Error contract
Wrappers return `io::Result<T>`; CLI maps to exit codes and prints the errno name. Denials are errnos (LNCH-06); below-floor refusal is a non-zero exit with the missing right named (D-14).

### Verification tagging
Every claim in reports is VERIFIED/ASSUMED; interpretation marked (AGENTS.md). Fallback-path results carry "simulated on a kernel that has the feature" (D-13).

### Input validation
**Source:** `cg.c:124-128` name-length guard, extended to `..`, `/`, empty (RESEARCH ASVS V5).

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| registry file reader/writer | service | file-I/O | no existing persisted-registry code; use RESEARCH Open Question 2 layout |
| external seccomp shim wrapper | test wrapper | transform | nothing installs a fake-errno filter; adapt `build_allowlist` structure (`sandbox.c:162-182`) with an `ioctl` request-number match (`0xc040ff0b` -> `ENOTTY`) and a whole-`clone3` `ENOSYS` rule |
| `lib.rs` surface | library | n/a | aok-adv is bin-only; no lib+bin crate to copy |

## Metadata

**Analog search scope:** `tools/aok-adv`, `tools/linux-baseline`, `tools/*.py|sh`
**Files read:** CONTEXT, RESEARCH, aok-adv Cargo/toolchain/sys.rs (header + clone3), main.rs head, two test heads, run.sh (all relevant ranges), cg.c, sandbox.c ranges
**Not cited:** anything under `.claude/worktrees/`
**Pattern extraction date:** 2026-11-05
