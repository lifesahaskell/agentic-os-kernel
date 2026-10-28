//! The D-16 route table: kill, ptrace, bind and impersonation routes, plus
//! route (iv) (clone3 `CLONE_INTO_CGROUP`). Every route is enumerated by
//! `list-routes` and attempted non-agent (same uid) as a baseline before any
//! agent-cgroup run (08-05, R). A route is `reachable` when its non-agent
//! baseline attempt succeeds and `void (baseline failed)` otherwise (D-16).
//! Every target this module signals, traces, or kills is spawned and
//! pidfd-opened by this module first (D-13 safety): no route here ever
//! signals pid 0 or pid -1.

use crate::sys;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Families and topologies.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Kill,
    Ptrace,
    Bind,
    Impersonation,
}

impl Family {
    pub fn as_str(&self) -> &'static str {
        match self {
            Family::Kill => "kill",
            Family::Ptrace => "ptrace",
            Family::Bind => "bind",
            Family::Impersonation => "impersonation",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
    Sibling,
    Ancestor,
}

impl Topology {
    pub fn as_str(&self) -> &'static str {
        match self {
            Topology::Sibling => "sibling",
            Topology::Ancestor => "ancestor",
        }
    }
}

/// `reachable` or `void (baseline failed)` (D-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Reachable,
    VoidBaselineFailed,
}

impl Reach {
    #[allow(dead_code)] // used by Task 3's report.rs wiring (the reach field)
    pub fn as_str(&self) -> &'static str {
        match self {
            Reach::Reachable => "reachable",
            Reach::VoidBaselineFailed => "void (baseline failed)",
        }
    }
}

/// The outcome of attempting one route row: either it ran (and reached a
/// `Reach` verdict), or it was skipped because the host has no delegated
/// cgroup (`--no-cgroup`). Skipped is never `invalid` and never a failure
/// (D-16, ADV-02 empty case 29).
#[derive(Debug)]
#[allow(dead_code)] // used by Task 3's run_cli wiring (per-row run result)
pub enum RunOutcome {
    Ran(Reach),
    Skipped(&'static str),
}

#[allow(dead_code)] // used by Task 3's run_cli wiring (the --no-cgroup skip rule)
pub const SKIPPED_NEEDS_CGROUP: &str = "skipped: needs delegated cgroup";

/// Route names for the four kill-family rows that run in the sibling
/// topology only.
pub const KILL_ROUTES: &[&str] = &[
    "kill",
    "tgkill",
    "rt_sigqueueinfo",
    "pidfd_send_signal",
    "cgroup.kill",
];

/// The six ptrace-family routes. Yama gates all six (RESEARCH A5): each is
/// offered in both the sibling and the ancestor topology (12 rows total).
pub const PTRACE_ROUTES: &[&str] = &[
    "PTRACE_ATTACH",
    "PTRACE_SEIZE",
    "process_vm_readv",
    "process_vm_writev",
    "/proc/pid/mem",
    "pidfd_getfd",
];

/// The two bind-family routes, sibling topology only.
pub const BIND_ROUTES: &[&str] = &["pathname", "abstract"];

/// The four impersonation routes (D-15 i-iii, plus route iv), sibling
/// topology only.
pub const IMPERSONATION_ROUTES: &[&str] = &[
    "cgroup.procs",
    "scm_rights",
    "payload_claim",
    "clone3_CLONE_INTO_CGROUP",
];

/// The cgroup subtree every cgroup-bearing route is scoped to (T-08-05):
/// `cgroup.kill`, `cgroup.procs` and the route (iv) clone3 target must all
/// resolve under this prefix.
#[allow(dead_code)] // used by Task 3's run_cli wiring (the cgroup.kill guard)
pub const CGROUP_ADV_PREFIX: &str = "/sys/fs/cgroup/aok-phase8/adv/";

/// True for the three routes whose non-agent baseline needs a delegated
/// cgroup subtree (D-16; 08-03's `--adv` wrapper creates it). Every other
/// route runs with no cgroup dependency.
#[allow(dead_code)] // used by Task 3's run_cli wiring (the --no-cgroup skip rule)
pub fn needs_cgroup(route: &str) -> bool {
    matches!(
        route,
        "cgroup.kill" | "cgroup.procs" | "clone3_CLONE_INTO_CGROUP"
    )
}

/// One row of the route table: a family, a route name, and the topology it
/// was attempted under.
#[derive(Clone, Copy, Debug)]
pub struct RouteRow {
    pub family: Family,
    pub route: &'static str,
    pub topology: Topology,
}

/// The full 23-row table (D-16 plus route iv): 5 kill + 12 ptrace (6 routes
/// x 2 topologies) + 2 bind + 4 impersonation = 23; 17 sibling rows, 6
/// ancestor rows (the six ptrace routes only).
pub fn all_rows() -> Vec<RouteRow> {
    let mut rows = Vec::with_capacity(23);
    for route in KILL_ROUTES {
        rows.push(RouteRow {
            family: Family::Kill,
            route,
            topology: Topology::Sibling,
        });
    }
    for route in PTRACE_ROUTES {
        rows.push(RouteRow {
            family: Family::Ptrace,
            route,
            topology: Topology::Sibling,
        });
    }
    for route in PTRACE_ROUTES {
        rows.push(RouteRow {
            family: Family::Ptrace,
            route,
            topology: Topology::Ancestor,
        });
    }
    for route in BIND_ROUTES {
        rows.push(RouteRow {
            family: Family::Bind,
            route,
            topology: Topology::Sibling,
        });
    }
    for route in IMPERSONATION_ROUTES {
        rows.push(RouteRow {
            family: Family::Impersonation,
            route,
            topology: Topology::Sibling,
        });
    }
    rows
}

fn format_row(row: &RouteRow) -> String {
    format!(
        "{} {} {}",
        row.family.as_str(),
        row.route,
        row.topology.as_str()
    )
}

/// `aok-adv list-routes`: one `family route topology` line per row, no
/// header, 23 rows total.
pub fn list_routes_cli(_args: &[String]) -> io::Result<()> {
    for row in all_rows() {
        println!("{}", format_row(&row));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ptrace_scope: read once at run start, stamped on every row, never written
// (RESEARCH A5, ADV-02 concurrency 32).
// ---------------------------------------------------------------------------

/// Reads `/proc/sys/kernel/yama/ptrace_scope` once. Missing file (no Yama
/// LSM compiled in) is recorded as `"absent"`, never treated as an error.
#[allow(dead_code)] // used by Task 3's run_cli wiring (ptrace_scope stamping)
pub fn read_ptrace_scope() -> String {
    match std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope") {
        Ok(s) => s.trim().to_string(),
        Err(_) => "absent".to_string(),
    }
}

// ---------------------------------------------------------------------------
// The baseline-success/failure rule (D-16): a row is reachable exactly when
// its non-agent attempt succeeds, and void otherwise. --no-cgroup skips a
// cgroup-bearing row before any attempt is made.
// ---------------------------------------------------------------------------

/// Evaluate one row: skip it if it needs a cgroup this host does not have,
/// otherwise run `attempt` and map its result onto `Reach`. `attempt` is
/// injected so routes.rs's own tests can assert the skip and void-baseline
/// rules without performing a real syscall.
#[allow(dead_code)] // used by Task 3's run_cli wiring (baseline-to-reach mapping)
pub fn evaluate_row<F>(row: &RouteRow, no_cgroup: bool, attempt: F) -> RunOutcome
where
    F: FnOnce() -> io::Result<()>,
{
    if no_cgroup && needs_cgroup(row.route) {
        return RunOutcome::Skipped(SKIPPED_NEEDS_CGROUP);
    }
    match attempt() {
        Ok(()) => RunOutcome::Ran(Reach::Reachable),
        Err(_) => RunOutcome::Ran(Reach::VoidBaselineFailed),
    }
}

/// Reject a `cgroup.kill` (or any cgroup-write) target outside the suite's
/// own throwaway subtree (T-08-05): the write never reaches the session
/// cgroup or any cgroup this suite did not create.
#[allow(dead_code)] // used by Task 3's run_cli wiring (the cgroup.kill guard)
pub fn assert_cgroup_kill_path(path: &str) -> io::Result<()> {
    if !path.starts_with(CGROUP_ADV_PREFIX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cgroup.kill target {path} is outside {CGROUP_ADV_PREFIX}"),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Kill-family attempts (ADV-02). Every target below is spawned by the
// attempt itself and signalled only through a pidfd opened on that spawned
// child: never pid 0, never pid -1, never an existing unrelated process.
// ---------------------------------------------------------------------------

/// Spawn a throwaway `sleep` child this attempt owns for its own duration.
#[allow(dead_code)] // used by the attempt_* helpers below, via Task 3's run_cli
fn spawn_target() -> io::Result<std::process::Child> {
    Command::new("/bin/sh").arg("-c").arg("sleep 5").spawn()
}

/// `kill` route: `kill(pid, SIGKILL)` against a suite-spawned child,
/// verified by waiting for the child to exit.
#[allow(dead_code)] // used by Task 3's run_cli wiring (kill route)
pub fn attempt_kill() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    sys::kill(pid, libc::SIGKILL)?;
    let _ = child.wait();
    Ok(())
}

/// `tgkill` route: signal the suite-spawned child's own tgid/tid pair
/// (single-threaded, so tgid == tid == pid).
#[allow(dead_code)] // used by Task 3's run_cli wiring (tgkill route)
pub fn attempt_tgkill() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    sys::tgkill(pid, pid, libc::SIGKILL)?;
    let _ = child.wait();
    Ok(())
}

/// `rt_sigqueueinfo` route: queue `SIGKILL` at the suite-spawned child's
/// tgid.
#[allow(dead_code)] // used by Task 3's run_cli wiring (rt_sigqueueinfo route)
pub fn attempt_rt_sigqueueinfo() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    sys::rt_sigqueueinfo(pid, libc::SIGKILL)?;
    let _ = child.wait();
    Ok(())
}

/// `pidfd_send_signal` route: open a pidfd on the suite-spawned child and
/// signal through it, verified by polling the pidfd for exit (ADV-02).
#[allow(dead_code)] // used by Task 3's run_cli wiring (pidfd_send_signal route)
pub fn attempt_pidfd_send_signal() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    let pidfd = sys::pidfd_open(pid, 0)?;
    let result = sys::pidfd_send_signal(pidfd, libc::SIGKILL).and_then(|()| {
        let exited = sys::poll_readable(pidfd, Duration::from_secs(5))?;
        if exited {
            Ok(())
        } else {
            Err(io::Error::other("pidfd never reported exit"))
        }
    });
    sys::close_fd(pidfd);
    let _ = child.wait();
    result
}

/// `cgroup.kill` route: write `1` to `<leaf>/cgroup.kill`, guarded by
/// `assert_cgroup_kill_path` so the write can only ever land under this
/// suite's own subtree.
#[allow(dead_code)] // used by Task 3's run_cli wiring (cgroup.kill route)
pub fn attempt_cgroup_kill(leaf_dir: &str) -> io::Result<()> {
    let target = format!("{leaf_dir}/cgroup.kill");
    assert_cgroup_kill_path(&target)?;
    std::fs::write(&target, "1")
}

// ---------------------------------------------------------------------------
// Ptrace-family attempts (ADV-02; Yama-gated, RESEARCH A5/Pitfall 4).
// ---------------------------------------------------------------------------

/// `PTRACE_ATTACH` then detach, against a suite-spawned child.
#[allow(dead_code)] // used by Task 3's run_cli wiring (PTRACE_ATTACH route)
pub fn attempt_ptrace_attach() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    let result = sys::ptrace_attach(pid);
    let _ = sys::ptrace_detach(pid);
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// `PTRACE_SEIZE` then detach, against a suite-spawned child.
#[allow(dead_code)] // used by Task 3's run_cli wiring (PTRACE_SEIZE route)
pub fn attempt_ptrace_seize() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    let result = sys::ptrace_seize(pid);
    let _ = sys::ptrace_detach(pid);
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// `process_vm_readv`: read at least one byte from a suite-spawned child.
/// Without a ptrace relationship this is expected to fail same-uid under
/// Yama scope >= 1 (RESEARCH A5); failure here is a baseline void, not a
/// bug.
#[allow(dead_code)] // used by Task 3's run_cli wiring (process_vm_readv route)
pub fn attempt_process_vm_readv() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    let mut buf = [0u8; 1];
    let result = sys::process_vm_readv(pid, &mut buf, 0).map(|_| ());
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// `process_vm_writev`: write one byte back into a suite-spawned child's
/// address space.
#[allow(dead_code)] // used by Task 3's run_cli wiring (process_vm_writev route)
pub fn attempt_process_vm_writev() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    let buf = [0u8; 1];
    let result = sys::process_vm_writev(pid, &buf, 0).map(|_| ());
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// `/proc/<pid>/mem`: open and read one byte from a suite-spawned child.
#[allow(dead_code)] // used by Task 3's run_cli wiring (/proc/pid/mem route)
pub fn attempt_proc_pid_mem() -> io::Result<()> {
    use std::io::Read;
    let mut child = spawn_target()?;
    let pid = child.id();
    let result = (|| -> io::Result<()> {
        let mut f = std::fs::File::open(format!("/proc/{pid}/mem"))?;
        let mut buf = [0u8; 1];
        f.read_exact(&mut buf)?;
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// `pidfd_getfd`: obtain the suite-spawned child's fd 0 through its pidfd.
#[allow(dead_code)] // used by Task 3's run_cli wiring (pidfd_getfd route)
pub fn attempt_pidfd_getfd() -> io::Result<()> {
    let mut child = spawn_target()?;
    let pid = child.id() as libc::pid_t;
    let pidfd = sys::pidfd_open(pid, 0)?;
    let result = sys::pidfd_getfd(pidfd, 0).map(sys::close_fd);
    sys::close_fd(pidfd);
    let _ = child.kill();
    let _ = child.wait();
    result
}

// ---------------------------------------------------------------------------
// Bind-family attempts (ADV-02): bind the broker's socket name out from
// under it.
// ---------------------------------------------------------------------------

/// `pathname` route: `unlink` then `bind` the broker's pathname socket while
/// nothing else holds it, oracle being that a fresh client reaches this
/// listener instead of the broker.
#[allow(dead_code)] // used by Task 3's run_cli wiring (bind pathname route)
pub fn attempt_bind_pathname(socket_path: &str) -> io::Result<()> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    drop(listener);
    Ok(())
}

/// `abstract` route: bind the broker's abstract-namespace name during the
/// restart gap between stub instances.
#[allow(dead_code)] // used by Task 3's run_cli wiring (bind abstract route)
pub fn attempt_bind_abstract(name: &str) -> io::Result<()> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::SocketAddr;
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    let listener = UnixListener::bind_addr(&addr)?;
    drop(listener);
    Ok(())
}

// ---------------------------------------------------------------------------
// Impersonation attempts (D-15 i-iii, route iv).
// ---------------------------------------------------------------------------

/// Route (i): move this process's own pid into another agent's cgroup via
/// `cgroup.procs`, guarded by `assert_cgroup_kill_path`'s prefix rule (the
/// same subtree boundary applies to every cgroup write this suite makes).
#[allow(dead_code)] // used by Task 3's run_cli wiring (cgroup.procs route)
pub fn attempt_cgroup_procs_move(target_cgroup_dir: &str) -> io::Result<()> {
    let target = format!("{target_cgroup_dir}/cgroup.procs");
    assert_cgroup_kill_path(&target)?;
    std::fs::write(&target, "0")
}

/// Route (ii): receive a connected fd over `SCM_RIGHTS` from a peer and use
/// it, simulated here with a suite-owned socket pair standing in for
/// agent-a and agent-b.
#[allow(dead_code)] // used by Task 3's run_cli wiring (scm_rights route)
pub fn attempt_scm_rights() -> io::Result<()> {
    let (a, b) = UnixStream::pair()?;
    let send_fd: std::os::fd::RawFd = std::os::fd::AsRawFd::as_raw_fd(&a);
    sys::send_fd(std::os::fd::AsRawFd::as_raw_fd(&b), send_fd)?;
    let received = sys::recv_fd(std::os::fd::AsRawFd::as_raw_fd(&a))?;
    sys::close_fd(received);
    Ok(())
}

/// Route (iii): claim another id in the `HELLO` payload. This is the D-15
/// control that must be ignored, so success here means the stub's reply
/// still carries this process's own kernel-stamped identity, not the
/// claimed one.
#[allow(dead_code)] // used by Task 3's run_cli wiring (payload_claim route)
pub fn attempt_payload_claim(socket_path: &str, claimed_id: &str) -> io::Result<String> {
    use std::io::{Read, Write};
    let mut stream = UnixStream::connect(socket_path)?;
    let request = format!("HELLO {claimed_id}");
    let len = (request.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(request.as_bytes())?;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let reply_len = u32::from_le_bytes(len_buf) as usize;
    let mut reply = vec![0u8; reply_len];
    stream.read_exact(&mut reply)?;
    Ok(String::from_utf8_lossy(&reply).into_owned())
}

/// Route (iv): `clone3` with `CLONE_INTO_CGROUP`, placing a new child
/// directly into another agent's cgroup, gated only by
/// `cgroup_may_write` on the reference host.
#[allow(dead_code)] // used by Task 3's run_cli wiring (clone3_CLONE_INTO_CGROUP route)
pub fn attempt_clone3_into_cgroup(target_cgroup_fd: std::os::fd::RawFd) -> io::Result<()> {
    match sys::clone3_fork_into_cgroup(target_cgroup_fd)? {
        sys::ForkResult::Parent {
            child_pid,
            child_pidfd,
        } => {
            let _ = sys::pidfd_send_signal(child_pidfd, libc::SIGKILL);
            sys::close_fd(child_pidfd);
            let _ = sys::waitpid_reap(child_pid);
            Ok(())
        }
        sys::ForkResult::Child => {
            // SAFETY-relevant note: the child side must not run any of this
            // process's normal control flow; it exits immediately.
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_rows_has_23_entries_17_sibling_6_ancestor() {
        let rows = all_rows();
        assert_eq!(rows.len(), 23);
        let sibling = rows
            .iter()
            .filter(|r| r.topology == Topology::Sibling)
            .count();
        let ancestor = rows
            .iter()
            .filter(|r| r.topology == Topology::Ancestor)
            .count();
        assert_eq!(sibling, 17);
        assert_eq!(ancestor, 6);
    }

    #[test]
    fn exactly_the_six_ptrace_routes_carry_an_ancestor_row() {
        let rows = all_rows();
        let ancestor_routes: Vec<&str> = rows
            .iter()
            .filter(|r| r.topology == Topology::Ancestor)
            .map(|r| r.route)
            .collect();
        assert_eq!(ancestor_routes.len(), 6);
        for route in &ancestor_routes {
            assert!(
                PTRACE_ROUTES.contains(route),
                "ancestor row for non-ptrace route {route}"
            );
            assert!(
                rows.iter()
                    .any(|r| r.family == Family::Ptrace && r.route == *route),
                "ancestor route {route} is not in the ptrace family"
            );
        }
        for route in PTRACE_ROUTES {
            assert!(
                ancestor_routes.contains(route),
                "ptrace route {route} is missing its ancestor row"
            );
        }
    }

    #[test]
    fn list_routes_output_names_every_route_including_clone3() {
        let rows = all_rows();
        let text: String = rows.iter().map(format_row).collect::<Vec<_>>().join("\n");
        for route in KILL_ROUTES
            .iter()
            .chain(PTRACE_ROUTES.iter())
            .chain(BIND_ROUTES.iter())
            .chain(IMPERSONATION_ROUTES.iter())
        {
            assert!(text.contains(route), "list-routes output missing {route}");
        }
        assert!(text.contains("CLONE_INTO_CGROUP"));
    }

    #[test]
    fn no_cgroup_marks_cgroup_routes_skipped() {
        let row = RouteRow {
            family: Family::Kill,
            route: "cgroup.kill",
            topology: Topology::Sibling,
        };
        let outcome = evaluate_row(&row, true, || Ok(()));
        match outcome {
            RunOutcome::Skipped(msg) => assert_eq!(msg, SKIPPED_NEEDS_CGROUP),
            other => panic!("expected Skipped, got {other:?}"),
        }
    }

    #[test]
    fn non_cgroup_route_is_not_skipped_under_no_cgroup() {
        let row = RouteRow {
            family: Family::Kill,
            route: "kill",
            topology: Topology::Sibling,
        };
        let outcome = evaluate_row(&row, true, || Ok(()));
        match outcome {
            RunOutcome::Ran(Reach::Reachable) => {}
            other => panic!("expected Ran(Reachable), got {other:?}"),
        }
    }

    #[test]
    fn failed_baseline_sets_reach_to_void() {
        let row = RouteRow {
            family: Family::Ptrace,
            route: "PTRACE_ATTACH",
            topology: Topology::Sibling,
        };
        let outcome = evaluate_row(&row, false, || {
            Err(io::Error::other("simulated baseline failure"))
        });
        match outcome {
            RunOutcome::Ran(Reach::VoidBaselineFailed) => {}
            other => panic!("expected Ran(VoidBaselineFailed), got {other:?}"),
        }
    }

    #[test]
    fn succeeding_baseline_is_reachable() {
        let row = RouteRow {
            family: Family::Bind,
            route: "pathname",
            topology: Topology::Sibling,
        };
        let outcome = evaluate_row(&row, false, || Ok(()));
        match outcome {
            RunOutcome::Ran(Reach::Reachable) => {}
            other => panic!("expected Ran(Reachable), got {other:?}"),
        }
    }

    #[test]
    fn cgroup_kill_guard_rejects_path_outside_adv_prefix() {
        assert!(assert_cgroup_kill_path("/sys/fs/cgroup/other/leaf").is_err());
        assert!(assert_cgroup_kill_path("/tmp/not-a-cgroup").is_err());
    }

    #[test]
    fn cgroup_kill_guard_accepts_path_inside_adv_prefix() {
        assert!(assert_cgroup_kill_path("/sys/fs/cgroup/aok-phase8/adv/agent-a/leaf").is_ok());
    }

    #[test]
    fn kill_route_runs_against_a_suite_spawned_child_only() {
        attempt_kill().expect("kill route baseline against a suite-spawned child");
    }

    #[test]
    fn bind_pathname_route_binds_and_releases_a_fresh_socket() {
        let dir = std::env::temp_dir().join(format!("aok-adv-bind-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("stub.sock");
        attempt_bind_pathname(path.to_str().unwrap()).expect("bind pathname route");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
