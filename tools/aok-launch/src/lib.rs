//! aok-launch: place an agent process in its own cgroup, narrow it, and manage
//! its lifecycle, from an unprivileged delegated cgroup subtree.
//!
//! Module seams (fixed in 09-01 so later plans edit only their own files):
//! `identity` (09-02), `landlock` and `seccomp` (09-03). `sys` is the one
//! module that may contain raw syscalls.
//!
//! Exit contract of `run` and `nest`:
//! - 64: usage error (bad flags, a malformed name, `--depth 0`).
//! - 125: launcher refusal or setup failure; the errno is named on stderr.
//!   A pre-exec failure in the child arrives over an `O_CLOEXEC` pipe: EOF
//!   means the exec succeeded, a value names the stage and errno.
//! - otherwise the leader's status, `128 + signal` if it was killed.

#![deny(unsafe_code)]

pub mod cgroup;
pub mod identity;
pub mod landlock;
pub mod seccomp;
#[allow(unsafe_code)]
pub mod sys;

use std::ffi::CString;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cgroup::Role;
use landlock::{LayerError, LayerErrorKind, LayerSpec};

/// Usage-error exit code.
pub const EXIT_USAGE: i32 = 64;
/// Refusal / setup-failure exit code.
pub const EXIT_REFUSED: i32 = 125;
/// Deepest `nest` chain accepted; a chain this deep is a test fixture, and
/// the cap keeps the cgroup path far from PATH_MAX.
pub const NEST_DEPTH_MAX: u32 = 32;

// Stages reported over the pre-exec pipe.
const STAGE_MOVE: u8 = 1;
const STAGE_LANDLOCK: u8 = 2;
const STAGE_SECCOMP: u8 = 3;
const STAGE_NNP: u8 = 4;
const STAGE_EXEC: u8 = 5;

fn stage_name(stage: u8) -> &'static str {
    match stage {
        STAGE_MOVE => "cgroup-move",
        STAGE_LANDLOCK => "landlock",
        STAGE_SECCOMP => "seccomp",
        STAGE_NNP => "no-new-privs",
        STAGE_EXEC => "exec",
        _ => "child",
    }
}

/// The symbolic name of an errno, or `errno N` for one not in the table.
pub fn errno_name(errno: i32) -> String {
    let name = match errno {
        libc::EPERM => "EPERM",
        libc::ENOENT => "ENOENT",
        libc::ESRCH => "ESRCH",
        libc::EINTR => "EINTR",
        libc::EIO => "EIO",
        libc::E2BIG => "E2BIG",
        libc::ENOEXEC => "ENOEXEC",
        libc::EBADF => "EBADF",
        libc::ECHILD => "ECHILD",
        libc::EAGAIN => "EAGAIN",
        libc::ENOMEM => "ENOMEM",
        libc::EACCES => "EACCES",
        libc::EFAULT => "EFAULT",
        libc::EBUSY => "EBUSY",
        libc::EEXIST => "EEXIST",
        libc::ENODEV => "ENODEV",
        libc::ENOTDIR => "ENOTDIR",
        libc::EISDIR => "EISDIR",
        libc::EINVAL => "EINVAL",
        libc::ENFILE => "ENFILE",
        libc::EMFILE => "EMFILE",
        libc::ENOTTY => "ENOTTY",
        libc::ETXTBSY => "ETXTBSY",
        libc::ENOSPC => "ENOSPC",
        libc::EROFS => "EROFS",
        libc::ENAMETOOLONG => "ENAMETOOLONG",
        libc::ENOSYS => "ENOSYS",
        libc::ENOTEMPTY => "ENOTEMPTY",
        libc::ELOOP => "ELOOP",
        libc::EOPNOTSUPP => "EOPNOTSUPP",
        libc::ETIMEDOUT => "ETIMEDOUT",
        _ => return format!("errno {errno}"),
    };
    name.to_string()
}

/// A failed launcher command: the exit code and the line to print.
#[derive(Debug)]
pub struct Fail {
    pub exit: i32,
    pub text: String,
}

impl Fail {
    pub fn usage(msg: impl AsRef<str>) -> Fail {
        Fail {
            exit: EXIT_USAGE,
            text: format!("aok-launch: usage: {}", msg.as_ref()),
        }
    }

    pub fn errno(stage: &str, errno: i32) -> Fail {
        Fail {
            exit: EXIT_REFUSED,
            text: format!("aok-launch: {stage}: {}", errno_name(errno)),
        }
    }

    pub fn io(stage: &str, e: &io::Error) -> Fail {
        Fail::errno(stage, e.raw_os_error().unwrap_or(libc::EIO))
    }

    fn from_layer(e: LayerError) -> Fail {
        match e.kind {
            LayerErrorKind::Usage => Fail::usage(format!("{}: {}", e.msg, errno_name(e.errno))),
            LayerErrorKind::Refused => Fail {
                exit: EXIT_REFUSED,
                text: format!("aok-launch: {}: {}", e.msg, errno_name(e.errno)),
            },
        }
    }

    fn from_name(e: &cgroup::NameError) -> Fail {
        Fail::usage(format!("name: {} ({})", errno_name(e.errno), e.reason))
    }
}

/// Everything `run` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSpec {
    pub name: String,
    pub on_behalf_of: Option<String>,
    pub grants: Vec<String>,
    pub cmd: Vec<String>,
}

/// Parse the arguments after `run`:
/// `--name N [--on-behalf-of H] [--grant SPEC]... -- CMD...`.
pub fn parse_run(args: &[String]) -> Result<RunSpec, String> {
    let mut name = None;
    let mut on_behalf_of = None;
    let mut grants = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                let cmd = args[i + 1..].to_vec();
                if cmd.is_empty() {
                    return Err("run: no command after --".into());
                }
                let name = name.ok_or("run: --name is required")?;
                return Ok(RunSpec {
                    name,
                    on_behalf_of,
                    grants,
                    cmd,
                });
            }
            "--name" => name = Some(take_value(args, &mut i, "--name")?),
            "--on-behalf-of" => on_behalf_of = Some(take_value(args, &mut i, "--on-behalf-of")?),
            "--grant" => grants.push(take_value(args, &mut i, "--grant")?),
            other => return Err(format!("run: unknown argument {other:?}")),
        }
        i += 1;
    }
    Err("run: missing -- before the command".into())
}

fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

/// The `nest` request: one base name, a depth, forwarded grants, a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestSpec {
    pub base: String,
    pub depth: u32,
    pub grants: Vec<String>,
    pub cmd: Vec<String>,
}

/// Parse the arguments after `nest`:
/// `--name BASE --depth N [--grant SPEC]... -- CMD...`.
pub fn parse_nest(args: &[String]) -> Result<NestSpec, String> {
    let mut base = None;
    let mut depth = None;
    let mut grants = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                let cmd = args[i + 1..].to_vec();
                if cmd.is_empty() {
                    return Err("nest: no command after --".into());
                }
                let base = base.ok_or("nest: --name is required")?;
                let depth = depth.ok_or("nest: --depth is required")?;
                if depth == 0 {
                    return Err("nest: --depth must be at least 1".into());
                }
                if depth > NEST_DEPTH_MAX {
                    return Err(format!("nest: --depth exceeds {NEST_DEPTH_MAX}"));
                }
                return Ok(NestSpec {
                    base,
                    depth,
                    grants,
                    cmd,
                });
            }
            "--name" => base = Some(take_value(args, &mut i, "--name")?),
            "--depth" => {
                let v = take_value(args, &mut i, "--depth")?;
                depth = Some(
                    v.parse::<u32>()
                        .map_err(|_| format!("nest: --depth {v:?} is not a number"))?,
                );
            }
            "--grant" => grants.push(take_value(args, &mut i, "--grant")?),
            other => return Err(format!("nest: unknown argument {other:?}")),
        }
        i += 1;
    }
    Err("nest: missing -- before the command".into())
}

/// Build the one root launch a `nest` makes. Level 1 is `run --name BASE`;
/// level k (2..=depth) is the launcher re-execed inside level k-1's agent as
/// `run --name dk`, each forwarding the same grants, so level k is a sub
/// launch inside level k-1.
pub fn nest_chain(spec: &NestSpec, exe: &str) -> RunSpec {
    let mut cmd = spec.cmd.clone();
    for k in (2..=spec.depth).rev() {
        let mut next = vec![
            exe.to_string(),
            "run".into(),
            "--name".into(),
            format!("d{k}"),
        ];
        for g in &spec.grants {
            next.push("--grant".into());
            next.push(g.clone());
        }
        next.push("--".into());
        next.extend(cmd);
        cmd = next;
    }
    RunSpec {
        name: spec.base.clone(),
        on_behalf_of: None,
        grants: spec.grants.clone(),
        cmd,
    }
}

/// How the child got into its cgroup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnPath {
    /// `clone3` with `CLONE_INTO_CGROUP`: born in the leaf.
    Clone3,
    /// `fork`, then the child wrote itself into the leaf. `window` is an
    /// upper bound on the time from just before the fork until the parent
    /// observed the child in the leaf, `None` if it never did.
    Fallback { window: Option<Duration> },
}

/// A launched leader: its pid, its pidfd, and where it lives.
pub struct Spawned {
    pub pid: libc::pid_t,
    pub pidfd: OwnedFd,
    pub agent: PathBuf,
    pub leaf: PathBuf,
    pub path: SpawnPath,
    pre_exec: io::PipeReader,
}

fn report(w: &io::PipeWriter, stage: u8, errno: i32) -> ! {
    let mut buf = [0u8; 5];
    buf[0] = stage;
    buf[1..].copy_from_slice(&errno.to_le_bytes());
    let mut w = w;
    let _ = w.write_all(&buf);
    sys::exit_now(EXIT_REFUSED)
}

/// The clone3 / fork child. Applies the layers in the fixed order, then
/// execs. Never returns: any failure is written to the pipe and the child
/// exits 125.
fn child_main(layer: &LayerSpec, plan: &sys::ExecPlan, w: &io::PipeWriter, move_errno: i32) -> ! {
    if move_errno != 0 {
        report(w, STAGE_MOVE, move_errno);
    }
    if let Err(e) = landlock::apply(layer) {
        report(w, STAGE_LANDLOCK, e.errno);
    }
    if let Err(e) = seccomp::apply() {
        report(w, STAGE_SECCOMP, e.errno);
    }
    if let Err(e) = sys::set_no_new_privs() {
        report(w, STAGE_NNP, e.raw_os_error().unwrap_or(libc::EIO));
    }
    let e = plan.exec();
    report(w, STAGE_EXEC, e.raw_os_error().unwrap_or(libc::EIO))
}

/// Poll `/proc/<pid>/cgroup` until the child shows up in `leaf`, for at most
/// one second. Used only to bound the fallback's fork-to-move window.
fn observe_in_leaf(pid: libc::pid_t, leaf: &Path, started: Instant) -> Option<Duration> {
    let deadline = started + Duration::from_secs(1);
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
            && cgroup::parse_own_cgroup(&text).is_ok_and(|p| p == leaf)
        {
            return Some(started.elapsed());
        }
        std::thread::sleep(Duration::from_micros(100));
    }
    None
}

/// Resolve the role, validate, build the cgroup layout and clone the leader
/// into `<agent>/main`.
pub fn spawn(spec: &RunSpec) -> Result<Spawned, Fail> {
    cgroup::validate_name(&spec.name).map_err(|e| Fail::from_name(&e))?;
    if spec.cmd.is_empty() {
        return Err(Fail::usage("run: no command"));
    }
    let plan = sys::ExecPlan::new(&spec.cmd)
        .map_err(|_| Fail::usage("command argument contains a NUL byte"))?;

    let role = identity::resolve_role(spec.on_behalf_of.as_deref()).map_err(Fail::from_layer)?;
    let agent_path = role.parent_dir().join(&spec.name);

    let launcher_exe =
        std::env::current_exe().map_err(|e| Fail::io("cannot resolve launcher path", &e))?;
    let layer = LayerSpec {
        grants: spec.grants.clone(),
        agent_dir: agent_path,
        launcher_exe,
    };
    landlock::preflight(&layer).map_err(Fail::from_layer)?;

    if let Role::Root(root) = &role {
        cgroup::evacuate_root(root).map_err(|e| Fail::io("cgroup: evacuate root", &e))?;
    }
    let (agent, leaf) = cgroup::make_agent(role.parent_dir(), &spec.name)
        .map_err(|e| Fail::io("cgroup: create agent", &e))?;
    if matches!(role, Role::Root(_)) {
        identity::register_root(&agent, spec.on_behalf_of.as_deref()).map_err(Fail::from_layer)?;
    }
    eprintln!(
        "aok-launch: role={} agent={}",
        role.label(),
        agent.display()
    );

    let leaf_fd = sys::open_cgroup_dir(&leaf).map_err(|e| Fail::io("cgroup: open leaf", &e))?;
    let procs_c = CString::new(leaf.join("cgroup.procs").as_os_str().as_bytes())
        .map_err(|_| Fail::errno("cgroup: leaf path", libc::EINVAL))?;
    let (reader, writer) = io::pipe().map_err(|e| Fail::io("pipe", &e))?;

    let (pid, pidfd, path) = match sys::clone3_fork_into_cgroup(leaf_fd.as_raw_fd()) {
        Ok(sys::ForkResult::Child) => child_main(&layer, &plan, &writer, 0),
        Ok(sys::ForkResult::Parent {
            child_pid,
            child_pidfd,
        }) => (child_pid, child_pidfd, SpawnPath::Clone3),
        // Only ENOSYS means "this kernel has no clone3"; any other error,
        // EPERM and EACCES included, is a real refusal and is not retried.
        Err(e) if e.raw_os_error() == Some(libc::ENOSYS) => {
            let started = Instant::now();
            match sys::fork_move(&procs_c) {
                Ok(sys::ForkMove::Child { move_errno }) => {
                    child_main(&layer, &plan, &writer, move_errno)
                }
                Ok(sys::ForkMove::Parent {
                    child_pid,
                    child_pidfd,
                }) => {
                    let window = observe_in_leaf(child_pid, &leaf, started);
                    (child_pid, child_pidfd, SpawnPath::Fallback { window })
                }
                Err(e) => return Err(Fail::io("fork", &e)),
            }
        }
        Err(e) => return Err(Fail::io("clone3", &e)),
    };
    drop(writer);
    drop(leaf_fd);

    match path {
        SpawnPath::Clone3 => eprintln!("aok-launch: spawn_path=clone3"),
        SpawnPath::Fallback { window: Some(w) } => eprintln!(
            "aok-launch: spawn_path=fallback window_us={}",
            w.as_micros()
        ),
        SpawnPath::Fallback { window: None } => {
            eprintln!("aok-launch: spawn_path=fallback window_us=unobserved")
        }
    }

    Ok(Spawned {
        pid,
        pidfd: sys::owned_fd(pidfd),
        agent,
        leaf,
        path,
        pre_exec: reader,
    })
}

impl Spawned {
    /// Read the pre-exec pipe (EOF means the exec succeeded), then wait for
    /// the leader on its pidfd and return its status.
    pub fn wait(mut self) -> Result<i32, Fail> {
        let mut buf = Vec::new();
        self.pre_exec
            .read_to_end(&mut buf)
            .map_err(|e| Fail::io("pre-exec pipe", &e))?;
        if buf.len() >= 5 {
            let errno = i32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
            let _ = sys::wait_status(self.pid);
            return Err(Fail::errno(stage_name(buf[0]), errno));
        }
        // Readable on exit. Loop on the timeout so a day-long agent is not
        // a special case.
        loop {
            match sys::poll_readable(self.pidfd.as_raw_fd(), Duration::from_secs(3600)) {
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => return Err(Fail::io("poll leader", &e)),
            }
        }
        sys::wait_status(self.pid).map_err(|e| Fail::io("wait leader", &e))
    }
}

/// `aok-launch run`: spawn, wait, return the exit code. Prints failures.
pub fn run(spec: &RunSpec) -> i32 {
    match spawn(spec).and_then(Spawned::wait) {
        Ok(code) => code,
        Err(f) => {
            eprintln!("{}", f.text);
            f.exit
        }
    }
}

/// `aok-launch nest`: one root launch whose command chain re-launches the
/// executable once per extra level.
pub fn nest(spec: &NestSpec) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            let f = Fail::io("cannot resolve launcher path", &e);
            eprintln!("{}", f.text);
            return f.exit;
        }
    };
    run(&nest_chain(spec, &exe.to_string_lossy()))
}

/// `aok-launch freeze|thaw|kill <agent>`.
pub fn lifecycle(verb: &str, args: &[String]) -> i32 {
    let result = (|| -> Result<(), Fail> {
        let [arg] = args else {
            return Err(Fail::usage(format!("{verb}: expected exactly one <agent>")));
        };
        let role = cgroup::launch_role().map_err(|e| Fail::io("cannot read own cgroup", &e))?;
        let agent = cgroup::resolve_agent(arg, &role).map_err(|e| {
            Fail::usage(format!(
                "{verb}: agent: {}",
                errno_name(e.raw_os_error().unwrap_or(libc::EINVAL))
            ))
        })?;
        let r = match verb {
            "freeze" => cgroup::set_frozen(&agent, true),
            "thaw" => cgroup::set_frozen(&agent, false),
            _ => cgroup::kill_agent(&agent),
        };
        r.map_err(|e| Fail::io(verb, &e))
    })();
    match result {
        Ok(()) => 0,
        Err(f) => {
            eprintln!("{}", f.text);
            f.exit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_run_full() {
        let s = parse_run(&v(&[
            "--name",
            "a",
            "--on-behalf-of",
            "h",
            "--grant",
            "rx:/usr",
            "--grant",
            "tcp:80",
            "--",
            "echo",
            "hi",
        ]))
        .unwrap();
        assert_eq!(s.name, "a");
        assert_eq!(s.on_behalf_of.as_deref(), Some("h"));
        assert_eq!(s.grants, v(&["rx:/usr", "tcp:80"]));
        assert_eq!(s.cmd, v(&["echo", "hi"]));
    }

    #[test]
    fn parse_run_usage_errors() {
        assert!(parse_run(&v(&["--", "true"])).is_err(), "missing --name");
        assert!(parse_run(&v(&["--name", "a"])).is_err(), "missing --");
        assert!(parse_run(&v(&["--name", "a", "--"])).is_err(), "empty cmd");
        assert!(parse_run(&v(&["--name"])).is_err(), "flag without value");
        assert!(parse_run(&v(&["--bogus", "--name", "a", "--", "x"])).is_err());
    }

    #[test]
    fn parse_run_keeps_empty_name_for_the_guard() {
        let s = parse_run(&v(&["--name", "", "--", "true"])).unwrap();
        assert_eq!(s.name, "");
    }

    #[test]
    fn parse_nest_rules() {
        assert!(parse_nest(&v(&["--name", "n", "--depth", "0", "--", "x"])).is_err());
        assert!(parse_nest(&v(&["--depth", "2", "--", "x"])).is_err());
        assert!(parse_nest(&v(&["--name", "n", "--depth", "two", "--", "x"])).is_err());
        assert!(parse_nest(&v(&["--name", "n", "--depth", "99", "--", "x"])).is_err());
        let s = parse_nest(&v(&[
            "--name", "n", "--depth", "2", "--grant", "g", "--", "x",
        ]))
        .unwrap();
        assert_eq!((s.depth, s.base.as_str()), (2, "n"));
    }

    #[test]
    fn nest_chain_depth_one_is_a_plain_run() {
        let n = parse_nest(&v(&["--name", "n", "--depth", "1", "--", "true"])).unwrap();
        let r = nest_chain(&n, "/x/aok-launch");
        assert_eq!(r.name, "n");
        assert_eq!(r.cmd, v(&["true"]));
    }

    #[test]
    fn nest_chain_forwards_grants_and_names_levels() {
        let n = parse_nest(&v(&[
            "--name", "n", "--depth", "3", "--grant", "rx:/usr", "--", "true",
        ]))
        .unwrap();
        let r = nest_chain(&n, "/x/aok-launch");
        assert_eq!(r.name, "n");
        assert_eq!(r.grants, v(&["rx:/usr"]));
        assert_eq!(
            r.cmd,
            v(&[
                "/x/aok-launch",
                "run",
                "--name",
                "d2",
                "--grant",
                "rx:/usr",
                "--",
                "/x/aok-launch",
                "run",
                "--name",
                "d3",
                "--grant",
                "rx:/usr",
                "--",
                "true",
            ])
        );
    }

    #[test]
    fn errno_names() {
        assert_eq!(errno_name(libc::ENOENT), "ENOENT");
        assert_eq!(errno_name(libc::E2BIG), "E2BIG");
        assert_eq!(errno_name(9999), "errno 9999");
    }

    #[test]
    fn layer_errors_map_to_the_exit_contract() {
        let u = Fail::from_layer(LayerError::usage(libc::EINVAL, "bad grant"));
        assert_eq!(u.exit, EXIT_USAGE);
        let r = Fail::from_layer(LayerError::refused(libc::E2BIG, "landlock"));
        assert_eq!(r.exit, EXIT_REFUSED);
        assert_eq!(r.text, "aok-launch: landlock: E2BIG");
    }

    #[test]
    fn name_failures_are_usage_errors() {
        let e = cgroup::validate_name("").unwrap_err();
        let f = Fail::from_name(&e);
        assert_eq!(f.exit, EXIT_USAGE);
        assert!(f.text.contains("EINVAL"));
    }
}
