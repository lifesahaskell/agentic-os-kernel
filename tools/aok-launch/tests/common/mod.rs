//! Delegated-scope fixture shared by the integration tests.
//!
//! Run these under `systemd-run --user --scope -q -p Delegate=yes -- cargo
//! test ...`. The fixture finds the delegated root from the test process's
//! own cgroup, hands out uniquely named throwaway agents under it, and tears
//! each one down on drop. It computes the root independently of the code
//! under test: after the first root launch the harness itself has been
//! evacuated into `<root>/aok-mgr`, so a trailing `aok-mgr` is stripped.
//!
//! Teardown never kills a cgroup that contains the test process: the agent is
//! asserted not to be the test process's cgroup or an ancestor of it first.

#![allow(dead_code)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const MOUNT: &str = "/sys/fs/cgroup";

/// The test process's own cgroup as a filesystem path.
pub fn own_cgroup() -> PathBuf {
    let text = fs::read_to_string("/proc/self/cgroup").expect("read /proc/self/cgroup");
    let rel = text
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .expect("cgroup v2 line");
    Path::new(MOUNT).join(rel.trim_start_matches('/'))
}

/// The delegated root: the own cgroup, minus a trailing `aok-mgr`.
pub fn scope_root() -> PathBuf {
    let own = own_cgroup();
    if own.file_name().is_some_and(|n| n == "aok-mgr") {
        own.parent().unwrap().to_path_buf()
    } else {
        own
    }
}

/// `/proc/self/cgroup` line for a path under the mount, e.g. `0::/a/b`.
pub fn cgroup_line(dir: &Path) -> String {
    let rel = dir.strip_prefix(MOUNT).expect("under mount");
    format!("0::/{}", rel.display())
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A uniquely named agent directory under the delegated root.
pub struct Agent {
    pub name: String,
    pub dir: PathBuf,
}

impl Agent {
    pub fn new(tag: &str) -> Agent {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let name = format!("t{}-{}-{}", std::process::id(), tag, n);
        let dir = scope_root().join(&name);
        Agent { name, dir }
    }

    pub fn leaf(&self) -> PathBuf {
        self.dir.join("main")
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        if !self.dir.exists() {
            return;
        }
        let own = own_cgroup();
        assert!(
            !own.starts_with(&self.dir),
            "refusing to tear down a cgroup holding the test process: {}",
            self.dir.display()
        );
        remove_tree(&self.dir);
    }
}

fn remove_tree(dir: &Path) {
    let _ = fs::write(dir.join("cgroup.kill"), "1");
    let mut kids = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                kids.push(e.path());
            }
        }
    }
    for k in kids {
        remove_tree(&k);
    }
    for _ in 0..200 {
        match fs::remove_dir(dir) {
            Ok(()) => return,
            Err(e) if e.raw_os_error() == Some(libc::EBUSY) => sleep(Duration::from_millis(5)),
            Err(_) => return,
        }
    }
}

/// The result of running the launcher to completion.
pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn launcher() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aok-launch"))
}

/// Run the launcher with `args` and collect its exit code and output.
pub fn launch(args: &[&str]) -> Out {
    let o = launcher().args(args).output().expect("spawn aok-launch");
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// Start the launcher without waiting; stdout and stderr go to pipes only if
/// asked, else null (a long-lived agent must not hold a test pipe open).
pub fn launch_bg(args: &[&str]) -> std::process::Child {
    launcher()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aok-launch")
}

/// Wait up to `secs` for `<leaf>/cgroup.procs` to list a pid.
pub fn wait_for_pid(leaf: &Path, secs: u64) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if let Ok(t) = fs::read_to_string(leaf.join("cgroup.procs"))
            && let Some(p) = t.lines().next().and_then(|l| l.trim().parse().ok())
        {
            return p;
        }
        sleep(Duration::from_millis(10));
    }
    panic!("no process appeared in {}", leaf.display());
}

pub fn read_stderr(child: &mut std::process::Child) -> String {
    let mut s = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut s);
    }
    s
}

/// `populated`/`frozen` from `<dir>/cgroup.events`.
pub fn event(dir: &Path, key: &str) -> i64 {
    let t = fs::read_to_string(dir.join("cgroup.events")).expect("read cgroup.events");
    t.lines()
        .find_map(|l| {
            let (k, v) = l.split_once(' ')?;
            (k == key).then(|| v.trim().parse().ok())?
        })
        .expect("key in cgroup.events")
}
