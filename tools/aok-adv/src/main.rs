#![deny(unsafe_code)]
//! aok-adv: the ADV-01/02/03 adversarial suite and stub broker (08-02).
//! Every raw syscall lives in `sys` (D-13); every other module is safe Rust.
//! Subcommands: `stub`, `run`, `list-routes`, `report` (run/list-routes/
//! report are fleshed out across Tasks 1-3 of this plan).

mod cases;
mod report;
mod routes;
mod stub;
#[allow(unsafe_code)]
mod sys;

use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::ExitCode;

fn print_usage() {
    eprintln!("usage: aok-adv <stub|run|list-routes|report> [args...]");
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.collect();
    let result = match cmd.as_str() {
        "stub" => stub::run_cli(&rest),
        "list-routes" => list_routes_cli(&rest),
        "run" => run_cli(&rest),
        "report" => report_cli(&rest),
        other => {
            eprintln!("aok-adv: unknown subcommand '{other}'");
            print_usage();
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aok-adv: {e}");
            ExitCode::FAILURE
        }
    }
}

fn list_routes_cli(args: &[String]) -> std::io::Result<()> {
    routes::list_routes_cli(args)
}

/// Default `--cgroup-root`: the throwaway subtree 08-03's `--adv` wrapper
/// creates (D-16); never written to unless a route explicitly needs it.
const DEFAULT_CGROUP_ROOT: &str = "/sys/fs/cgroup/aok-phase8/adv";

/// Parsed `run` flags (D-16/ADV-01..03): `--out` is the only required flag.
/// `--routes`/`--topology`/`--cases` are optional filters (comma-separated
/// for the first and third); omitting `--socket` skips route evaluation
/// entirely (no route row is attempted without a broker to attempt it
/// against), while the S1 case suite always runs.
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

fn require_value(args: &[String], i: usize, flag: &str) -> io::Result<String> {
    args.get(i).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{flag} requires a value"),
        )
    })
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
fn dispatch_attempt(row: &routes::RouteRow, socket: &str, cgroup_root: &str) -> io::Result<()> {
    match row.route {
        "kill" => routes::attempt_kill(),
        "tgkill" => routes::attempt_tgkill(),
        "rt_sigqueueinfo" => routes::attempt_rt_sigqueueinfo(),
        "pidfd_send_signal" => routes::attempt_pidfd_send_signal(),
        "cgroup.kill" => routes::attempt_cgroup_kill(cgroup_root),
        "PTRACE_ATTACH" => routes::attempt_ptrace_attach(),
        "PTRACE_SEIZE" => routes::attempt_ptrace_seize(),
        "process_vm_readv" => routes::attempt_process_vm_readv(),
        "process_vm_writev" => routes::attempt_process_vm_writev(),
        "/proc/pid/mem" => routes::attempt_proc_pid_mem(),
        "pidfd_getfd" => routes::attempt_pidfd_getfd(),
        "pathname" => routes::attempt_bind_pathname(socket),
        "abstract" => routes::attempt_bind_abstract(socket),
        "cgroup.procs" => routes::attempt_cgroup_procs_move(cgroup_root),
        "scm_rights" => routes::attempt_scm_rights(),
        "payload_claim" => routes::attempt_payload_claim(socket, "impersonated-id").map(|_| ()),
        "clone3_CLONE_INTO_CGROUP" => {
            let dir = std::fs::File::open(cgroup_root)?;
            routes::attempt_clone3_into_cgroup(dir.as_raw_fd())
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("run: unknown route {other}"),
        )),
    }
}

/// `aok-adv run`: the S1 case suite always runs; route rows run only when
/// `--socket` names a live broker to attempt them against. Cgroup-bearing
/// routes still go through `routes::evaluate_row`'s own `--no-cgroup` skip
/// (D-16) regardless of this flag's default here.
fn run_cli(args: &[String]) -> io::Result<()> {
    let parsed = parse_run_args(args)?;

    if let Some(pid) = parsed.broker_pid {
        // Liveness only (deviation): not cross-checked against the
        // socket's actual peer credentials.
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("--broker-pid {pid}: no such process"),
            ));
        }
    }

    let ptrace_scope = routes::read_ptrace_scope();
    let runid = std::process::id().to_string();
    let work_dir = std::env::temp_dir().join(format!("aok-adv-run-{runid}"));
    std::fs::create_dir_all(&work_dir)?;

    let mut records = Vec::new();

    for outcome in cases::run_all_cases(&work_dir, &runid) {
        if let Some(filter) = &parsed.cases
            && !filter.iter().any(|c| c.as_str() == outcome.spec.class)
        {
            continue;
        }
        records.push(report::Record::from_case(
            outcome.spec,
            outcome.baseline_ok,
            &parsed.mode,
            &ptrace_scope,
        ));
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
            let outcome = routes::evaluate_row(&row, parsed.no_cgroup, || {
                dispatch_attempt(&row, socket, &parsed.cgroup_root)
            });
            records.push(report::Record::from_route(
                &row,
                &outcome,
                &parsed.mode,
                &ptrace_scope,
            ));
        }
    }

    report::write_jsonl(&records, Path::new(&parsed.out))?;
    let _ = std::fs::remove_dir_all(&work_dir);
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
}
