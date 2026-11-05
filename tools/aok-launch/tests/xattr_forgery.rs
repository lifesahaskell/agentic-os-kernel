//! D-10 negative control. Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test xattr_forgery -- --test-threads=1`
//!
//! Identity is the kernel-stamped cgroup id plus the launcher-written
//! registry, never a `user.*` xattr on the agent's cgroup directory. This test
//! shows why. The Landlock ABI has no xattr right, so a same-uid process that
//! has applied a ruleset handling every filesystem right can still overwrite,
//! create and remove `user.*` xattrs on its own cgroup directory. The same
//! process is denied (EACCES) the operations that do matter: creating a
//! sibling cgroup directory and writing `cgroup.procs`.
//!
//! This is a tripwire as much as a proof: if a future kernel adds an xattr
//! hook and the forgery starts failing, the "forgeable" assertions below flip
//! and say so. Identity-not-from-xattr still holds either way, because the
//! resolver never reads one.
//!
//! The probe is a `python3 -I -B` script in a fresh directory under the system
//! temp dir. It applies its own Landlock ruleset (no 09-03 code involved), so
//! the test depends on nothing outside this crate.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;

/// `setup`: set `user.aok.agent_id=orig` on the directory.
/// `plain`: run the operations with no Landlock (the DAC control).
/// `ll`: apply a Landlock ruleset handling the ABI-1 filesystem rights
/// (0x1FFF) with no rules, then run the same operations.
///
/// Output is one `label=ok|ERRNAME` line per operation.
const PROBE: &str = r#"
import ctypes, errno, os, struct, sys

mode, d = sys.argv[1], sys.argv[2]
XA = "user.aok.agent_id"
XB = "user.aok.extra"

def report(label, fn):
    try:
        fn()
        print("%s=ok" % label)
    except OSError as e:
        print("%s=%s" % (label, errno.errorcode.get(e.errno, str(e.errno))))

if mode == "setup":
    report("setxattr_initial", lambda: os.setxattr(d, XA, b"orig"))
    sys.exit(0)

if mode == "ll":
    libc = ctypes.CDLL(None, use_errno=True)
    def sc(n, *a):
        r = libc.syscall(n, *a)
        if r < 0:
            e = ctypes.get_errno()
            print("landlock_setup=%s" % errno.errorcode.get(e, str(e)))
            sys.exit(70)
        return r
    attr = struct.pack("Q", 0x1FFF)
    rfd = sc(444, attr, len(attr), 0)
    sc(157, 38, 1, 0, 0, 0)
    sc(446, rfd, 0)
    print("landlock_setup=ok")

report("setxattr_overwrite", lambda: os.setxattr(d, XA, b"forged"))
report("setxattr_new", lambda: os.setxattr(d, XB, b"x"))
report("removexattr_agent_id", lambda: os.removexattr(d, XA))
report("removexattr_extra", lambda: os.removexattr(d, XB))
report("final_read", lambda: os.getxattr(d, XA))
sib = d + "-sib"
report("mkdir_sibling", lambda: os.mkdir(sib))
report("mkdir_child", lambda: os.mkdir(d + "/child"))
report("open_procs_w", lambda: os.close(os.open(d + "/cgroup.procs", os.O_WRONLY)))
for p in (sib, d + "/child"):
    try:
        os.rmdir(p)
    except OSError:
        pass
"#;

fn parse(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A fresh, empty directory for the probe script, removed on drop.
struct ScriptDir(PathBuf);

impl ScriptDir {
    fn new() -> ScriptDir {
        let dir = std::env::temp_dir().join(format!("aok-xattr-probe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("create probe dir");
        ScriptDir(dir)
    }
}

impl Drop for ScriptDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_probe(script: &Path, mode: &str, dir: &Path) -> BTreeMap<String, String> {
    let o = Command::new("python3")
        .args(["-I", "-B"])
        .arg(script)
        .arg(mode)
        .arg(dir)
        .output()
        .expect("spawn python3");
    let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(
        o.status.success(),
        "probe mode {mode} failed ({:?}): stdout: {stdout} stderr: {stderr}",
        o.status.code()
    );
    parse(&stdout)
}

#[test]
fn xattr_forgery() {
    let agent = Agent::new("xattr");
    let dir = agent.dir.clone();
    assert!(
        !own_cgroup().starts_with(&dir),
        "the probe leaf must not hold the test process"
    );
    fs::create_dir(&dir).expect("create probe leaf");

    let scripts = ScriptDir::new();
    let script = scripts.0.join("probe.py");
    fs::write(&script, PROBE).expect("write probe");

    // Control: with no Landlock the DAC boundary lets the same process do
    // everything, so a denial under Landlock below is Landlock's.
    let setup = run_probe(&script, "setup", &dir);
    let initial = setup["setxattr_initial"].clone();
    let plain = run_probe(&script, "plain", &dir);
    assert_eq!(plain["mkdir_sibling"], "ok", "control: {plain:?}");
    assert_eq!(plain["mkdir_child"], "ok", "control: {plain:?}");
    assert_eq!(plain["open_procs_w"], "ok", "control: {plain:?}");

    // Restore the starting xattr, then run the Landlocked probe.
    let _ = run_probe(&script, "setup", &dir);
    let ll = run_probe(&script, "ll", &dir);
    assert_eq!(ll["landlock_setup"], "ok", "Landlock unavailable: {ll:?}");

    // The boundary that matters holds whether or not xattrs are writable:
    // Landlock denies a sibling cgroup directory, a child cgroup directory
    // and a cgroup.procs write with EACCES.
    for op in ["mkdir_sibling", "mkdir_child", "open_procs_w"] {
        assert_eq!(
            ll[op], "EACCES",
            "{op} must be denied under Landlock: {ll:?}"
        );
    }
    assert!(!PathBuf::from(format!("{}-sib", dir.display())).exists());

    if initial != "ok" {
        eprintln!(
            "xattr store not writable here ({initial}) - forgeability not demonstrable; \
             identity-not-from-xattr still holds a priori"
        );
        assert!(
            ["EOPNOTSUPP", "ENOTSUP", "EPERM", "EACCES"].contains(&initial.as_str()),
            "unexpected errno setting the initial xattr: {initial}"
        );
        return;
    }

    // Forgery: under the same ruleset the xattr store is wide open. This is
    // WHY identity is the kernel cgroup id plus the launcher registry and
    // not a user.* xattr (D-10): a Landlocked same-uid agent can rewrite it.
    for op in [
        "setxattr_overwrite",
        "setxattr_new",
        "removexattr_agent_id",
        "removexattr_extra",
    ] {
        assert_eq!(
            ll[op], "ok",
            "{op}: Landlock has no xattr hook, so user.* is forgeable (D-10). \
             If this flipped, a kernel now denies the forgery; identity still \
             never reads an xattr: {ll:?}"
        );
    }
    assert_eq!(ll["final_read"], "ENODATA", "{ll:?}");
}
