//! cgroup v2 bookkeeping: which role the launcher plays, the agent layout
//! (D-07), root evacuation, and the freeze, thaw and kill lifecycle. Safe
//! Rust over std fs only.
//!
//! Layout under a delegated root:
//!
//! ```text
//! <root>/aok-mgr          manager leaf: the launcher and any bystanders
//! <root>/<name>           no processes; +memory +pids enabled for children
//! <root>/<name>/main      the leaf that holds the agent's processes
//! <root>/<name>/<sub>     a sub-agent, same shape, built by a Sub launch
//! ```

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

/// Where the cgroup v2 hierarchy is mounted.
pub const CGROUP_MOUNT: &str = "/sys/fs/cgroup";
/// The manager leaf a Root launch evacuates the delegated root into.
pub const MGR_LEAF: &str = "aok-mgr";
/// The leaf that holds an agent's processes.
pub const MAIN_LEAF: &str = "main";
/// Longest agent name accepted. The kernel allows 255 bytes per component;
/// this stays well below that so a nest chain cannot approach PATH_MAX.
pub const NAME_MAX_LEN: usize = 128;

const FREEZE_POLLS: u32 = 1000;
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const RMDIR_TRIES: u32 = 200;
const EVACUATE_PASSES: u32 = 50;

/// Which kind of launch this is (D-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Run from the delegated root (or its `aok-mgr` leaf). Holds the
    /// delegated root's path.
    Root(PathBuf),
    /// Run from inside an agent's `main` leaf. Holds that agent's directory,
    /// the only place this launch may write.
    Sub(PathBuf),
}

impl Role {
    pub fn label(&self) -> &'static str {
        match self {
            Role::Root(_) => "root",
            Role::Sub(_) => "sub",
        }
    }

    /// The directory a new agent is created directly under.
    pub fn parent_dir(&self) -> &Path {
        match self {
            Role::Root(p) | Role::Sub(p) => p,
        }
    }
}

/// A name-validation failure: an errno and a reason.
#[derive(Debug, PartialEq, Eq)]
pub struct NameError {
    pub errno: i32,
    pub reason: &'static str,
}

/// Validate an agent name before any mkdir. `EINVAL` for a shape violation,
/// `ENAMETOOLONG` for the length guard. Beyond the empty, `/`, `..` and NUL
/// cases, names that collide with the layout (`.`, `main`, `aok-mgr`) or with
/// kernel control files (`cgroup.*`) are refused: a sub-agent named `main`
/// would be the parent's process leaf.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    let bad = |reason| {
        Err(NameError {
            errno: libc::EINVAL,
            reason,
        })
    };
    if name.is_empty() {
        return bad("empty name");
    }
    if name.len() > NAME_MAX_LEN {
        return Err(NameError {
            errno: libc::ENAMETOOLONG,
            reason: "name exceeds the length guard",
        });
    }
    if name.contains('/') {
        return bad("name contains '/'");
    }
    if name.contains('\0') {
        return bad("name contains a NUL byte");
    }
    if name == "." || name == ".." || name.contains("..") {
        return bad("name contains '..'");
    }
    if name == MAIN_LEAF || name == MGR_LEAF {
        return bad("name collides with a reserved leaf");
    }
    if name.starts_with("cgroup.") {
        return bad("name collides with a cgroup control file");
    }
    Ok(())
}

/// Pure half of `launch_role`: classify a cgroup path (as in
/// `/proc/self/cgroup`, or already joined onto the mount).
pub fn role_from_path(own: &Path) -> Role {
    match own.file_name().and_then(|n| n.to_str()) {
        Some(MAIN_LEAF) => match own.parent() {
            Some(agent) => Role::Sub(agent.to_path_buf()),
            None => Role::Root(own.to_path_buf()),
        },
        Some(MGR_LEAF) => match own.parent() {
            Some(root) => Role::Root(root.to_path_buf()),
            None => Role::Root(own.to_path_buf()),
        },
        _ => Role::Root(own.to_path_buf()),
    }
}

/// The launcher's own cgroup, as a filesystem path under the mount, from the
/// `0::` line of `/proc/self/cgroup`.
pub fn own_cgroup() -> io::Result<PathBuf> {
    let text = fs::read_to_string("/proc/self/cgroup")?;
    parse_own_cgroup(&text)
}

/// Pure half of `own_cgroup`, split out for tests.
pub fn parse_own_cgroup(text: &str) -> io::Result<PathBuf> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("0::") {
            let rel = rest.trim_start_matches('/');
            return Ok(Path::new(CGROUP_MOUNT).join(rel));
        }
    }
    Err(io::Error::from_raw_os_error(libc::ENODEV))
}

/// D-12: tell root from sub by the launcher's own cgroup path. `identity`
/// adds the registry half on top of this.
pub fn launch_role() -> io::Result<Role> {
    Ok(role_from_path(&own_cgroup()?))
}

fn mkdir_ok(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// Write `value` to a cgroup control file. A fresh open per write, no
/// truncate: cgroup files take one write per command.
pub fn write_file(path: &Path, value: &str) -> io::Result<()> {
    let mut f = fs::OpenOptions::new().write(true).open(path)?;
    f.write_all(value.as_bytes())
}

/// Move every pid in `<root>/cgroup.procs` into `<root>/aok-mgr`, then enable
/// `+memory +pids` for the root's children. Root launch only.
///
/// Every pid, not only the launcher's own: under `systemd-run ... cargo test`
/// cargo and the test harness also sit in the scope root, and the
/// no-internal-process rule fails the controller write with `EBUSY` while any
/// process remains there. Idempotent: a second root launch from inside
/// `aok-mgr` finds the root's own list empty.
pub fn evacuate_root(root: &Path) -> io::Result<()> {
    // Never evacuate the hierarchy root: that is every process on the host.
    if root == Path::new(CGROUP_MOUNT) {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    let mgr = root.join(MGR_LEAF);
    mkdir_ok(&mgr)?;
    let dest = mgr.join("cgroup.procs");
    let mut drained = false;
    for _ in 0..EVACUATE_PASSES {
        let procs = fs::read_to_string(root.join("cgroup.procs"))?;
        let pids: Vec<&str> = procs
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if pids.is_empty() {
            drained = true;
            break;
        }
        for pid in pids {
            match write_file(&dest, pid) {
                Ok(()) => {}
                // The pid exited between the listing and the write.
                Err(e) if e.raw_os_error() == Some(libc::ESRCH) => {}
                Err(e) => return Err(e),
            }
        }
    }
    if !drained {
        return Err(io::Error::from_raw_os_error(libc::EBUSY));
    }
    write_file(&root.join("cgroup.subtree_control"), "+memory +pids")
}

/// Create `<parent>/<name>` and `<parent>/<name>/main` (EEXIST-tolerant) and
/// enable `+memory +pids` on `<parent>/<name>`. Returns the agent directory
/// and its `main` leaf. Both launch roles use this; a Sub launch passes its
/// own agent directory as `parent` and so never writes outside it.
pub fn make_agent(parent: &Path, name: &str) -> io::Result<(PathBuf, PathBuf)> {
    let agent = parent.join(name);
    let leaf = agent.join(MAIN_LEAF);
    mkdir_ok(&agent)?;
    mkdir_ok(&leaf)?;
    write_file(&agent.join("cgroup.subtree_control"), "+memory +pids")?;
    Ok((agent, leaf))
}

/// Parse `cgroup.events` text into the integer value of `key`.
pub fn parse_event(text: &str, key: &str) -> Option<i64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(' ')?;
        if k == key {
            v.trim().parse().ok()
        } else {
            None
        }
    })
}

/// Read one key of `<dir>/cgroup.events` (`populated`, `frozen`).
pub fn event(dir: &Path, key: &str) -> io::Result<i64> {
    let text = fs::read_to_string(dir.join("cgroup.events"))?;
    parse_event(&text, key).ok_or_else(|| io::Error::from_raw_os_error(libc::EIO))
}

/// `dir` and every cgroup directory below it, parents before children.
pub fn subtree(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = vec![dir.to_path_buf()];
    let mut i = 0;
    while i < out.len() {
        let cur = out[i].clone();
        i += 1;
        let mut kids = BTreeSet::new();
        for ent in fs::read_dir(&cur)? {
            let ent = ent?;
            if ent.file_type()?.is_dir() {
                kids.insert(ent.path());
            }
        }
        out.extend(kids);
    }
    Ok(out)
}

/// Refuse to act on a cgroup the launcher lives in. Killing or freezing
/// `agent` reaches every cgroup below it, so the launcher's own cgroup must
/// not be `agent` or inside it (RESEARCH Pitfall 2, T-09-02). Component-wise,
/// so `/a/b` does not shadow `/a/bc`.
pub fn check_not_inside(own: &Path, agent: &Path) -> io::Result<()> {
    if own.starts_with(agent) {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    Ok(())
}

fn wait_for(dirs: &[PathBuf], key: &str, want: i64) -> io::Result<()> {
    for _ in 0..FREEZE_POLLS {
        let mut all = true;
        for d in dirs {
            if event(d, key)? != want {
                all = false;
                break;
            }
        }
        if all {
            return Ok(());
        }
        sleep(POLL_INTERVAL);
    }
    Err(io::Error::from_raw_os_error(libc::ETIMEDOUT))
}

/// Freeze (`true`) or thaw (`false`) the agent directory and wait until
/// `cgroup.events` reads the target `frozen` value on the agent directory and
/// on every cgroup below it. The write goes to the non-leaf agent directory;
/// the kernel propagates it to the whole subtree (RESEARCH item 7).
pub fn set_frozen(agent: &Path, freeze: bool) -> io::Result<()> {
    check_not_inside(&own_cgroup()?, agent)?;
    write_file(&agent.join("cgroup.freeze"), if freeze { "1" } else { "0" })?;
    wait_for(&subtree(agent)?, "frozen", i64::from(freeze))
}

/// `rmdir` with a bounded retry on `EBUSY`, mirroring `cg.c`'s
/// `cg_kill_and_rmdir` (about 200 x 5 ms): after `cgroup.kill` the dead tasks
/// can take a moment to be reaped and the directory stays busy until then.
pub fn rmdir_retry(dir: &Path) -> io::Result<()> {
    for _ in 0..RMDIR_TRIES {
        match fs::remove_dir(dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.raw_os_error() == Some(libc::EBUSY) => sleep(POLL_INTERVAL),
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::from_raw_os_error(libc::EBUSY))
}

/// Kill every process in the agent's subtree: write `cgroup.kill`, wait until
/// `populated` reads 0, then remove the emptied directories deepest first.
/// Returns after the agent directory is gone.
pub fn kill_agent(agent: &Path) -> io::Result<()> {
    check_not_inside(&own_cgroup()?, agent)?;
    write_file(&agent.join("cgroup.kill"), "1")?;
    wait_for(&[agent.to_path_buf()], "populated", 0)?;
    println!("aok-launch: populated=0 agent={}", agent.display());
    let mut dirs = subtree(agent)?;
    dirs.reverse();
    for d in dirs {
        rmdir_retry(&d)?;
    }
    Ok(())
}

/// Resolve the `<agent>` argument of freeze, thaw and kill. An absolute path
/// must lie under the cgroup mount and carry no `..`; a bare name is
/// validated and resolved under the role's parent directory (the root for a
/// Root launcher, the own agent directory for a Sub launcher), so a Sub
/// launcher can name only its own descendants.
pub fn resolve_agent(arg: &str, role: &Role) -> io::Result<PathBuf> {
    let einval = || io::Error::from_raw_os_error(libc::EINVAL);
    if arg.starts_with('/') {
        let p = PathBuf::from(arg);
        if !p.starts_with(CGROUP_MOUNT)
            || p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(einval());
        }
        return Ok(p);
    }
    validate_name(arg).map_err(|e| io::Error::from_raw_os_error(e.errno))?;
    Ok(role.parent_dir().join(arg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_guard_rejects_shapes() {
        for bad in [
            "",
            "a/b",
            "/",
            "..",
            "a..b",
            ".",
            "main",
            "aok-mgr",
            "cgroup.kill",
        ] {
            assert_eq!(
                validate_name(bad).unwrap_err().errno,
                libc::EINVAL,
                "{bad:?}"
            );
        }
        assert_eq!(validate_name("a\0b").unwrap_err().errno, libc::EINVAL);
    }

    #[test]
    fn name_guard_length() {
        assert!(validate_name(&"a".repeat(NAME_MAX_LEN)).is_ok());
        assert_eq!(
            validate_name(&"a".repeat(NAME_MAX_LEN + 1))
                .unwrap_err()
                .errno,
            libc::ENAMETOOLONG
        );
    }

    #[test]
    fn name_guard_accepts_plain_names() {
        for ok in ["agent", "a-1", "d2", "x.y", "Agent_7"] {
            assert!(validate_name(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn role_sub_from_main_leaf() {
        let r = role_from_path(Path::new("/sys/fs/cgroup/u/scope/agent/main"));
        assert_eq!(r, Role::Sub(PathBuf::from("/sys/fs/cgroup/u/scope/agent")));
        assert_eq!(r.label(), "sub");
    }

    #[test]
    fn role_root_from_mgr_leaf_and_plain() {
        assert_eq!(
            role_from_path(Path::new("/sys/fs/cgroup/u/scope/aok-mgr")),
            Role::Root(PathBuf::from("/sys/fs/cgroup/u/scope"))
        );
        assert_eq!(
            role_from_path(Path::new("/sys/fs/cgroup/u/scope")),
            Role::Root(PathBuf::from("/sys/fs/cgroup/u/scope"))
        );
    }

    #[test]
    fn parse_own_cgroup_reads_v2_line() {
        let p = parse_own_cgroup("0::/user.slice/run-x.scope\n").unwrap();
        assert_eq!(p, PathBuf::from("/sys/fs/cgroup/user.slice/run-x.scope"));
        assert!(parse_own_cgroup("1:cpu:/x\n").is_err());
        assert_eq!(
            parse_own_cgroup("0::/\n").unwrap(),
            PathBuf::from("/sys/fs/cgroup/")
        );
    }

    #[test]
    fn events_parse() {
        let t = "populated 1\nfrozen 0\n";
        assert_eq!(parse_event(t, "populated"), Some(1));
        assert_eq!(parse_event(t, "frozen"), Some(0));
        assert_eq!(parse_event(t, "missing"), None);
    }

    #[test]
    fn launcher_never_targets_its_own_cgroup() {
        let own = Path::new("/sys/fs/cgroup/s/aok-mgr");
        // Killing the delegated root would take the launcher with it.
        assert!(check_not_inside(own, Path::new("/sys/fs/cgroup/s")).is_err());
        // Killing the leaf it lives in, likewise.
        assert!(check_not_inside(own, own).is_err());
        // A sibling agent is fine, and so is a prefix-sharing name.
        assert!(check_not_inside(own, Path::new("/sys/fs/cgroup/s/agent")).is_ok());
        assert!(check_not_inside(own, Path::new("/sys/fs/cgroup/s/aok-mgr2")).is_ok());
        // A sub launcher inside A/main may target its own child A/d2.
        let sub = Path::new("/sys/fs/cgroup/s/A/main");
        assert!(check_not_inside(sub, Path::new("/sys/fs/cgroup/s/A/d2")).is_ok());
        assert!(check_not_inside(sub, Path::new("/sys/fs/cgroup/s/A")).is_err());
    }

    #[test]
    fn resolve_agent_confines_paths() {
        let role = Role::Root(PathBuf::from("/sys/fs/cgroup/s"));
        assert_eq!(
            resolve_agent("a1", &role).unwrap(),
            PathBuf::from("/sys/fs/cgroup/s/a1")
        );
        assert!(resolve_agent("../x", &role).is_err());
        assert!(resolve_agent("/etc/passwd", &role).is_err());
        assert!(resolve_agent("/sys/fs/cgroup/s/../../x", &role).is_err());
        assert!(resolve_agent("/sys/fs/cgroup/s/a1", &role).is_ok());
    }
}
