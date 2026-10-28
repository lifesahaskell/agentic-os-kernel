//! The D-16 route table: kill, ptrace, bind and impersonation routes, plus
//! route (iv) (clone3 `CLONE_INTO_CGROUP`). Every route is enumerated by
//! `list-routes` and attempted non-agent (same uid) as a baseline before any
//! agent-cgroup run (08-05, R). A route is `reachable` when its non-agent
//! baseline attempt succeeds and `void (baseline failed)` otherwise (D-16).
//!
//! Every `attempt_*` function below takes its target as a parameter (D1,
//! D-14): none of them spawns a throwaway child to attack. `main.rs` owns
//! the stub broker's lifecycle and the attacker/helper process topology
//! (D2/D3); this module only runs the one syscall sequence a route needs
//! against the `Target` it is handed.

use crate::sys;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
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
pub enum RunOutcome {
    Ran(Reach),
    Skipped(&'static str),
}

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
pub const CGROUP_ADV_PREFIX: &str = "/sys/fs/cgroup/aok-phase8/adv/";

/// True for the three routes whose non-agent baseline needs a delegated
/// cgroup subtree (D-16; 08-03's `--adv` wrapper creates it). Every other
/// route runs with no cgroup dependency.
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
#[allow(dead_code)] // kept as a convenience wrapper this module's own tests use directly
pub fn evaluate_row<F>(row: &RouteRow, no_cgroup: bool, attempt: F) -> RunOutcome
where
    F: FnOnce() -> io::Result<()>,
{
    let (outcome, _errno) = evaluate_row_with_errno(row, no_cgroup, attempt);
    outcome
}

/// Same rule as `evaluate_row`, but also surfaces the failing attempt's
/// `errno` (when the underlying `io::Error` carries one) so the caller can
/// record it on the row's `Record` (D5: "failures are recorded with
/// errno").
pub fn evaluate_row_with_errno<F>(
    row: &RouteRow,
    no_cgroup: bool,
    attempt: F,
) -> (RunOutcome, Option<i64>)
where
    F: FnOnce() -> io::Result<()>,
{
    if no_cgroup && needs_cgroup(row.route) {
        return (RunOutcome::Skipped(SKIPPED_NEEDS_CGROUP), None);
    }
    match attempt() {
        Ok(()) => (RunOutcome::Ran(Reach::Reachable), None),
        Err(e) => {
            let errno = e.raw_os_error().map(i64::from);
            (RunOutcome::Ran(Reach::VoidBaselineFailed), errno)
        }
    }
}

/// Reject a cgroup-write target outside the suite's own throwaway subtree
/// (T-08-05): the write never reaches the session cgroup or any cgroup this
/// suite did not create. Guards every `cgroup.kill`, `cgroup.procs` and
/// placement write this module makes.
pub fn assert_cgroup_kill_path(path: &str) -> io::Result<()> {
    if !path.starts_with(CGROUP_ADV_PREFIX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cgroup write target {path} is outside {CGROUP_ADV_PREFIX}"),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Harness failures versus attempt failures. Only the attacker's own attempt
// may yield a void row; anything that stops the suite from setting the
// attempt up (stub spawn, stub readiness, placement, helper spawn) is a
// `HarnessError` and makes `run` exit non-zero with no row written.
// ---------------------------------------------------------------------------

/// Marker payload carried inside an `io::Error` for a harness failure.
#[derive(Debug)]
pub struct HarnessError(pub String);

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HarnessError {}

/// Build an `io::Error` that `is_harness_error` recognises.
pub fn harness_error(msg: impl Into<String>) -> io::Error {
    io::Error::other(HarnessError(msg.into()))
}

/// True when `e` came from `harness_error` (not from the attempt itself).
pub fn is_harness_error(e: &io::Error) -> bool {
    e.get_ref()
        .is_some_and(|inner| inner.downcast_ref::<HarnessError>().is_some())
}

/// Move `pid` into `leaf_dir`'s `cgroup.procs` (D4 agent-mode placement:
/// attacker into agent-a, stub into broker, helper into agent-b). Guarded
/// by `assert_cgroup_kill_path` like every other cgroup write this suite
/// makes.
pub fn place_in_cgroup(pid: libc::pid_t, leaf_dir: &str) -> io::Result<()> {
    let target = format!("{leaf_dir}/cgroup.procs");
    assert_cgroup_kill_path(&target)?;
    write_procs_file(&target, pid)
}

/// The raw write `place_in_cgroup` performs once its path guard has passed,
/// split out so this module's own tests can exercise the write mechanics
/// (D4) against an injected, non-prefixed tempdir standing in for a
/// delegated cgroup leaf, without needing a real `/sys/fs/cgroup` mount or
/// re-litigating `assert_cgroup_kill_path`'s own prefix tests.
fn write_procs_file(target: &str, pid: libc::pid_t) -> io::Result<()> {
    std::fs::write(target, pid.to_string())
}

// ---------------------------------------------------------------------------
// Target: what an attempt_* function attacks. The runner (main.rs) owns
// spawning and killing whatever process backs `pid`/`socket_path`; no
// function below spawns its own throwaway target (D1).
// ---------------------------------------------------------------------------

/// The broker (or suite-spawned stand-in) this route attacks: a pid for the
/// kill/ptrace families, a socket address for the bind/impersonation
/// families. Not every field is meaningful for every route.
pub struct Target {
    pub pid: libc::pid_t,
    pub socket_path: String,
    pub abstract_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Kill-family attempts (ADV-02). Every target is owned and spawned by the
// caller (main.rs); these functions only ever signal `target.pid`, never
// pid 0 or pid -1 (D-13 safety, enforced again inside `sys::kill`/`tgkill`).
// ---------------------------------------------------------------------------

/// `kill` route: `kill(pid, SIGKILL)` against the given target.
pub fn attempt_kill(target: &Target) -> io::Result<()> {
    sys::kill(target.pid, libc::SIGKILL)
}

/// `tgkill` route: signal the target's own tgid/tid pair (single-threaded,
/// so tgid == tid == pid).
pub fn attempt_tgkill(target: &Target) -> io::Result<()> {
    sys::tgkill(target.pid, target.pid, libc::SIGKILL)
}

/// `rt_sigqueueinfo` route: queue `SIGKILL` at the target's tgid.
pub fn attempt_rt_sigqueueinfo(target: &Target) -> io::Result<()> {
    sys::rt_sigqueueinfo(target.pid, libc::SIGKILL)
}

/// `pidfd_send_signal` route: open a pidfd on the target and signal through
/// it, verified by polling the pidfd for exit (ADV-02).
pub fn attempt_pidfd_send_signal(target: &Target) -> io::Result<()> {
    let pidfd = sys::pidfd_open(target.pid, 0)?;
    let result = sys::pidfd_send_signal(pidfd, libc::SIGKILL).and_then(|()| {
        let exited = sys::poll_readable(pidfd, Duration::from_secs(5))?;
        if exited {
            Ok(())
        } else {
            Err(io::Error::other("pidfd never reported exit"))
        }
    });
    sys::close_fd(pidfd);
    result
}

/// `cgroup.kill` route: write `1` to `<leaf>/cgroup.kill`, guarded by
/// `assert_cgroup_kill_path` so the write can only ever land under this
/// suite's own subtree. `leaf_dir` must be the leaf the target (the stub)
/// was actually placed in, or this kills nothing meaningful (D-16).
pub fn attempt_cgroup_kill(leaf_dir: &str) -> io::Result<()> {
    let target = format!("{leaf_dir}/cgroup.kill");
    assert_cgroup_kill_path(&target)?;
    std::fs::write(&target, "1")
}

// ---------------------------------------------------------------------------
// Ptrace-family attempts (ADV-02; Yama-gated, RESEARCH A5/Pitfall 4).
// ---------------------------------------------------------------------------

/// Find the start address of the first `/proc/<pid>/maps` region whose
/// permission string contains `perm` (e.g. `'r'`, `'w'`). Used so
/// `process_vm_readv`/`writev`/`/proc/pid/mem` attempt a real, mapped
/// address in the target rather than address 0 (which would fail with
/// EFAULT even when the permission check that matters here would have
/// allowed the access, masking the Yama verdict this route exists to
/// measure).
fn first_region_addr(pid: libc::pid_t, perm: char) -> io::Result<u64> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps"))?;
    for line in maps.lines() {
        let mut parts = line.split_whitespace();
        let range = parts.next().unwrap_or("");
        let perms = parts.next().unwrap_or("");
        if perms.contains(perm)
            && let Some((start, _end)) = range.split_once('-')
            && let Ok(addr) = u64::from_str_radix(start, 16)
        {
            return Ok(addr);
        }
    }
    Err(io::Error::other(format!(
        "no '{perm}' region found in /proc/{pid}/maps"
    )))
}

/// `PTRACE_ATTACH` then detach, against `target.pid`.
pub fn attempt_ptrace_attach(target: &Target) -> io::Result<()> {
    let result = sys::ptrace_attach(target.pid);
    let _ = sys::ptrace_detach(target.pid);
    result
}

/// `PTRACE_SEIZE` then detach, against `target.pid`.
pub fn attempt_ptrace_seize(target: &Target) -> io::Result<()> {
    let result = sys::ptrace_seize(target.pid);
    let _ = sys::ptrace_detach(target.pid);
    result
}

/// `process_vm_readv`: read one byte from a real mapped region of
/// `target.pid`'s address space. Without a ptrace relationship this is
/// expected to fail same-uid under Yama scope >= 1 (RESEARCH A5); failure
/// here is a baseline void, not a bug.
pub fn attempt_process_vm_readv(target: &Target) -> io::Result<()> {
    let addr = first_region_addr(target.pid, 'r')?;
    let mut buf = [0u8; 1];
    sys::process_vm_readv(target.pid, &mut buf, addr).map(|_| ())
}

/// `process_vm_writev`: read then write back the same byte at a writable
/// mapped region of `target.pid`'s address space (a no-op write that still
/// exercises the route without corrupting the target).
pub fn attempt_process_vm_writev(target: &Target) -> io::Result<()> {
    let addr = first_region_addr(target.pid, 'w')?;
    let mut buf = [0u8; 1];
    sys::process_vm_readv(target.pid, &mut buf, addr)?;
    sys::process_vm_writev(target.pid, &buf, addr).map(|_| ())
}

/// `/proc/<pid>/mem`: open and read one byte at a real mapped address in
/// `target.pid`.
pub fn attempt_proc_pid_mem(target: &Target) -> io::Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    let addr = first_region_addr(target.pid, 'r')?;
    let mut f = std::fs::File::open(format!("/proc/{}/mem", target.pid))?;
    f.seek(SeekFrom::Start(addr))?;
    let mut buf = [0u8; 1];
    f.read_exact(&mut buf)?;
    Ok(())
}

/// `pidfd_getfd`: obtain `target.pid`'s fd 0 through its pidfd.
pub fn attempt_pidfd_getfd(target: &Target) -> io::Result<()> {
    let pidfd = sys::pidfd_open(target.pid, 0)?;
    let result = sys::pidfd_getfd(pidfd, 0).map(sys::close_fd);
    sys::close_fd(pidfd);
    result
}

// ---------------------------------------------------------------------------
// Bind-family attempts (ADV-02): bind the broker's socket name out from
// under it, then prove a fresh client reaches the attacker's listener
// instead of the broker's (D5).
// ---------------------------------------------------------------------------

/// Accept exactly one connection on `listener` and write a fixed marker, so
/// the caller's fresh connect can tell whether it reached this listener
/// (the attacker) or the original broker.
fn serve_one_marker(listener: UnixListener) -> std::thread::JoinHandle<io::Result<()>> {
    use std::io::Write;
    std::thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        stream.write_all(b"ATTACKER")
    })
}

fn confirm_marker_reached(
    socket_path_connect: impl Fn() -> io::Result<UnixStream>,
) -> io::Result<()> {
    use std::io::Read;
    let mut client = socket_path_connect()?;
    let mut buf = [0u8; 8];
    client.read_exact(&mut buf)?;
    if &buf == b"ATTACKER" {
        Ok(())
    } else {
        Err(io::Error::other(
            "fresh connect did not reach attacker listener",
        ))
    }
}

/// `pathname` route: `unlink` then `bind` the broker's pathname socket
/// while the broker (stub) is still alive (D5), oracle being that a fresh
/// client reaches this listener instead of the broker's.
pub fn attempt_bind_pathname(target: &Target) -> io::Result<()> {
    let _ = std::fs::remove_file(&target.socket_path);
    let listener = UnixListener::bind(&target.socket_path)?;
    let accept_thread = serve_one_marker(listener);
    let path = target.socket_path.clone();
    // The accept thread is still blocked in accept() on a confirm failure;
    // joining it would hang the attacker. It dies with this process instead.
    confirm_marker_reached(move || UnixStream::connect(&path))?;
    accept_thread
        .join()
        .map_err(|_| io::Error::other("accept thread panicked"))?
}

/// `abstract` route: bind the broker's abstract-namespace name during the
/// restart gap between stub instances (the caller, main.rs, kills the live
/// stub before calling this and starts a fresh one afterward, D5).
pub fn attempt_bind_abstract(target: &Target) -> io::Result<()> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::SocketAddr;
    let name = target.abstract_name.as_deref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "abstract route requires an abstract name",
        )
    })?;
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    let listener = UnixListener::bind_addr(&addr)?;
    let accept_thread = serve_one_marker(listener);
    let name = name.to_string();
    confirm_marker_reached(move || {
        let connect_addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        UnixStream::connect_addr(&connect_addr)
    })?;
    accept_thread
        .join()
        .map_err(|_| io::Error::other("accept thread panicked"))?
}

// ---------------------------------------------------------------------------
// Impersonation attempts (D-15 i-iii, route iv).
// ---------------------------------------------------------------------------

/// Send one `HELLO <claimed_id>` request/reply round trip on an already
/// connected stream.
fn send_hello(stream: &mut UnixStream, claimed_id: &str) -> io::Result<String> {
    use std::io::{Read, Write};
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

/// Parse the `cgroupid=<u64>` field out of a stub reply of the form
/// `OK cgroupid=<n> pid=<m>`.
fn parse_cgroupid(reply: &str) -> io::Result<u64> {
    reply
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("cgroupid="))
        .ok_or_else(|| io::Error::other(format!("reply missing cgroupid field: {reply}")))?
        .parse::<u64>()
        .map_err(|e| io::Error::other(format!("bad cgroupid in reply {reply}: {e}")))
}

/// This process's own kernel-stamped cgroup id, read the same way the stub
/// reads a peer's: via `/proc/self/cgroup`'s single cgroup v2 line, then
/// `stat`ing the corresponding `/sys/fs/cgroup/<path>` directory for its
/// inode (the cgroup id).
fn own_cgroup_id() -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    let content = std::fs::read_to_string("/proc/self/cgroup")?;
    let line = content
        .lines()
        .next()
        .ok_or_else(|| io::Error::other("empty /proc/self/cgroup"))?;
    let path = line
        .splitn(3, ':')
        .nth(2)
        .ok_or_else(|| io::Error::other(format!("malformed /proc/self/cgroup line: {line}")))?;
    let full = format!("/sys/fs/cgroup{path}");
    Ok(std::fs::metadata(full)?.ino())
}

/// Route (i): move this process's own pid into another agent's cgroup via
/// `cgroup.procs`, then prove the move landed by asking the stub for the
/// kernel-stamped identity it now sees on this connection (D5): the reply's
/// `cgroupid` must equal `agent_b_dir`'s own id.
pub fn attempt_cgroup_procs_move(agent_b_dir: &str, socket_path: &str) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let procs = format!("{agent_b_dir}/cgroup.procs");
    assert_cgroup_kill_path(&procs)?;
    std::fs::write(&procs, "0")?;
    let want = std::fs::metadata(agent_b_dir)?.ino();
    let mut stream = UnixStream::connect(socket_path)?;
    let reply = send_hello(&mut stream, "self")?;
    let got = parse_cgroupid(&reply)?;
    if got == want {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "cgroup.procs move: stub saw cgroupid {got}, expected agent-b id {want}"
        )))
    }
}

/// Wait (bounded) for the scm-rights helper to connect to the handoff
/// listener, failing early if the helper dies first (for example because its
/// own connect to the stub was refused: an attempt failure, not a harness
/// one) instead of blocking in `accept` forever.
fn accept_handoff(
    listener: &UnixListener,
    helper: &mut std::process::Child,
) -> io::Result<UnixStream> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if let Some(status) = helper.try_wait()? {
                    return Err(io::Error::other(format!(
                        "scm_rights helper exited ({status}) before handing off a connected fd"
                    )));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "scm_rights helper never handed off a connected fd",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Route (ii): a real helper process connects to the stub at `socket_path`
/// and hands its now-connected fd to this process over a side-channel
/// `SCM_RIGHTS` handoff (D5). This process then speaks to the stub through
/// that borrowed fd, so the identity the stub stamps on the reply is the
/// helper's, not this process's own. When `agent_b_dir` is given (agent
/// mode), the helper is first placed in that cgroup and the reply's
/// `cgroupid` is checked against it; otherwise success just means a real
/// borrowed, connected fd was obtained and used.
pub fn attempt_scm_rights(socket_path: &str, agent_b_dir: Option<&str>) -> io::Result<()> {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    use std::process::{Command, Stdio};

    let setup = |what: &str, e: io::Error| harness_error(format!("scm_rights: {what}: {e}"));

    let handoff_dir = std::env::temp_dir().join(format!("aok-adv-scm-{}", std::process::id()));
    std::fs::create_dir_all(&handoff_dir).map_err(|e| setup("creating handoff dir", e))?;
    let handoff_path = handoff_dir.join("handoff.sock");
    let _ = std::fs::remove_file(&handoff_path);
    let listener =
        UnixListener::bind(&handoff_path).map_err(|e| setup("binding handoff socket", e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| setup("handoff socket nonblocking", e))?;

    let exe = std::env::current_exe().map_err(|e| setup("locating own executable", e))?;
    let mut cmd = Command::new(exe);
    cmd.arg("scm-helper")
        .arg("--socket")
        .arg(socket_path)
        .arg("--handoff")
        .arg(&handoff_path)
        .stdin(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| setup("spawning helper", e))?;

    // The helper is placed before it is told to connect, so the identity the
    // stub reads for it is the placed one, never a race against placement.
    if let Some(dir) = agent_b_dir
        && let Err(e) = place_in_cgroup(child.id() as libc::pid_t, dir)
    {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&handoff_dir);
        return Err(setup(&format!("placing helper in {dir}"), e));
    }
    let mut helper_stdin = child.stdin.take();
    if let Some(stdin) = helper_stdin.as_mut() {
        let _ = stdin.write_all(b"go\n");
    }

    let result = (|| -> io::Result<u64> {
        let handoff_stream = accept_handoff(&listener, &mut child)?;
        let borrowed_fd = sys::recv_fd(handoff_stream.as_raw_fd())?;
        let mut borrowed = UnixStream::from(sys::owned_fd(borrowed_fd));
        let reply = send_hello(&mut borrowed, "via-scm-rights")?;
        parse_cgroupid(&reply)
    })();

    drop(helper_stdin);
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&handoff_dir);

    let got = result?;
    match agent_b_dir {
        Some(dir) => {
            let want = std::fs::metadata(dir)?.ino();
            if got == want {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "scm_rights: borrowed fd identity {got} != agent-b id {want}"
                )))
            }
        }
        None => Ok(()),
    }
}

/// Route (iii): claim another id in the `HELLO` payload. This is the D-15
/// control that must be ignored, so success here means the stub's reply
/// still carries this process's own kernel-stamped identity, not the
/// claimed one. Reachable only when the reply's `cgroupid` equals this
/// process's own id (D5); never counted toward S5.
pub fn attempt_payload_claim(socket_path: &str, claimed_id: &str) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    let reply = send_hello(&mut stream, claimed_id)?;
    let got = parse_cgroupid(&reply)?;
    let own = own_cgroup_id()?;
    if got == own {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "payload_claim control failed: reply cgroupid {got} != own id {own} \
             (claim {claimed_id} may have been honored)"
        )))
    }
}

/// Route (iv): `clone3` with `CLONE_INTO_CGROUP`, placing a new child
/// directly into another agent's cgroup, gated only by `cgroup_may_write`
/// on the reference host. Verifies the child actually landed in
/// `target_cgroup_dir` (via `PIDFD_GET_INFO` on the pidfd `clone3` handed
/// back) before killing and reaping it (D5).
pub fn attempt_clone3_into_cgroup(
    target_cgroup_fd: std::os::fd::RawFd,
    target_cgroup_dir: &str,
) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    match sys::clone3_fork_into_cgroup(target_cgroup_fd)? {
        sys::ForkResult::Parent {
            child_pid,
            child_pidfd,
        } => {
            let result = (|| -> io::Result<()> {
                let identity = sys::pidfd_identity(child_pidfd)?;
                let want = std::fs::metadata(target_cgroup_dir)?.ino();
                if identity.cgroupid == want {
                    Ok(())
                } else {
                    Err(io::Error::other(format!(
                        "clone3 child landed in cgroupid {}, expected {want}",
                        identity.cgroupid
                    )))
                }
            })();
            let _ = sys::pidfd_send_signal(child_pidfd, libc::SIGKILL);
            sys::close_fd(child_pidfd);
            let _ = sys::waitpid_reap(child_pid);
            result
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
    use std::process::Command;

    #[test]
    fn harness_errors_are_distinguishable_from_attempt_errors() {
        let h = harness_error("cannot spawn the stub");
        assert!(is_harness_error(&h));
        assert!(h.to_string().contains("cannot spawn the stub"));
        assert!(!is_harness_error(&io::Error::other("attempt failed")));
        assert!(!is_harness_error(&io::Error::from_raw_os_error(
            libc::EPERM
        )));
    }

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
    fn failed_baseline_surfaces_errno() {
        let row = RouteRow {
            family: Family::Ptrace,
            route: "PTRACE_ATTACH",
            topology: Topology::Sibling,
        };
        let (outcome, errno) = evaluate_row_with_errno(&row, false, || {
            Err(io::Error::from_raw_os_error(libc::EPERM))
        });
        match outcome {
            RunOutcome::Ran(Reach::VoidBaselineFailed) => {}
            other => panic!("expected Ran(VoidBaselineFailed), got {other:?}"),
        }
        assert_eq!(errno, Some(libc::EPERM as i64));
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

    /// Acceptance 4 (agent-mode placement, injectable cgroup root): D4 says
    /// the attacker lands in `<root>/agent-a`, the stub in `<root>/broker`,
    /// and the impersonation helper in `<root>/agent-b`, each via a
    /// `cgroup.procs` write. `write_procs_file` is `place_in_cgroup`'s write
    /// mechanism with the real-path guard split off (D-16's
    /// `assert_cgroup_kill_path` is tested on its own, above); this test
    /// drives that mechanism against a tempdir standing in for a delegated
    /// cgroup v2 mount, so it never touches the real `/sys/fs/cgroup` and
    /// never needs root or an actual cgroup delegation on the test host.
    #[test]
    fn agent_mode_placement_writes_each_pid_to_its_own_leafs_cgroup_procs() {
        let root =
            std::env::temp_dir().join(format!("aok-adv-placement-test-{}", std::process::id()));
        for leaf in ["agent-a", "agent-b", "broker"] {
            std::fs::create_dir_all(root.join(leaf)).expect("create fake cgroup leaf dir");
        }

        let attacker_pid: libc::pid_t = 1111;
        let helper_pid: libc::pid_t = 2222;
        let stub_pid: libc::pid_t = 3333;

        write_procs_file(
            &format!("{}/agent-a/cgroup.procs", root.display()),
            attacker_pid,
        )
        .expect("place attacker in agent-a");
        write_procs_file(
            &format!("{}/agent-b/cgroup.procs", root.display()),
            helper_pid,
        )
        .expect("place helper in agent-b");
        write_procs_file(&format!("{}/broker/cgroup.procs", root.display()), stub_pid)
            .expect("place stub in broker");

        for (leaf, pid) in [
            ("agent-a", attacker_pid),
            ("agent-b", helper_pid),
            ("broker", stub_pid),
        ] {
            let contents =
                std::fs::read_to_string(root.join(leaf).join("cgroup.procs")).unwrap_or_default();
            assert_eq!(
                contents,
                pid.to_string(),
                "{leaf}/cgroup.procs should carry exactly the pid placed there"
            );
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// D5/acceptance (a): the kill route acts on the given pid and only
    /// that pid. Fails on the pre-fix code, which spawned its own `sleep 5`
    /// child inside `attempt_kill` regardless of what was passed in.
    #[test]
    fn kill_route_acts_on_given_pid_and_spawns_no_extra_child() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .expect("spawn a target this test owns");
        let pid = child.id() as libc::pid_t;

        let children_before = count_children_of(std::process::id() as libc::pid_t);
        let target = Target {
            pid,
            socket_path: String::new(),
            abstract_name: None,
        };
        attempt_kill(&target).expect("kill route against the given target");
        let children_after = count_children_of(std::process::id() as libc::pid_t);

        assert_eq!(
            children_before, children_after,
            "attempt_kill spawned an extra child instead of only signalling the given pid"
        );

        let status = child.wait().expect("reap the killed target");
        assert!(
            !status.success(),
            "target pid did not die from the kill route"
        );
    }

    /// Count this test process's own live children by reading `/proc/*/stat`
    /// field 4 (ppid), so the no-extra-spawn assertion above does not depend
    /// on a shell helper.
    fn count_children_of(parent_pid: libc::pid_t) -> usize {
        let mut count = 0;
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return 0;
        };
        for entry in entries.flatten() {
            let Some(pid_str) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if pid_str.parse::<i32>().is_err() {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            // Field 4 is ppid; field 2 (comm) may itself contain spaces
            // inside parens, so split after the closing paren.
            let Some(after_comm) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
                continue;
            };
            let fields: Vec<&str> = after_comm.split_whitespace().collect();
            // fields[0] is state, fields[1] is ppid (field 4 overall).
            if let Some(ppid_str) = fields.get(1)
                && let Ok(ppid) = ppid_str.parse::<libc::pid_t>()
                && ppid == parent_pid
            {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn bind_pathname_route_binds_and_releases_a_fresh_socket() {
        use std::os::unix::net::UnixListener as StdListener;
        let dir = std::env::temp_dir().join(format!("aok-adv-bind-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("stub.sock");

        // A live "stub" stand-in the attempt races against (D5: pathname
        // bind requires a live stub during the attempt).
        let _ = std::fs::remove_file(&path);
        let stub = StdListener::bind(&path).expect("bind stand-in stub");
        drop(stub);

        let target = Target {
            pid: 0,
            socket_path: path.to_str().unwrap().to_string(),
            abstract_name: None,
        };
        attempt_bind_pathname(&target).expect("bind pathname route");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// D5/acceptance (d): `payload_claim` must fail when the reply it gets
    /// back actually carries the claimed id (i.e. a malicious/misbehaving
    /// stub honored the claim). Fails on the pre-fix code, which returned
    /// `Ok` with the raw reply string and never checked it.
    #[test]
    fn payload_claim_fails_when_reply_carries_the_claimed_id() {
        use std::io::{Read, Write};
        let dir = std::env::temp_dir().join(format!("aok-adv-claim-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("malicious.sock");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind malicious stand-in");

        let claimed = "424242";
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).expect("read len");
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut payload = vec![0u8; len];
            stream.read_exact(&mut payload).expect("read payload");
            // Malicious stub: honors the claimed id verbatim.
            let reply = format!("OK cgroupid={claimed} pid=1");
            let reply_len = (reply.len() as u32).to_le_bytes();
            stream.write_all(&reply_len).expect("write reply len");
            stream.write_all(reply.as_bytes()).expect("write reply");
        });

        let err = attempt_payload_claim(path.to_str().unwrap(), claimed)
            .expect_err("payload_claim must reject a reply that honored the claim");
        assert!(
            err.to_string().contains("control failed"),
            "unexpected error: {err}"
        );

        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
