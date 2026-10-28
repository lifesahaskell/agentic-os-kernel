#![deny(unsafe_code)]
//! aok-adv: the ADV-01/02/03 adversarial suite and stub broker (08-02).
//! Every raw syscall lives in `sys` (D-13); every other module is safe Rust.
//! Subcommands: `stub`, `run`, `list-routes`, `report`, plus two hidden
//! subcommands `attempt` and `scm-helper` that `run` re-execs itself into
//! (D2/D3): the attacker and any helper are always separate processes from
//! the runner, never the runner's own code running the syscall in-process.

mod cases;
mod report;
mod routes;
mod stub;
#[allow(unsafe_code)]
mod sys;

use std::io::{self, BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn print_usage() {
    eprintln!("usage: aok-adv <stub|run|list-routes|report> [args...]");
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return std::process::ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.collect();
    let result = match cmd.as_str() {
        "stub" => stub::run_cli(&rest),
        "list-routes" => list_routes_cli(&rest),
        "run" => run_cli(&rest),
        "report" => report_cli(&rest),
        "attempt" => attempt_cli(&rest),
        "scm-helper" => scm_helper_cli(&rest),
        other => {
            eprintln!("aok-adv: unknown subcommand '{other}'");
            print_usage();
            return std::process::ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aok-adv: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn list_routes_cli(args: &[String]) -> io::Result<()> {
    routes::list_routes_cli(args)
}

/// Default `--cgroup-root`: the throwaway subtree 08-03's `--adv` wrapper
/// creates (D-16); never written to unless a route explicitly needs it.
const DEFAULT_CGROUP_ROOT: &str = "/sys/fs/cgroup/aok-phase8/adv";

/// How long `run`/`attempt` wait for a freshly spawned stub to start
/// accepting connections before giving up (D3).
const STUB_ACCEPT_TIMEOUT: Duration = Duration::from_secs(5);

fn require_value(args: &[String], i: usize, flag: &str) -> io::Result<String> {
    args.get(i).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{flag} requires a value"),
        )
    })
}

// ---------------------------------------------------------------------------
// `aok-adv run`: parses flags, owns the stub lifecycle, and drives one
// attacker (and sometimes one helper) subprocess per route row (D2/D3).
// ---------------------------------------------------------------------------

/// Parsed `run` flags (D-16/ADV-01..03): `--out` is the only required flag.
/// `--routes`/`--topology`/`--cases` are optional filters (comma-separated
/// for the first and third); omitting `--socket` skips route evaluation
/// entirely (no route row is attempted without a broker socket path to
/// start a stub on), while the S1 case suite always runs.
struct RunArgs {
    socket: Option<String>,
    cgroup_root: String,
    no_cgroup: bool,
    routes: Option<Vec<String>>,
    topology: Option<String>,
    cases: Option<Vec<String>>,
    mode: String,
    out: String,
    broker_pid: Option<i32>,
}

fn parse_run_args(args: &[String]) -> io::Result<RunArgs> {
    let mut socket = None;
    let mut cgroup_root = DEFAULT_CGROUP_ROOT.to_string();
    let mut no_cgroup = false;
    let mut routes_filter = None;
    let mut topology = None;
    let mut cases_filter = None;
    let mut mode = "nonagent".to_string();
    let mut out = None;
    let mut broker_pid = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                i += 1;
                socket = Some(require_value(args, i, "--socket")?);
            }
            "--cgroup-root" => {
                i += 1;
                cgroup_root = require_value(args, i, "--cgroup-root")?;
            }
            "--no-cgroup" => no_cgroup = true,
            "--routes" => {
                i += 1;
                let v = require_value(args, i, "--routes")?;
                routes_filter = Some(v.split(',').map(str::to_string).collect());
            }
            "--topology" => {
                i += 1;
                topology = Some(require_value(args, i, "--topology")?);
            }
            "--cases" => {
                i += 1;
                let v = require_value(args, i, "--cases")?;
                cases_filter = Some(v.split(',').map(str::to_string).collect());
            }
            "--mode" => {
                i += 1;
                mode = require_value(args, i, "--mode")?;
            }
            "--out" => {
                i += 1;
                out = Some(require_value(args, i, "--out")?);
            }
            "--broker-pid" => {
                i += 1;
                let v = require_value(args, i, "--broker-pid")?;
                broker_pid = Some(v.parse::<i32>().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("--broker-pid: not an integer: {v}"),
                    )
                })?);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("run: unknown flag {other}"),
                ));
            }
        }
        i += 1;
    }

    let out =
        out.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "run requires --out"))?;

    Ok(RunArgs {
        socket,
        cgroup_root,
        no_cgroup,
        routes: routes_filter,
        topology,
        cases: cases_filter,
        mode,
        out,
        broker_pid,
    })
}

/// Map one route name to the `routes::attempt_*` call that exercises it
/// (D-13: every raw syscall still lives in `sys`, reached only through
/// `routes`'s safe wrappers). `socket` doubles as both the pathname-socket
/// path and the abstract-socket name, since `run`'s flag set has no
/// separate `--abstract-name` (deviation, documented in the plan Summary).
///
/// Cgroup-write routes resolve their leaf under `cgroup_root` by name:
/// `cgroup.kill` always targets the broker leaf (the stub's own placement,
/// D4's one documented exception to "no placement in nonagent" — the route
/// is meaningless otherwise); `cgroup.procs`/`clone3_CLONE_INTO_CGROUP`
/// target agent-b, the "other agent" this suite's impersonation routes
/// attack, independent of what mode the attacker itself is running under.
fn dispatch_attempt(
    route: &str,
    target: &routes::Target,
    cgroup_root: &str,
    mode: &str,
    no_cgroup: bool,
) -> io::Result<()> {
    let broker_dir = format!("{cgroup_root}/broker");
    let agent_b_dir = format!("{cgroup_root}/agent-b");

    match route {
        "kill" => routes::attempt_kill(target),
        "tgkill" => routes::attempt_tgkill(target),
        "rt_sigqueueinfo" => routes::attempt_rt_sigqueueinfo(target),
        "pidfd_send_signal" => routes::attempt_pidfd_send_signal(target),
        "cgroup.kill" => routes::attempt_cgroup_kill(&broker_dir),
        "PTRACE_ATTACH" => routes::attempt_ptrace_attach(target),
        "PTRACE_SEIZE" => routes::attempt_ptrace_seize(target),
        "process_vm_readv" => routes::attempt_process_vm_readv(target),
        "process_vm_writev" => routes::attempt_process_vm_writev(target),
        "/proc/pid/mem" => routes::attempt_proc_pid_mem(target),
        "pidfd_getfd" => routes::attempt_pidfd_getfd(target),
        "pathname" => routes::attempt_bind_pathname(target),
        "abstract" => routes::attempt_bind_abstract(target),
        "cgroup.procs" => routes::attempt_cgroup_procs_move(&agent_b_dir, &target.socket_path),
        "scm_rights" => routes::attempt_scm_rights(
            &target.socket_path,
            scm_agent_b_dir(cgroup_root, mode, no_cgroup).as_deref(),
        ),
        "payload_claim" => routes::attempt_payload_claim(&target.socket_path, "impersonated-id"),
        "clone3_CLONE_INTO_CGROUP" => {
            let dir = std::fs::File::open(&agent_b_dir).map_err(|e| {
                routes::harness_error(format!("opening the target cgroup {agent_b_dir}: {e}"))
            })?;
            routes::attempt_clone3_into_cgroup(dir.as_raw_fd(), &agent_b_dir)
        }
        other => Err(routes::harness_error(format!("run: unknown route {other}"))),
    }
}

/// The cgroup the `scm_rights` helper is placed in, and whose id the stub's
/// reply is checked against: `<cgroup-root>/agent-b` in agent mode (not
/// `--no-cgroup`), none otherwise, where success just means a real borrowed
/// connected fd was obtained and used.
fn scm_agent_b_dir(cgroup_root: &str, mode: &str, no_cgroup: bool) -> Option<String> {
    (mode == "agent" && !no_cgroup).then(|| format!("{cgroup_root}/agent-b"))
}

/// Spawn the stub broker as a fresh child of the calling process, re-exec'd
/// from `current_exe()` (D3): never an external, suite-didn't-spawn broker.
/// Its stdio is never left to inherit the caller's: `run`'s own stderr
/// carries the per-row diagnostic line (acceptance (b)/(c)), and `attempt`'s
/// stderr carries both that diagnostic and the final `REACHABLE`/`VOID`
/// verdict a caller parses, so a stub log line landing on the same stream
/// (e.g. a request error from a connection the attempt itself raced) would
/// otherwise corrupt either read.
///
/// `stub::run_cli` always requires `--socket PATH` (its CLI contract is
/// `stub --socket PATH [--abstract NAME]`, never `--abstract` alone), so
/// `--socket socket_path` is always passed here even in abstract mode;
/// `stub::serve` binds the abstract name instead whenever one is given and
/// never touches the socket-path argument in that case (gap closure D3: the
/// original either/or here made every abstract-route row's stub exit
/// immediately with "stub: --socket PATH is required", silently swallowed
/// by `Stdio::null()`, surfacing only as a 5-second `wait_for_stub_ready`
/// timeout with no errno).
fn spawn_stub(socket_path: &str, abstract_mode: bool) -> io::Result<std::process::Child> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.arg("stub").arg("--socket").arg(socket_path);
    if abstract_mode {
        cmd.arg("--abstract").arg(socket_path);
    }
    cmd.spawn()
}

/// Block (bounded) until the stub is accepting connections, by polling a
/// real connect attempt rather than sleeping a fixed guess (D3). A stub that
/// exits before it accepts (bind failure, bad path) is reported at once,
/// naming its exit status, instead of burning the whole timeout.
fn wait_for_stub_ready(
    stub: &mut std::process::Child,
    socket_path: &str,
    abstract_mode: bool,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = stub.try_wait()? {
            return Err(io::Error::other(format!(
                "stub exited ({status}) before accepting connections on {socket_path}"
            )));
        }
        let connected = if abstract_mode {
            use std::os::linux::net::SocketAddrExt;
            use std::os::unix::net::{SocketAddr, UnixStream};
            SocketAddr::from_abstract_name(socket_path.as_bytes())
                .and_then(|addr| UnixStream::connect_addr(&addr))
                .is_ok()
        } else {
            std::os::unix::net::UnixStream::connect(socket_path).is_ok()
        };
        if connected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "stub did not start accepting connections within the timeout",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn kill_and_reap(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Run one route row end to end: spawn a fresh stub and a separate attacker
/// process in the topology the row names (D2), hand the attacker a `go`
/// signal only once the stub (sibling topology) is ready, and parse the
/// attacker's reported outcome.
///
/// The return type separates the two kinds of failure: the outer `Err` is a
/// harness failure (stub could not be spawned or never became ready,
/// placement failed, the attacker produced no verdict), which `run_cli`
/// turns into a non-zero exit with no row; the inner `io::Result` is the
/// attacker's own attempt, the only thing allowed to yield a void row.
fn run_one_row(
    row: &routes::RouteRow,
    socket: &str,
    cgroup_root: &str,
    mode: &str,
    no_cgroup: bool,
) -> io::Result<io::Result<()>> {
    let abstract_mode = row.route == "abstract";
    let place_attacker_in_agent_a = mode == "agent" && !no_cgroup;

    match row.topology {
        routes::Topology::Sibling => {
            let mut stub_child = spawn_stub(socket, abstract_mode)
                .map_err(|e| routes::harness_error(format!("spawning the stub: {e}")))?;
            let stub_pid = stub_child.id() as libc::pid_t;
            if let Err(e) =
                wait_for_stub_ready(&mut stub_child, socket, abstract_mode, STUB_ACCEPT_TIMEOUT)
            {
                kill_and_reap(stub_child);
                return Err(routes::harness_error(format!("stub not ready: {e}")));
            }
            // Diagnostic (also acceptance (c)'s observable: a fresh stub pid
            // per row, since every row re-enters `run_one_row` and spawns a
            // brand new child here).
            eprintln!(
                "row route={} topology={} stub_pid={stub_pid}",
                row.route,
                row.topology.as_str()
            );

            // D4: the stub always lands in the broker leaf when this host
            // has a delegated cgroup and the row either needs one, or the
            // suite is running in agent mode (placement is otherwise a
            // no-op in nonagent mode).
            if !no_cgroup && (routes::needs_cgroup(row.route) || mode == "agent") {
                let broker = format!("{cgroup_root}/broker");
                if let Err(e) = routes::place_in_cgroup(stub_pid, &broker) {
                    kill_and_reap(stub_child);
                    return Err(routes::harness_error(format!(
                        "placing the stub in {broker}: {e}"
                    )));
                }
            }

            // D5: the `abstract` route's gap-bind oracle needs the stub
            // dead before the attacker binds the name, and a fresh one
            // started afterward; killing it here (before the attacker ever
            // runs) creates that gap. The per-row "fresh stub" policy (D3)
            // means the loop's next iteration starts an unrelated new one
            // regardless of what happens to this one.
            if row.route == "abstract" {
                kill_and_reap(stub_child);
                let result = run_attacker_subprocess(
                    row.route,
                    AttackerStubMode::Sibling {
                        pid: 0,
                        mode: mode.to_string(),
                        no_cgroup,
                    },
                    socket,
                    cgroup_root,
                    place_attacker_in_agent_a,
                );
                return result;
            }

            let result = run_attacker_subprocess(
                row.route,
                AttackerStubMode::Sibling {
                    pid: stub_pid,
                    mode: mode.to_string(),
                    no_cgroup,
                },
                socket,
                cgroup_root,
                place_attacker_in_agent_a,
            );
            kill_and_reap(stub_child);
            result
        }
        routes::Topology::Ancestor => run_attacker_subprocess(
            row.route,
            AttackerStubMode::AncestorSpawn {
                mode: mode.to_string(),
                no_cgroup,
            },
            socket,
            cgroup_root,
            place_attacker_in_agent_a,
        ),
    }
}

enum AttackerStubMode {
    /// Sibling topology: the runner already spawned the stub; this is its
    /// pid. The attacker only attacks it, never spawns it.
    Sibling {
        pid: libc::pid_t,
        mode: String,
        no_cgroup: bool,
    },
    /// Ancestor topology: the attacker itself spawns and owns the stub as
    /// its own child (D2), then attacks that child.
    AncestorSpawn { mode: String, no_cgroup: bool },
}

/// Spawn the attacker as a separate process from the runner (D2), hand it a
/// `go` signal over a pipe once any runner-side setup (stub readiness,
/// agent-a placement) is done, and parse its stdout for the attempt's
/// outcome. The outer `Err` is a harness failure, the inner result is the
/// attempt's own (see `run_one_row`).
fn run_attacker_subprocess(
    route: &str,
    stub_mode: AttackerStubMode,
    socket: &str,
    cgroup_root: &str,
    place_attacker_in_agent_a: bool,
) -> io::Result<io::Result<()>> {
    let exe = std::env::current_exe()
        .map_err(|e| routes::harness_error(format!("locating own executable: {e}")))?;
    let mut cmd = Command::new(exe);
    cmd.arg("attempt")
        .arg("--route")
        .arg(route)
        .arg("--socket")
        .arg(socket)
        .arg("--cgroup-root")
        .arg(cgroup_root);

    match &stub_mode {
        AttackerStubMode::Sibling {
            pid,
            mode,
            no_cgroup,
        } => {
            cmd.arg("--topology")
                .arg("sibling")
                .arg("--pid")
                .arg(pid.to_string())
                .arg("--mode")
                .arg(mode);
            if *no_cgroup {
                cmd.arg("--no-cgroup");
            }
        }
        AttackerStubMode::AncestorSpawn { mode, no_cgroup } => {
            cmd.arg("--topology")
                .arg("ancestor")
                .arg("--mode")
                .arg(mode);
            if *no_cgroup {
                cmd.arg("--no-cgroup");
            }
        }
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| routes::harness_error(format!("spawning the attacker: {e}")))?;

    if place_attacker_in_agent_a {
        let agent_a = format!("{cgroup_root}/agent-a");
        if let Err(e) = routes::place_in_cgroup(child.id() as libc::pid_t, &agent_a) {
            kill_and_reap(child);
            return Err(routes::harness_error(format!(
                "placing the attacker in {agent_a}: {e}"
            )));
        }
    }

    // Pipe handshake (D2): the attacker blocks on stdin until it reads
    // this line, so nothing it does races the runner's own setup above.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"go\n");
    }

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    let _ = child.wait();

    parse_attempt_outcome(stdout.trim(), stderr.trim())
}

/// Parse the attacker subprocess's one-line stdout verdict back into the
/// two-level shape the row loop expects: `REACHABLE` is `Ok(Ok(()))`;
/// `VOID errno=<n|none> msg=<...>` is `Ok(Err(..))` with that errno attached
/// (D5); `HARNESS msg=<...>` and an unparseable or missing verdict are an
/// outer `Err`, a harness failure that produces no row.
fn parse_attempt_outcome(stdout: &str, stderr: &str) -> io::Result<io::Result<()>> {
    if stdout == "REACHABLE" {
        return Ok(Ok(()));
    }
    if let Some(rest) = stdout.strip_prefix("HARNESS ") {
        return Err(routes::harness_error(
            rest.trim_start_matches("msg=").to_string(),
        ));
    }
    if let Some(rest) = stdout.strip_prefix("VOID ") {
        let mut errno = None;
        let mut msg = rest.to_string();
        if let Some(after_errno) = rest.strip_prefix("errno=") {
            let (errno_str, remainder) = after_errno.split_once(' ').unwrap_or((after_errno, ""));
            errno = errno_str.parse::<i32>().ok();
            msg = remainder.trim_start_matches("msg=").to_string();
        }
        return Ok(Err(match errno {
            Some(e) => io::Error::from_raw_os_error(e),
            None => io::Error::other(if msg.is_empty() {
                format!("attacker reported void with no errno; stderr: {stderr}")
            } else {
                msg
            }),
        }));
    }
    Err(routes::harness_error(format!(
        "attacker subprocess produced no parseable verdict; stdout: {stdout:?} stderr: {stderr:?}"
    )))
}

/// `aok-adv run`: the S1 case suite always runs; route rows run only when
/// `--socket` names a path to start a stub on. Cgroup-bearing routes still
/// go through `routes::evaluate_row_with_errno`'s own `--no-cgroup` skip
/// (D-16) regardless of this flag's default here. `--broker-pid` is
/// rejected outright: external-broker mode is ADV-05 (Phase 10), not this
/// plan, and this suite never signals or ptraces a pid it did not spawn.
fn run_cli(args: &[String]) -> io::Result<()> {
    let parsed = parse_run_args(args)?;

    if parsed.broker_pid.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "--broker-pid: external-broker mode is ADV-05 (Phase 10) and not supported yet; \
             this suite only ever attacks a stub it spawned itself",
        ));
    }

    if let Some(socket) = &parsed.socket {
        check_socket_path(socket)?;
    }

    let ptrace_scope = routes::read_ptrace_scope();
    let runid = std::process::id().to_string();
    let work_dir = std::env::temp_dir().join(format!("aok-adv-run-{runid}"));
    std::fs::create_dir_all(&work_dir)?;

    // The rows are collected in memory and written only after every row has
    // run, so a harness failure in any row leaves no output file at all.
    let result = collect_records(&parsed, &work_dir, &runid, &ptrace_scope)
        .and_then(|records| report::write_jsonl(&records, Path::new(&parsed.out)));
    let _ = std::fs::remove_dir_all(&work_dir);
    result
}

/// An AF_UNIX `sun_path` holds 108 bytes including the trailing NUL (and, for
/// an abstract name, the leading NUL), so a longer `--socket` can never be
/// bound. That is a harness problem: say so up front instead of letting the
/// stub fail to bind and the row read as a failed baseline.
const SUN_PATH_MAX: usize = 107;

fn check_socket_path(socket: &str) -> io::Result<()> {
    if socket.len() > SUN_PATH_MAX {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "--socket is {} bytes; an AF_UNIX socket path holds at most {SUN_PATH_MAX} bytes",
                socket.len()
            ),
        ));
    }
    if socket.is_empty() || socket.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--socket must be a non-empty path without NUL bytes",
        ));
    }
    Ok(())
}

/// Run the S1 cases and the selected route rows. Only the attacker's own
/// attempt may produce a void row; any harness failure aborts the whole
/// invocation with an error naming the row and the cause.
fn collect_records(
    parsed: &RunArgs,
    work_dir: &Path,
    runid: &str,
    ptrace_scope: &str,
) -> io::Result<Vec<report::Record>> {
    let mut records = Vec::new();

    // D1/deviation: when `--routes` selects specific route rows, the S1
    // case suite (always-on in the original flag set) is suppressed, so a
    // route-focused wrapper invocation does not also re-run and re-report
    // all 8 cases on every row's own invocation. `--cases` without
    // `--routes` still runs cases only, exactly as before.
    //
    // The cases ignore cgroup confinement, so in agent mode they would run
    // as an unconfined process and their success would be noise: no case
    // rows are emitted and `agent_result` stays null (untested).
    if parsed.routes.is_none() {
        if parsed.mode == "agent" {
            eprintln!(
                "aok-adv: agent-mode S1 needs enforcement hooks that arrive with ADV-04 and \
                 Phases 11-13; no S1 case rows emitted (agent_result stays untested)"
            );
        } else {
            for outcome in cases::run_all_cases(work_dir, runid) {
                if let Some(filter) = &parsed.cases
                    && !filter.iter().any(|c| c.as_str() == outcome.spec.class)
                {
                    continue;
                }
                records.push(report::Record::from_case(
                    outcome.spec,
                    outcome.baseline_ok,
                    &parsed.mode,
                    ptrace_scope,
                ));
            }
        }
    }

    if let Some(socket) = &parsed.socket {
        for row in routes::all_rows() {
            if let Some(filter) = &parsed.routes
                && !filter.iter().any(|r| r.as_str() == row.route)
            {
                continue;
            }
            if let Some(topology) = &parsed.topology
                && row.topology.as_str() != topology.as_str()
            {
                continue;
            }
            let mut harness_failure: Option<io::Error> = None;
            let (outcome, errno) = routes::evaluate_row_with_errno(&row, parsed.no_cgroup, || {
                match run_one_row(
                    &row,
                    socket,
                    &parsed.cgroup_root,
                    &parsed.mode,
                    parsed.no_cgroup,
                ) {
                    Ok(attempt) => attempt,
                    Err(e) => {
                        harness_failure = Some(e);
                        Ok(())
                    }
                }
            });
            if let Some(e) = harness_failure {
                return Err(io::Error::new(
                    e.kind(),
                    format!(
                        "harness failure on route {} topology {}: {e}",
                        row.route,
                        row.topology.as_str()
                    ),
                ));
            }
            records.push(report::Record::from_route(
                &row,
                &outcome,
                &parsed.mode,
                ptrace_scope,
                errno,
            ));
        }
    }
    Ok(records)
}

// ---------------------------------------------------------------------------
// `aok-adv attempt` (hidden): the attacker subprocess (D2). Never run
// directly by a human; `run_cli` re-execs itself into this. Blocks on one
// line of stdin (the pipe handshake) before doing anything, then runs
// exactly one route against exactly one target and reports the verdict on
// stdout as `REACHABLE` or `VOID errno=<n|none> msg=<text>`.
// ---------------------------------------------------------------------------

struct AttemptArgs {
    route: String,
    topology: String,
    socket: String,
    cgroup_root: String,
    pid: Option<i32>,
    mode: Option<String>,
    no_cgroup: bool,
}

fn parse_attempt_args(args: &[String]) -> io::Result<AttemptArgs> {
    let mut route = None;
    let mut topology = None;
    let mut socket = None;
    let mut cgroup_root = DEFAULT_CGROUP_ROOT.to_string();
    let mut pid = None;
    let mut mode = None;
    let mut no_cgroup = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--route" => {
                i += 1;
                route = Some(require_value(args, i, "--route")?);
            }
            "--topology" => {
                i += 1;
                topology = Some(require_value(args, i, "--topology")?);
            }
            "--socket" => {
                i += 1;
                socket = Some(require_value(args, i, "--socket")?);
            }
            "--cgroup-root" => {
                i += 1;
                cgroup_root = require_value(args, i, "--cgroup-root")?;
            }
            "--pid" => {
                i += 1;
                let v = require_value(args, i, "--pid")?;
                pid = Some(v.parse::<i32>().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("--pid: not an integer: {v}"),
                    )
                })?);
            }
            "--mode" => {
                i += 1;
                mode = Some(require_value(args, i, "--mode")?);
            }
            "--no-cgroup" => no_cgroup = true,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("attempt: unknown flag {other}"),
                ));
            }
        }
        i += 1;
    }

    Ok(AttemptArgs {
        route: route.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "attempt requires --route")
        })?,
        topology: topology.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "attempt requires --topology")
        })?,
        socket: socket.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "attempt requires --socket")
        })?,
        cgroup_root,
        pid,
        mode,
        no_cgroup,
    })
}

fn attempt_cli(args: &[String]) -> io::Result<()> {
    let parsed = parse_attempt_args(args)?;

    // Pipe handshake (D2): block until the runner signals "go", so this
    // process never races the runner's own stub-readiness/placement setup.
    let mut line = String::new();
    let _ = io::stdin().lock().read_line(&mut line);

    let mode = parsed.mode.as_deref().unwrap_or("nonagent");
    let outcome: io::Result<()> = (|| {
        let abstract_mode = parsed.route == "abstract";
        let (pid, stub_child) = match parsed.topology.as_str() {
            "sibling" => {
                let pid = parsed
                    .pid
                    .ok_or_else(|| routes::harness_error("sibling topology requires --pid"))?;
                (pid as libc::pid_t, None)
            }
            "ancestor" => {
                let mut stub_child = spawn_stub(&parsed.socket, abstract_mode)
                    .map_err(|e| routes::harness_error(format!("spawning the stub: {e}")))?;
                let stub_pid = stub_child.id() as libc::pid_t;
                if let Err(e) = wait_for_stub_ready(
                    &mut stub_child,
                    &parsed.socket,
                    abstract_mode,
                    STUB_ACCEPT_TIMEOUT,
                ) {
                    kill_and_reap(stub_child);
                    return Err(routes::harness_error(format!("stub not ready: {e}")));
                }
                if !parsed.no_cgroup && (routes::needs_cgroup(&parsed.route) || mode == "agent") {
                    let broker = format!("{}/broker", parsed.cgroup_root);
                    if let Err(e) = routes::place_in_cgroup(stub_pid, &broker) {
                        kill_and_reap(stub_child);
                        return Err(routes::harness_error(format!(
                            "placing the stub in {broker}: {e}"
                        )));
                    }
                }
                // Diagnostic (also acceptance (b)'s observable: in ancestor
                // topology this process, the attacker, is the stub's own
                // parent). `AOK_ADV_TEST_PAUSE_MS` is a test-only hook: when
                // set, pause here so an integration test can read this line
                // and inspect /proc/<stub_pid>/stat before the route attempt
                // (which may kill the stub, e.g. no route in this suite's
                // ancestor set does, but a future one might) runs.
                eprintln!("stub_pid={stub_pid} attacker_pid={}", std::process::id());
                if let Ok(ms) = std::env::var("AOK_ADV_TEST_PAUSE_MS")
                    && let Ok(ms) = ms.parse::<u64>()
                {
                    std::thread::sleep(Duration::from_millis(ms));
                }
                (stub_pid, Some(stub_child))
            }
            other => {
                return Err(routes::harness_error(format!(
                    "attempt: unknown topology {other}"
                )));
            }
        };

        let target = routes::Target {
            pid,
            socket_path: parsed.socket.clone(),
            abstract_name: if abstract_mode {
                Some(parsed.socket.clone())
            } else {
                None
            },
        };
        let result = dispatch_attempt(
            &parsed.route,
            &target,
            &parsed.cgroup_root,
            mode,
            parsed.no_cgroup,
        );
        if let Some(stub_child) = stub_child {
            kill_and_reap(stub_child);
        }
        result
    })();

    match outcome {
        Ok(()) => {
            println!("REACHABLE");
            Ok(())
        }
        Err(e) if routes::is_harness_error(&e) => {
            // Not the attempt's verdict: the runner turns this into a
            // non-zero exit with no row.
            println!("HARNESS msg={}", e.to_string().replace('\n', " "));
            eprintln!("aok-adv attempt: harness failure: {e}");
            Ok(())
        }
        Err(e) => {
            let errno = e
                .raw_os_error()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "none".to_string());
            println!("VOID errno={errno} msg={e}");
            eprintln!("aok-adv attempt: {e}");
            // Non-zero would also signal failure, but the runner reads the
            // verdict from stdout regardless; exit 0 so a legitimate "void"
            // baseline (an expected outcome, not a crash) never looks like
            // the attempt subprocess itself malfunctioned.
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// `aok-adv scm-helper` (hidden): route (ii)'s real helper process (D5). It
// connects to the stub, then hands that connected fd to whoever is
// listening at `--handoff` via `SCM_RIGHTS`, so the attacker ends up
// talking to the stub through a connection stamped with this helper's
// identity, not its own.
// ---------------------------------------------------------------------------

fn scm_helper_cli(args: &[String]) -> io::Result<()> {
    let mut socket = None;
    let mut handoff = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                i += 1;
                socket = Some(require_value(args, i, "--socket")?);
            }
            "--handoff" => {
                i += 1;
                handoff = Some(require_value(args, i, "--handoff")?);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("scm-helper: unknown flag {other}"),
                ));
            }
        }
        i += 1;
    }
    let socket = socket.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "scm-helper requires --socket")
    })?;
    let handoff = handoff.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "scm-helper requires --handoff")
    })?;

    // Wait for the attacker's "go" (it places this helper in its cgroup
    // first, so the identity the stub reads is the placed one), and stay
    // alive after the handoff until the attacker closes our stdin, so the
    // stub can still read this process's identity when it answers.
    let mut line = String::new();
    let _ = io::stdin().lock().read_line(&mut line);

    let stub_conn = std::os::unix::net::UnixStream::connect(&socket)?;
    let handoff_conn = std::os::unix::net::UnixStream::connect(&handoff)?;
    sys::send_fd(handoff_conn.as_raw_fd(), stub_conn.as_raw_fd())?;
    let mut rest = Vec::new();
    let _ = io::stdin().lock().read_to_end(&mut rest);
    Ok(())
}

/// `aok-adv report --in <nonagent.jsonl> --in <agent.jsonl> [--merge-counters
/// deny-counters.txt] --out <report.json>`: the first `--in` is the
/// non-agent baseline, the second is the agent-cgroup run (ordering is an
/// assumption of this flag set, documented in the plan Summary).
fn report_cli(args: &[String]) -> io::Result<()> {
    let mut counters_path = None;
    let mut ins: Vec<String> = Vec::new();
    let mut out = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--merge-counters" => {
                i += 1;
                counters_path = Some(require_value(args, i, "--merge-counters")?);
            }
            "--in" => {
                i += 1;
                ins.push(require_value(args, i, "--in")?);
            }
            "--out" => {
                i += 1;
                out = Some(require_value(args, i, "--out")?);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("report: unknown flag {other}"),
                ));
            }
        }
        i += 1;
    }

    if ins.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "report requires exactly two --in files (nonagent, then agent)",
        ));
    }
    let out =
        out.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "report requires --out"))?;

    let nonagent = report::read_jsonl(Path::new(&ins[0]))?;
    let agent = report::read_jsonl(Path::new(&ins[1]))?;
    let counters = match &counters_path {
        Some(path) => report::parse_deny_counters(Path::new(path))?,
        None => Vec::new(),
    };

    let merged = report::merge(&nonagent, &agent, &counters);
    let text = report::report_text(&merged);
    report::write_report_json(&merged, &text, Path::new(&out))?;
    println!("{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::sys;
    use std::process::Command;
    use std::time::Duration;

    /// The kill route, non-agent, --no-cgroup: a suite-spawned child killed
    /// by `pidfd_send_signal(SIGKILL)`, verified by the pidfd reporting exit
    /// (ADV-02). Never touches an existing process: the child is spawned by
    /// this test and nothing else; the pidfd is opened by this test, not
    /// derived from a raw pid handed around elsewhere.
    #[test]
    fn kill_route_pidfd_send_signal_non_agent() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .expect("spawn target child");
        let pid = child.id() as libc::pid_t;

        let pidfd = sys::pidfd_open(pid, 0).expect("pidfd_open the spawned child");
        sys::pidfd_send_signal(pidfd, libc::SIGKILL).expect("pidfd_send_signal SIGKILL");

        let exited =
            sys::poll_readable(pidfd, Duration::from_secs(5)).expect("poll pidfd for exit");
        assert!(exited, "pidfd never reported exit after SIGKILL");

        let _ = child.wait();
        sys::close_fd(pidfd);
    }

    /// D5/acceptance (e): `--broker-pid` must be rejected with the ADV-05
    /// message, never treated as a liveness check against `/proc`.
    #[test]
    fn broker_pid_flag_is_rejected_with_adv05_message() {
        let args: Vec<String> = [
            "--out",
            "/tmp/aok-adv-broker-pid-test.jsonl",
            "--broker-pid",
            "1",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let err = super::run_cli(&args).expect_err("--broker-pid must be rejected outright");
        assert!(
            err.to_string().contains("ADV-05"),
            "unexpected error message: {err}"
        );
    }

    /// B: in agent mode (not `--no-cgroup`) the scm_rights route is handed
    /// `<cgroup-root>/agent-b`, so success means the stub's reply names
    /// agent-b's cgroup id. Fails when `None` is passed unconditionally (the
    /// pre-fix dispatch). The tempdir stands in for the cgroup root.
    #[test]
    fn scm_agent_b_dir_is_wired_only_in_agent_mode() {
        let root = std::env::temp_dir().join(format!("aok-adv-agentb-{}", std::process::id()));
        std::fs::create_dir_all(root.join("agent-b")).expect("create stand-in agent-b");
        let root_s = root.to_str().expect("utf-8 temp path");

        assert_eq!(
            super::scm_agent_b_dir(root_s, "agent", false),
            Some(format!("{root_s}/agent-b"))
        );
        assert_eq!(super::scm_agent_b_dir(root_s, "nonagent", false), None);
        assert_eq!(super::scm_agent_b_dir(root_s, "baseline", false), None);
        assert_eq!(super::scm_agent_b_dir(root_s, "agent", true), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stub that dies at once is reported as such, naming the exit, and
    /// well inside the timeout.
    #[test]
    fn wait_for_stub_ready_reports_an_immediate_exit() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 3")
            .spawn()
            .expect("spawn short-lived child");
        let started = std::time::Instant::now();
        let err = super::wait_for_stub_ready(
            &mut child,
            "/nonexistent-aok-dir/s.sock",
            false,
            Duration::from_secs(5),
        )
        .expect_err("an exited stub is never ready");
        assert!(
            err.to_string().contains("exited"),
            "error must name the exit: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "must fail fast, not burn the timeout"
        );
        let _ = child.wait();
    }

    #[test]
    fn check_socket_path_enforces_the_sun_path_limit() {
        let ok = format!("/{}", "a".repeat(super::SUN_PATH_MAX - 1));
        assert_eq!(ok.len(), super::SUN_PATH_MAX);
        assert!(super::check_socket_path(&ok).is_ok());
        let long = format!("/{}", "a".repeat(super::SUN_PATH_MAX));
        let err = super::check_socket_path(&long).expect_err("108 bytes cannot be bound");
        assert!(err.to_string().contains("108"), "{err}");
        assert!(super::check_socket_path("").is_err());
        assert!(super::check_socket_path("/tmp/a\0b").is_err());
    }

    /// Only the attacker's own attempt is a void row; `HARNESS` and garbage
    /// are hard errors, and a void row carries the attempt's errno when one
    /// exists.
    #[test]
    fn parse_attempt_outcome_separates_attempt_from_harness() {
        assert!(matches!(
            super::parse_attempt_outcome("REACHABLE", ""),
            Ok(Ok(()))
        ));

        let void = super::parse_attempt_outcome("VOID errno=1 msg=Operation not permitted", "")
            .expect("VOID is an attempt result, not a harness failure")
            .expect_err("VOID is a failed attempt");
        assert_eq!(void.raw_os_error(), Some(1));

        let no_errno = super::parse_attempt_outcome("VOID errno=none msg=identity mismatch", "")
            .expect("VOID is an attempt result")
            .expect_err("VOID is a failed attempt");
        assert_eq!(no_errno.raw_os_error(), None);

        let harness = super::parse_attempt_outcome("HARNESS msg=cannot place helper", "")
            .expect_err("HARNESS is a harness failure");
        assert!(crate::routes::is_harness_error(&harness));
        assert!(harness.to_string().contains("cannot place helper"));

        let garbage =
            super::parse_attempt_outcome("", "boom").expect_err("no verdict is a harness failure");
        assert!(crate::routes::is_harness_error(&garbage));
    }
}
