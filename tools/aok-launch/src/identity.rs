//! Agent identity (LNCH-02): resolve who a task is from a pidfd, and keep the
//! launcher-written registry that supplies `on_behalf_of`.
//!
//! The identity key is the kernel-stamped cgroup id of the task's cgroup.
//! Resolution (D-09):
//!
//! ```text
//! pidfd --PIDFD_GET_INFO(CGROUPID)--> cgroup id
//!   --/proc/PID/cgroup--> path, checked: st_ino(path) == cgroup id
//!   --drop trailing "main"--> agent directory
//!   --nearest ancestor whose cgroup id is in the registry--> root agent
//!   agent_id = agent directory relative to the root agent's parent (D-06)
//!   on_behalf_of = the root agent's registry entry (D-08)
//! ```
//!
//! Where `PIDFD_GET_INFO` is absent (ENOTTY, and only ENOTTY) the same key is
//! reached a second way: the pid the caller already knows, then
//! `/proc/PID/cgroup`. That route is racy against pid reuse and reports the
//! window it cannot close. It is a second route to the one key, never a second
//! key.
//!
//! No `user.*` xattr is ever read (D-10); `tests/xattr_forgery.rs` is the
//! negative control showing why. This module is safe Rust: the raw ioctl and
//! socket calls live in `sys`.

use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use crate::cgroup::{self, CGROUP_MOUNT, MAIN_LEAF, Role};
use crate::landlock::LayerError;
use crate::{EXIT_REFUSED, EXIT_USAGE, Fail, errno_name, sys};

/// Longest `on_behalf_of` accepted.
pub const ON_BEHALF_OF_MAX: usize = 128;
/// The registry is read up to this many bytes; the rest is ignored.
const REGISTRY_READ_MAX: u64 = 1 << 20;
/// Longest registry line accepted.
const LINE_MAX: usize = 1024 + ON_BEHALF_OF_MAX;
/// Most entries kept from one registry file.
const ENTRIES_MAX: usize = 4096;
/// Longest cgroup path accepted in a registry line.
const PATH_MAX_LEN: usize = 1024;

// ---------------------------------------------------------------------------
// Registry.
// ---------------------------------------------------------------------------

/// One root agent: the cgroup id of its directory, that directory's path (for
/// pruning and for humans, never trusted for lookup) and its principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub cgroup_id: u64,
    pub path: PathBuf,
    pub on_behalf_of: Option<String>,
}

/// The validated contents of a registry file plus a warning per skipped line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Registry {
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
}

/// True for 1..=128 bytes of `[A-Za-z0-9._:@+-]`. An empty `on_behalf_of` is
/// stored as the empty field and means "none"; it is not accepted as input.
pub fn valid_on_behalf_of(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= ON_BEHALF_OF_MAX
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:@+-".contains(&b))
}

fn validate_on_behalf_of(s: &str) -> Result<(), LayerError> {
    if valid_on_behalf_of(s) {
        Ok(())
    } else {
        Err(LayerError::usage(
            libc::EINVAL,
            "--on-behalf-of must be 1..=128 bytes of [A-Za-z0-9._:@+-]",
        ))
    }
}

fn valid_registry_path(p: &str) -> bool {
    let path = Path::new(p);
    p.len() <= PATH_MAX_LEN
        && path.is_absolute()
        && path.starts_with(CGROUP_MOUNT)
        && !p.chars().any(char::is_control)
        && !path.components().any(|c| matches!(c, Component::ParentDir))
}

fn parse_line(line: &str) -> Result<Entry, &'static str> {
    if line.len() > LINE_MAX {
        return Err("line too long");
    }
    let mut it = line.split('\t');
    let (Some(id), Some(path), Some(obo), None) = (it.next(), it.next(), it.next(), it.next())
    else {
        return Err("expected 3 tab-separated fields");
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("cgroup_id is not a decimal number");
    }
    let cgroup_id: u64 = id.parse().map_err(|_| "cgroup_id out of range")?;
    if cgroup_id == 0 {
        return Err("cgroup_id is zero");
    }
    if !valid_registry_path(path) {
        return Err("path is not an absolute cgroup path");
    }
    let on_behalf_of = if obo.is_empty() {
        None
    } else if valid_on_behalf_of(obo) {
        Some(obo.to_string())
    } else {
        return Err("on_behalf_of is malformed");
    };
    Ok(Entry {
        cgroup_id,
        path: PathBuf::from(path),
        on_behalf_of,
    })
}

impl Registry {
    /// Validate every line; a bad line is skipped with a warning and never
    /// trusted. The first entry for a cgroup id wins and a later duplicate is
    /// skipped: the launcher writes one line per id, so a second one is
    /// tampering or a bug, and either way not authoritative.
    pub fn parse(text: &str) -> Registry {
        let mut reg = Registry::default();
        for (i, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let n = i + 1;
            match parse_line(line) {
                Ok(_) if reg.entries.len() >= ENTRIES_MAX => {
                    reg.warnings
                        .push(format!("line {n} skipped: more than {ENTRIES_MAX} entries"));
                }
                Ok(e) if reg.get(e.cgroup_id).is_some() => {
                    reg.warnings
                        .push(format!("line {n} skipped: duplicate cgroup_id"));
                }
                Ok(e) => reg.entries.push(e),
                Err(why) => reg.warnings.push(format!("line {n} skipped: {why}")),
            }
        }
        reg
    }

    pub fn get(&self, cgroup_id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.cgroup_id == cgroup_id)
    }

    fn render(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&format!(
                "{}\t{}\t{}\n",
                e.cgroup_id,
                e.path.display(),
                e.on_behalf_of.as_deref().unwrap_or("")
            ));
        }
        out
    }
}

/// `/run/user/UID/aok-launch`, with UID the real uid. `XDG_RUNTIME_DIR` is
/// not consulted: an agent controls its own environment and could otherwise
/// point the registry check at an empty file of its own (D-08).
pub fn registry_dir() -> PathBuf {
    PathBuf::from(format!("/run/user/{}/aok-launch", sys::real_uid()))
}

fn valid_boot_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// `/run/user/UID/aok-launch/registry.<boot_id>`.
pub fn registry_path() -> io::Result<PathBuf> {
    let raw = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let boot = raw.trim();
    if !valid_boot_id(boot) {
        return Err(io::Error::from_raw_os_error(libc::EIO));
    }
    Ok(registry_dir().join(format!("registry.{boot}")))
}

fn warn_all(reg: &Registry) {
    for w in &reg.warnings {
        eprintln!("aok-launch: registry: warning: {w}");
    }
}

/// Read and validate the registry file at `path`. A missing file is an empty
/// registry. Any other failure, `EACCES` included, is returned: a Landlocked
/// agent gets `EACCES` here because the registry is under no grant.
fn read_registry_file(path: &Path) -> io::Result<Registry> {
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
        Err(e) => return Err(e),
    };
    if f.metadata()?.uid() != sys::real_uid() {
        return Err(io::Error::from_raw_os_error(libc::EACCES));
    }
    let mut bytes = Vec::new();
    f.take(REGISTRY_READ_MAX).read_to_end(&mut bytes)?;
    Ok(Registry::parse(&String::from_utf8_lossy(&bytes)))
}

/// Load the registry, printing a warning for each skipped line.
pub fn load_registry() -> io::Result<Registry> {
    let reg = read_registry_file(&registry_path()?)?;
    warn_all(&reg);
    Ok(reg)
}

/// A registered root's directory is alive while it exists with the same id.
fn root_alive(e: &Entry) -> bool {
    fs::metadata(&e.path).is_ok_and(|m| m.ino() == e.cgroup_id)
}

fn io_refused(what: &str, e: &io::Error) -> LayerError {
    LayerError::refused(e.raw_os_error().unwrap_or(libc::EIO), what)
}

/// Create the registry directory (0700) if absent and insist it is ours,
/// not a symlink and closed to group and other.
fn ensure_private_dir(dir: &Path) -> Result<(), LayerError> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(io_refused("registry: create directory", &e)),
    }
    let meta = fs::symlink_metadata(dir).map_err(|e| io_refused("registry: stat directory", &e))?;
    if !meta.is_dir() || meta.uid() != sys::real_uid() || meta.mode() & 0o077 != 0 {
        return Err(LayerError::refused(
            libc::EACCES,
            "registry: directory is not a private directory owned by this uid",
        ));
    }
    Ok(())
}

/// Record a new root agent in the registry. Called for a Root launch only,
/// after the agent directory exists and before the child is cloned, because
/// the entry is keyed on that directory's cgroup id (D-06, D-08).
///
/// Under an exclusive lock: drop entries whose root directory is gone and
/// lines that fail validation, add one line, and replace the file atomically
/// (mode 0600 in a 0700 directory). Registering a directory that is already
/// registered with the same `on_behalf_of` changes nothing; with a different
/// one it is refused `EEXIST` while the agent has live processes, because
/// `on_behalf_of` is fixed at root launch, and replaced otherwise.
pub fn register_root(agent_dir: &Path, on_behalf_of: Option<&str>) -> Result<(), LayerError> {
    if let Some(o) = on_behalf_of {
        validate_on_behalf_of(o)?;
    }
    let cgroup_id = fs::metadata(agent_dir)
        .map_err(|e| io_refused("registry: stat agent directory", &e))?
        .ino();
    let agent_str = agent_dir.to_string_lossy();
    if !valid_registry_path(&agent_str) {
        return Err(LayerError::refused(
            libc::EINVAL,
            "registry: agent path cannot be recorded",
        ));
    }

    let dir = registry_dir();
    ensure_private_dir(&dir)?;
    let path = registry_path().map_err(|e| io_refused("registry: boot id", &e))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|e| io_refused("registry: open lock", &e))?;
    lock.lock().map_err(|e| io_refused("registry: lock", &e))?;

    let mut reg = read_registry_file(&path).map_err(|e| io_refused("registry: read", &e))?;
    warn_all(&reg);
    let before = reg.entries.len();
    reg.entries.retain(root_alive);
    let mut changed = reg.entries.len() != before || !reg.warnings.is_empty();

    let new = Entry {
        cgroup_id,
        path: agent_dir.to_path_buf(),
        on_behalf_of: on_behalf_of.map(str::to_string),
    };
    match reg.entries.iter().position(|e| e.cgroup_id == cgroup_id) {
        Some(i) if reg.entries[i].on_behalf_of == new.on_behalf_of => {}
        Some(i) => {
            let live = cgroup::event(agent_dir, "populated").map_or(true, |v| v != 0);
            if live {
                return Err(LayerError::refused(
                    libc::EEXIST,
                    "registry: on_behalf_of is fixed at root launch and this agent is live",
                ));
            }
            reg.entries[i] = new;
            changed = true;
        }
        None => {
            reg.entries.push(new);
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }

    let tmp = path.with_extension("tmp");
    let mut f = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| io_refused("registry: open temp file", &e))?;
    f.write_all(reg.render().as_bytes())
        .map_err(|e| io_refused("registry: write", &e))?;
    fs::rename(&tmp, &path).map_err(|e| io_refused("registry: replace", &e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Walking up to the registered root.
// ---------------------------------------------------------------------------

type InoFn<'a> = &'a dyn Fn(&Path) -> io::Result<u64>;

/// Starting at `start` and going up to (not including) the cgroup mount, the
/// nearest directory whose cgroup id is in the registry. Lookup is by the
/// directory's own id; the path stored in the registry is never consulted.
fn find_registered_root<'a>(
    start: &'a Path,
    reg: &Registry,
    ino: InoFn,
) -> io::Result<Option<&'a Path>> {
    for dir in start
        .ancestors()
        .take_while(|p| p.starts_with(CGROUP_MOUNT) && *p != Path::new(CGROUP_MOUNT))
    {
        if reg.get(ino(dir)?).is_some() {
            return Ok(Some(dir));
        }
    }
    Ok(None)
}

fn stat_ino(p: &Path) -> io::Result<u64> {
    Ok(fs::metadata(p)?.ino())
}

/// The agent directory of a task's cgroup: the cgroup with a trailing `main`
/// leaf dropped. A task in any other directory (an agent that made itself a
/// directory, say) is attributed to that directory itself.
fn agent_dir_of(cgroup: &Path) -> &Path {
    if cgroup.file_name().is_some_and(|n| n == MAIN_LEAF) {
        cgroup.parent().unwrap_or(cgroup)
    } else {
        cgroup
    }
}

// ---------------------------------------------------------------------------
// Launch role (D-08, D-12).
// ---------------------------------------------------------------------------

/// Decide the launch role and refuse a launch that must not happen. Starts
/// from `cgroup::launch_role` (the own cgroup path) and adds the registry
/// half.
///
/// - Sub (own cgroup ends in `main`): the registry is neither read nor
///   written, and `--on-behalf-of` is refused `EACCES`, because
///   `on_behalf_of` is fixed at root launch.
/// - Root: refused `EACCES` if the registry cannot be read with `EACCES`
///   (what a Landlocked agent gets) or if any ancestor of the launcher's own
///   cgroup is a registered root (a process in an agent tree but outside a
///   `main` leaf). Any other registry read failure is refused with its errno.
///   Only a Root outside every registered root goes on to `register_root`.
pub fn resolve_role(on_behalf_of: Option<&str>) -> Result<Role, LayerError> {
    let cant_read = |e: io::Error| io_refused("cannot read own cgroup", &e);
    let role = cgroup::launch_role().map_err(cant_read)?;
    let own = cgroup::own_cgroup().map_err(cant_read)?;
    decide_role(&own, role, on_behalf_of, &load_registry, &stat_ino)
}

fn decide_role(
    own: &Path,
    role: Role,
    on_behalf_of: Option<&str>,
    load: &dyn Fn() -> io::Result<Registry>,
    ino: InoFn,
) -> Result<Role, LayerError> {
    match &role {
        Role::Sub(_) => {
            if on_behalf_of.is_some() {
                return Err(LayerError::refused(
                    libc::EACCES,
                    "on_behalf_of is fixed at root launch; a sub launch cannot set it",
                ));
            }
            Ok(role)
        }
        Role::Root(_) => {
            if let Some(o) = on_behalf_of {
                validate_on_behalf_of(o)?;
            }
            let reg = load().map_err(|e| io_refused("registry: read", &e))?;
            match find_registered_root(own, &reg, ino) {
                Err(e) => Err(io_refused("registry: stat ancestor", &e)),
                Ok(Some(_)) => Err(LayerError::refused(
                    libc::EACCES,
                    "root launch from inside a registered agent tree; on_behalf_of is fixed at root launch",
                )),
                Ok(None) => Ok(role),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution.
// ---------------------------------------------------------------------------

/// A resolved agent identity. `cgroup_id` is the id of the task's own cgroup
/// (its `main` leaf for an ordinary agent task); `agent_id` is the agent
/// directory's path from the root agent (D-06).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub agent_id: String,
    pub on_behalf_of: Option<String>,
    pub parent: Option<String>,
    pub cgroup_id: u64,
}

/// Which route reached the identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `PIDFD_GET_INFO` on a pidfd.
    Pidfd,
    /// The known pid and `/proc/PID/cgroup`. `window_ns` is the time from
    /// taking the pid to finishing the `/proc` read: the pid-reuse race this
    /// route cannot close.
    Fallback { window_ns: u128 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub identity: Identity,
    pub route: Route,
}

/// Why a resolution failed.
#[derive(Debug)]
pub enum ResolveError {
    /// `PIDFD_GET_INFO` returned ENOTTY: the feature is absent. The only
    /// error that selects the fallback.
    NoPidfdInfo,
    /// The kernel answered but gave no cgroup id.
    NoCgroupId,
    Errno {
        stage: &'static str,
        errno: i32,
    },
    /// The D-06 falsifier: the cgroup id and the directory inode of the
    /// resolved path disagree.
    Mismatch {
        cgroup_id: u64,
        path_ino: u64,
        path: PathBuf,
    },
    /// The task is not under any registered root agent.
    NotInAgent {
        path: PathBuf,
    },
    /// The process exited while the fallback was reading `/proc`.
    Exited,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NoPidfdInfo => {
                write!(f, "pidfd-get-info: ENOTTY and no pid to fall back on")
            }
            ResolveError::NoCgroupId => write!(f, "pidfd-get-info: kernel returned no cgroup id"),
            ResolveError::Errno { stage, errno } => write!(f, "{stage}: {}", errno_name(*errno)),
            ResolveError::Mismatch {
                cgroup_id,
                path_ino,
                path,
            } => write!(
                f,
                "identity: cgroup id {cgroup_id} disagrees with inode {path_ino} of {}",
                path.display()
            ),
            ResolveError::NotInAgent { path } => write!(
                f,
                "identity: {} is not under a registered root agent",
                path.display()
            ),
            ResolveError::Exited => write!(f, "identity: process exited during resolution"),
        }
    }
}

impl From<ResolveError> for Fail {
    fn from(e: ResolveError) -> Fail {
        Fail {
            exit: EXIT_REFUSED,
            text: format!("aok-launch: {e}"),
        }
    }
}

/// Classify a `PIDFD_GET_INFO` failure. ENOTTY, and only ENOTTY, means the
/// feature is absent; every other errno is a real failure and is never a
/// reason to fall back (D-09).
fn classify_info_error(errno: i32) -> ResolveError {
    if errno == libc::ENOTTY {
        ResolveError::NoPidfdInfo
    } else {
        ResolveError::Errno {
            stage: "pidfd-get-info",
            errno,
        }
    }
}

fn errno_of(e: &io::Error) -> i32 {
    e.raw_os_error().unwrap_or(libc::EIO)
}

/// `agent_dir` relative to the parent of `root_dir`: `r7`, `r7/s1`, ...
fn agent_id_under(root_dir: &Path, agent_dir: &Path) -> Option<String> {
    let base = root_dir.parent()?;
    agent_dir
        .strip_prefix(base)
        .ok()?
        .to_str()
        .map(str::to_string)
}

/// The shared tail of both routes: from `/proc/PID/cgroup` text to an
/// identity. With `expected` (the id the kernel stamped) the directory inode
/// must equal it; without it (the fallback) the inode is the id.
fn identity_from_cgroup_text(
    expected: Option<u64>,
    text: &str,
    reg: &Registry,
    ino: InoFn,
) -> Result<Identity, ResolveError> {
    let path = cgroup::parse_own_cgroup(text).map_err(|e| ResolveError::Errno {
        stage: "proc-cgroup",
        errno: errno_of(&e),
    })?;
    let path_ino = ino(&path).map_err(|e| ResolveError::Errno {
        stage: "stat-cgroup",
        errno: errno_of(&e),
    })?;
    if let Some(id) = expected
        && id != path_ino
    {
        return Err(ResolveError::Mismatch {
            cgroup_id: id,
            path_ino,
            path,
        });
    }
    let agent_dir = agent_dir_of(&path);
    let root = find_registered_root(agent_dir, reg, ino)
        .map_err(|e| ResolveError::Errno {
            stage: "stat-ancestor",
            errno: errno_of(&e),
        })?
        .ok_or_else(|| ResolveError::NotInAgent { path: path.clone() })?;
    let on_behalf_of = ino(root)
        .ok()
        .and_then(|id| reg.get(id))
        .and_then(|e| e.on_behalf_of.clone());
    let agent_id = agent_id_under(root, agent_dir).ok_or(ResolveError::Errno {
        stage: "agent-id",
        errno: libc::EILSEQ,
    })?;
    let parent = agent_id.rsplit_once('/').map(|(p, _)| p.to_string());
    Ok(Identity {
        agent_id,
        on_behalf_of,
        parent,
        cgroup_id: path_ino,
    })
}

fn read_proc_cgroup(pid: i32) -> Result<String, ResolveError> {
    fs::read_to_string(format!("/proc/{pid}/cgroup")).map_err(|e| ResolveError::Errno {
        stage: "proc-cgroup",
        errno: errno_of(&e),
    })
}

fn load_for_resolve() -> Result<Registry, ResolveError> {
    load_registry().map_err(|e| ResolveError::Errno {
        stage: "registry",
        errno: errno_of(&e),
    })
}

/// Resolve the identity of the task a pidfd names, by the primary route. The
/// pidfd is the caller's and is not closed. A pidfd names one task for as long
/// as it is held, so this route is reuse-proof.
pub fn resolve(pidfd: RawFd) -> Result<Identity, ResolveError> {
    let info = sys::pidfd_identity(pidfd).map_err(|e| classify_info_error(errno_of(&e)))?;
    if info.cgroupid == 0 {
        return Err(ResolveError::NoCgroupId);
    }
    if info.pid <= 0 {
        return Err(ResolveError::Errno {
            stage: "pidfd-get-info",
            errno: libc::ESRCH,
        });
    }
    let text = read_proc_cgroup(info.pid)?;
    let reg = load_for_resolve()?;
    identity_from_cgroup_text(Some(info.cgroupid), &text, &reg, &stat_ino)
}

/// Resolve with the pid-based fallback available. Tries the pidfd route
/// first when `pidfd` is given; only `NoPidfdInfo` (ENOTTY) moves on to the
/// fallback, which needs `known_pid`. `pid_taken` is when that pid was
/// obtained, the start of the window the fallback reports.
pub fn resolve_route(
    pidfd: Option<RawFd>,
    known_pid: Option<i32>,
    pid_taken: Instant,
) -> Result<Resolved, ResolveError> {
    if let Some(fd) = pidfd {
        match resolve(fd) {
            Ok(identity) => {
                return Ok(Resolved {
                    identity,
                    route: Route::Pidfd,
                });
            }
            Err(ResolveError::NoPidfdInfo) => {}
            Err(e) => return Err(e),
        }
    }
    let pid = known_pid.ok_or(ResolveError::NoPidfdInfo)?;
    let text = read_proc_cgroup(pid)?;
    let window_ns = pid_taken.elapsed().as_nanos();
    // If a pidfd is held, a process that has exited by now may have been
    // replaced under the same pid: refuse rather than guess.
    if let Some(fd) = pidfd
        && sys::poll_readable(fd, std::time::Duration::ZERO).unwrap_or(false)
    {
        return Err(ResolveError::Exited);
    }
    let reg = load_for_resolve()?;
    let identity = identity_from_cgroup_text(None, &text, &reg, &stat_ino)?;
    Ok(Resolved {
        identity,
        route: Route::Fallback { window_ns },
    })
}

// ---------------------------------------------------------------------------
// whois.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Pid(i32),
    Pidfd(RawFd),
    Listen(PathBuf),
}

fn parse_whois(args: &[String]) -> Result<Target, String> {
    let [flag, value] = args else {
        return Err("whois: expected --pid P | --pidfd FD | --listen SOCK".into());
    };
    match flag.as_str() {
        "--pid" => match value.parse::<i32>() {
            Ok(p) if p > 0 => Ok(Target::Pid(p)),
            _ => Err(format!("whois: --pid {value:?} is not a positive pid")),
        },
        "--pidfd" => match value.parse::<RawFd>() {
            Ok(fd) if fd >= 0 => Ok(Target::Pidfd(fd)),
            _ => Err(format!("whois: --pidfd {value:?} is not a file descriptor")),
        },
        "--listen" if !value.is_empty() => Ok(Target::Listen(PathBuf::from(value))),
        other => Err(format!("whois: unknown or incomplete argument {other:?}")),
    }
}

/// The `Pid:` line of a pidfd's `/proc/self/fdinfo/FD`.
fn fdinfo_pid(text: &str) -> Option<i32> {
    text.lines()
        .find_map(|l| l.strip_prefix("Pid:"))
        .and_then(|v| v.trim().parse().ok())
        .filter(|p| *p > 0)
}

/// What `whois` prints on success, one `key=value` per line. `path=` names
/// the route, not a filesystem path.
fn render(r: &Resolved) -> String {
    let i = &r.identity;
    let mut out = format!(
        "agent_id={}\non_behalf_of={}\nparent={}\ncgroup_id={}\n",
        i.agent_id,
        i.on_behalf_of.as_deref().unwrap_or("none"),
        i.parent.as_deref().unwrap_or("none"),
        i.cgroup_id
    );
    match r.route {
        Route::Pidfd => out.push_str("path=pidfd\n"),
        Route::Fallback { window_ns } => {
            out.push_str("path=fallback\n");
            out.push_str(&format!("window_ns={window_ns}\n"));
        }
    }
    out
}

/// Removes the listening socket's filesystem entry when `whois` is done.
struct SocketFile(PathBuf);

impl Drop for SocketFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn whois_target(target: Target) -> Result<Resolved, Fail> {
    match target {
        Target::Pid(pid) => {
            let taken = Instant::now();
            let fd =
                sys::owned_fd(sys::pidfd_open(pid, 0).map_err(|e| Fail::io("pidfd-open", &e))?);
            Ok(resolve_route(Some(fd.as_raw_fd()), Some(pid), taken)?)
        }
        Target::Pidfd(fd) => {
            // The caller's fd: used, never closed. Its pid, for the fallback,
            // comes from fdinfo.
            let taken = Instant::now();
            let pid = fs::read_to_string(format!("/proc/self/fdinfo/{fd}"))
                .ok()
                .and_then(|t| fdinfo_pid(&t));
            Ok(resolve_route(Some(fd), pid, taken)?)
        }
        Target::Listen(sock) => {
            let listener = UnixListener::bind(&sock).map_err(|e| Fail::io("listen: bind", &e))?;
            let _cleanup = SocketFile(sock);
            let (stream, _) = listener
                .accept()
                .map_err(|e| Fail::io("listen: accept", &e))?;
            let taken = Instant::now();
            let peer = sys::peer_pidfd(stream.as_raw_fd())
                .map_err(|e| Fail::io("listen: SO_PEERPIDFD", &e))?;
            let cred_pid = sys::peer_cred_pid(stream.as_raw_fd()).ok();
            Ok(resolve_route(Some(peer.as_raw_fd()), cred_pid, taken)?)
        }
    }
}

/// `aok-launch whois ARGS...`: `args` is everything after `whois`. Returns
/// the process exit code: 0 and the record on stdout, 64 for a usage error,
/// 125 when resolution fails.
pub fn whois(args: &[String]) -> i32 {
    let target = match parse_whois(args) {
        Ok(t) => t,
        Err(m) => {
            eprintln!("aok-launch: usage: {m}");
            return EXIT_USAGE;
        }
    };
    match whois_target(target) {
        Ok(r) => {
            print!("{}", render(&r));
            0
        }
        Err(f) => {
            eprintln!("{}", f.text);
            f.exit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const GOOD: &str = "42\t/sys/fs/cgroup/s/r7\talice@example.org";

    #[test]
    fn parse_accepts_a_good_line_and_an_empty_principal() {
        let reg = Registry::parse(&format!("{GOOD}\n43\t/sys/fs/cgroup/s/r8\t\n"));
        assert!(reg.warnings.is_empty(), "{:?}", reg.warnings);
        assert_eq!(
            reg.get(42).unwrap().on_behalf_of.as_deref(),
            Some("alice@example.org")
        );
        assert_eq!(reg.get(43).unwrap().on_behalf_of, None);
    }

    #[test]
    fn parse_skips_each_kind_of_bad_line_with_a_warning() {
        let bad = [
            "junk",
            "42\t/sys/fs/cgroup/s/r7",
            "42\t/sys/fs/cgroup/s/r7\ta\tb",
            "x\t/sys/fs/cgroup/s/r7\ta",
            "+5\t/sys/fs/cgroup/s/r7\ta",
            "0\t/sys/fs/cgroup/s/r7\ta",
            "99999999999999999999999\t/sys/fs/cgroup/s/r7\ta",
            "5\t/etc/passwd\ta",
            "5\trelative/path\ta",
            "5\t/sys/fs/cgroup/../etc\ta",
            "5\t/sys/fs/cgroup/s/r7\ta b",
            "5\t/sys/fs/cgroup/s/r7\ta/b",
        ];
        for line in bad {
            let reg = Registry::parse(&format!("{line}\n{GOOD}\n"));
            assert_eq!(reg.entries.len(), 1, "{line:?}");
            assert_eq!(reg.warnings.len(), 1, "{line:?}");
            assert!(reg.get(42).is_some(), "{line:?}");
        }
        let long = format!(
            "5\t/sys/fs/cgroup/s/r7\t{}",
            "a".repeat(ON_BEHALF_OF_MAX + 1)
        );
        assert_eq!(Registry::parse(&long).entries.len(), 0);
        let huge = format!("5\t/sys/fs/cgroup/{}\ta", "x".repeat(LINE_MAX));
        assert_eq!(Registry::parse(&huge).entries.len(), 0);
    }

    #[test]
    fn parse_keeps_the_first_entry_for_a_duplicate_id() {
        let reg = Registry::parse(&format!("{GOOD}\n42\t/sys/fs/cgroup/s/r7\tmallory\n"));
        assert_eq!(reg.entries.len(), 1);
        assert_eq!(
            reg.get(42).unwrap().on_behalf_of.as_deref(),
            Some("alice@example.org")
        );
        assert_eq!(reg.warnings.len(), 1);
    }

    #[test]
    fn render_round_trips() {
        let reg = Registry::parse(&format!("{GOOD}\n43\t/sys/fs/cgroup/s/r8\t\n"));
        assert_eq!(Registry::parse(&reg.render()), reg);
    }

    #[test]
    fn on_behalf_of_charset() {
        assert!(valid_on_behalf_of("alice@example.org"));
        assert!(valid_on_behalf_of("uid:1000"));
        assert!(!valid_on_behalf_of(""));
        assert!(!valid_on_behalf_of("a b"));
        assert!(!valid_on_behalf_of("a\tb"));
        assert!(!valid_on_behalf_of("a\nb"));
        assert!(!valid_on_behalf_of(&"a".repeat(ON_BEHALF_OF_MAX + 1)));
        assert!(valid_on_behalf_of(&"a".repeat(ON_BEHALF_OF_MAX)));
    }

    #[test]
    fn registry_path_uses_the_real_uid_not_the_environment() {
        let dir = registry_dir();
        assert_eq!(
            dir,
            PathBuf::from(format!("/run/user/{}/aok-launch", sys::real_uid()))
        );
    }

    #[test]
    fn only_enotty_selects_the_fallback() {
        assert!(matches!(
            classify_info_error(libc::ENOTTY),
            ResolveError::NoPidfdInfo
        ));
        for errno in [
            libc::EPERM,
            libc::EINVAL,
            libc::EBADF,
            libc::ENOSYS,
            libc::EOPNOTSUPP,
            libc::ESRCH,
            libc::EACCES,
            libc::EFAULT,
        ] {
            match classify_info_error(errno) {
                ResolveError::Errno { stage, errno: e } => {
                    assert_eq!((stage, e), ("pidfd-get-info", errno));
                }
                other => panic!("errno {errno} was classified as {other:?}"),
            }
        }
        let f: Fail = classify_info_error(libc::EPERM).into();
        assert_eq!(f.exit, EXIT_REFUSED);
        assert!(f.text.contains("EPERM"), "{}", f.text);
    }

    fn inos(pairs: &[(&str, u64)]) -> HashMap<PathBuf, u64> {
        pairs.iter().map(|(p, i)| (PathBuf::from(p), *i)).collect()
    }

    fn lookup(map: &HashMap<PathBuf, u64>) -> impl Fn(&Path) -> io::Result<u64> + '_ {
        |p| {
            map.get(p)
                .copied()
                .ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))
        }
    }

    fn registry(ids: &[(u64, &str)]) -> Registry {
        Registry {
            entries: ids
                .iter()
                .map(|(id, obo)| Entry {
                    cgroup_id: *id,
                    path: PathBuf::from("/sys/fs/cgroup/ignored"),
                    on_behalf_of: Some(obo.to_string()),
                })
                .collect(),
            warnings: vec![],
        }
    }

    #[test]
    fn identity_drops_main_and_walks_to_the_root_agent() {
        let m = inos(&[
            ("/sys/fs/cgroup/s", 1),
            ("/sys/fs/cgroup/s/r7", 2),
            ("/sys/fs/cgroup/s/r7/s1", 3),
            ("/sys/fs/cgroup/s/r7/s1/s2", 4),
            ("/sys/fs/cgroup/s/r7/s1/s2/main", 5),
        ]);
        let reg = registry(&[(2, "alice")]);
        let id =
            identity_from_cgroup_text(Some(5), "0::/s/r7/s1/s2/main\n", &reg, &lookup(&m)).unwrap();
        assert_eq!(id.agent_id, "r7/s1/s2");
        assert_eq!(id.parent.as_deref(), Some("r7/s1"));
        assert_eq!(id.on_behalf_of.as_deref(), Some("alice"));
        assert_eq!(id.cgroup_id, 5);

        // The root agent itself has no parent.
        let id = identity_from_cgroup_text(Some(2), "0::/s/r7\n", &reg, &lookup(&m)).unwrap();
        assert_eq!((id.agent_id.as_str(), id.parent), ("r7", None));
    }

    #[test]
    fn identity_rejects_a_path_and_id_that_disagree() {
        let m = inos(&[("/sys/fs/cgroup/s/r7", 2), ("/sys/fs/cgroup/s/r7/main", 3)]);
        let reg = registry(&[(2, "a")]);
        let e =
            identity_from_cgroup_text(Some(99), "0::/s/r7/main\n", &reg, &lookup(&m)).unwrap_err();
        assert!(matches!(
            e,
            ResolveError::Mismatch {
                cgroup_id: 99,
                path_ino: 3,
                ..
            }
        ));
        assert!(e.to_string().contains("disagrees"));
    }

    #[test]
    fn identity_outside_every_registered_root_is_an_error() {
        let m = inos(&[("/sys/fs/cgroup/s", 1), ("/sys/fs/cgroup/s/x", 2)]);
        let e = identity_from_cgroup_text(None, "0::/s/x\n", &registry(&[(7, "a")]), &lookup(&m))
            .unwrap_err();
        assert!(matches!(e, ResolveError::NotInAgent { .. }));
    }

    #[test]
    fn nearest_registered_root_wins() {
        let m = inos(&[
            ("/sys/fs/cgroup/s/a", 10),
            ("/sys/fs/cgroup/s/a/b", 11),
            ("/sys/fs/cgroup/s/a/b/main", 12),
        ]);
        let reg = registry(&[(10, "outer"), (11, "inner")]);
        let id =
            identity_from_cgroup_text(Some(12), "0::/s/a/b/main\n", &reg, &lookup(&m)).unwrap();
        assert_eq!(id.on_behalf_of.as_deref(), Some("inner"));
        assert_eq!(id.agent_id, "b");
    }

    #[test]
    fn a_non_main_directory_is_attributed_to_itself() {
        let m = inos(&[("/sys/fs/cgroup/s/r7", 2), ("/sys/fs/cgroup/s/r7/esc", 3)]);
        let reg = registry(&[(2, "a")]);
        let id = identity_from_cgroup_text(None, "0::/s/r7/esc\n", &reg, &lookup(&m)).unwrap();
        assert_eq!(id.agent_id, "r7/esc");
    }

    fn mgr_role() -> (PathBuf, Role) {
        let own = PathBuf::from("/sys/fs/cgroup/s/aok-mgr");
        let role = cgroup::role_from_path(&own);
        (own, role)
    }

    #[test]
    fn sub_role_never_touches_the_registry_and_refuses_on_behalf_of() {
        let own = PathBuf::from("/sys/fs/cgroup/s/r7/main");
        let role = cgroup::role_from_path(&own);
        let panics = || -> io::Result<Registry> { panic!("a sub launch read the registry") };
        let m = inos(&[]);
        assert!(decide_role(&own, role.clone(), None, &panics, &lookup(&m)).is_ok());
        let e = decide_role(&own, role, Some("h"), &panics, &lookup(&m)).unwrap_err();
        assert_eq!(
            (e.errno, e.kind),
            (libc::EACCES, crate::landlock::LayerErrorKind::Refused)
        );
    }

    #[test]
    fn root_role_is_refused_when_the_registry_read_is_eacces() {
        let (own, role) = mgr_role();
        let denied = || Err(io::Error::from_raw_os_error(libc::EACCES));
        let m = inos(&[]);
        let e = decide_role(&own, role, Some("h"), &denied, &lookup(&m)).unwrap_err();
        assert_eq!(e.errno, libc::EACCES);
    }

    #[test]
    fn root_role_is_refused_under_a_registered_ancestor_and_allowed_outside() {
        let own = PathBuf::from("/sys/fs/cgroup/s/r7/esc");
        let role = cgroup::role_from_path(&own);
        assert!(matches!(role, Role::Root(_)));
        let m = inos(&[
            ("/sys/fs/cgroup/s", 1),
            ("/sys/fs/cgroup/s/r7", 2),
            ("/sys/fs/cgroup/s/r7/esc", 3),
        ]);
        let reg = || Ok(registry(&[(2, "a")]));
        let e = decide_role(&own, role.clone(), Some("h"), &reg, &lookup(&m)).unwrap_err();
        assert_eq!(e.errno, libc::EACCES);
        // The same launcher with an unrelated registry entry is a plain root.
        let other = || Ok(registry(&[(99, "a")]));
        assert!(decide_role(&own, role, Some("h"), &other, &lookup(&m)).is_ok());
    }

    #[test]
    fn root_role_with_a_malformed_principal_is_a_usage_error() {
        let (own, role) = mgr_role();
        let empty = || Ok(Registry::default());
        let m = inos(&[("/sys/fs/cgroup/s", 1), ("/sys/fs/cgroup/s/aok-mgr", 2)]);
        let e = decide_role(&own, role, Some("a b"), &empty, &lookup(&m)).unwrap_err();
        assert_eq!(e.kind, crate::landlock::LayerErrorKind::Usage);
    }

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn whois_argument_parsing() {
        assert_eq!(parse_whois(&v(&["--pid", "12"])), Ok(Target::Pid(12)));
        assert_eq!(parse_whois(&v(&["--pidfd", "3"])), Ok(Target::Pidfd(3)));
        assert_eq!(
            parse_whois(&v(&["--listen", "/tmp/s"])),
            Ok(Target::Listen(PathBuf::from("/tmp/s")))
        );
        for bad in [
            v(&[]),
            v(&["--pid"]),
            v(&["--pid", "0"]),
            v(&["--pid", "-4"]),
            v(&["--pid", "x"]),
            v(&["--pidfd", "-1"]),
            v(&["--listen", ""]),
            v(&["--bogus", "1"]),
            v(&["--pid", "1", "--pid", "2"]),
        ] {
            assert!(parse_whois(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn fdinfo_pid_parses() {
        assert_eq!(
            fdinfo_pid("pos:\t0\nflags:\t02000002\nPid:\t4242\nNSpid:\t4242\n"),
            Some(4242)
        );
        assert_eq!(fdinfo_pid("pos:\t0\nPid:\t-1\n"), None);
        assert_eq!(fdinfo_pid("pos:\t0\n"), None);
    }

    #[test]
    fn render_names_the_route() {
        let identity = Identity {
            agent_id: "r7/s1".into(),
            on_behalf_of: Some("alice".into()),
            parent: Some("r7".into()),
            cgroup_id: 9,
        };
        let p = render(&Resolved {
            identity: identity.clone(),
            route: Route::Pidfd,
        });
        assert_eq!(
            p,
            "agent_id=r7/s1\non_behalf_of=alice\nparent=r7\ncgroup_id=9\npath=pidfd\n"
        );
        let f = render(&Resolved {
            identity,
            route: Route::Fallback { window_ns: 1234 },
        });
        assert!(f.ends_with("path=fallback\nwindow_ns=1234\n"), "{f}");
    }
}
