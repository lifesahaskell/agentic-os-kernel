//! D-08 registry confinement (LNCH-02, LNCH-03, T-09-15). Run inside a
//! delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test registry_confinement -- --test-threads=1`
//!
//! This test never writes, truncates or removes the registry or any line in
//! it. Its own root line is left for the next root launch's dead-root prune.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use aok_launch::identity::{registry_dir, registry_path};
use common::*;

/// Prints one `key=value` line per check and exits 0 whatever it observes.
/// Every fd it opens is closed. It never writes a byte to the registry.
const PROBE: &str = r#"
import errno, os, sys
reg, d = sys.argv[1], sys.argv[2]

def attempt(path, flags, mode=0o600):
    try:
        fd = os.open(path, flags, mode)
    except OSError as e:
        return errno.errorcode.get(e.errno, str(e.errno))
    os.close(fd)
    return "ok"

print("read=" + attempt(reg, os.O_RDONLY))
print("append=" + attempt(reg, os.O_WRONLY | os.O_APPEND))
p = os.path.join(d, "probe.%d" % os.getpid())
r = attempt(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
if r == "ok":
    os.unlink(p)
print("create=" + r)
print("procroot=" + attempt("/proc/self/root" + reg, os.O_RDONLY))

hit = "none"
for name in os.listdir("/proc/self/fd"):
    try:
        link = os.readlink("/proc/self/fd/" + name)
    except OSError:
        continue
    if link == reg:
        hit = link
print("fd=" + hit)

cg = [l for l in open("/proc/self/cgroup").read().splitlines() if l.startswith("0::")][0][3:]
leaf = "/sys/fs/cgroup" + cg
print("agent=%d" % os.stat(os.path.dirname(leaf)).st_ino)
"#;

fn parse(out: &str) -> BTreeMap<String, String> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn registry_unreachable_from_agent() {
    let a = Agent::new("regconf");
    let reg = registry_path().expect("registry path");
    let dir = registry_dir();
    let (reg_s, dir_s) = (reg.to_str().unwrap(), dir.to_str().unwrap());
    let probe_argv = ["python3", "-I", "-B", "-c", PROBE, reg_s, dir_s];

    // The agent: a root launch with no --grant, so it holds exactly the
    // default grant and the D-12 implicit grants. register_root writes this
    // launch's line before the spawn.
    let mut args = vec!["run", "--name", a.name.as_str(), "--"];
    args.extend_from_slice(&probe_argv);
    let o = launch(&args);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let inside = parse(&o.stdout);
    for check in ["read", "append", "create", "procroot"] {
        assert_eq!(
            inside.get(check).map(String::as_str),
            Some("EACCES"),
            "inside the agent {check} must be EACCES (not ENOENT, not ok): {}",
            o.stdout
        );
    }
    assert_eq!(
        inside.get("fd").map(String::as_str),
        Some("none"),
        "the agent inherited a registry fd: {}",
        o.stdout
    );

    // The control: the same probe outside any agent, same uid, a 0600 file in
    // a 0700 directory, so the denials above come from Landlock and not DAC.
    let c = Command::new(probe_argv[0])
        .args(&probe_argv[1..])
        .output()
        .expect("run control probe");
    assert!(c.status.success(), "control failed: {:?}", c.status);
    let control = parse(&String::from_utf8_lossy(&c.stdout));
    for check in ["read", "append", "create", "procroot"] {
        assert_eq!(
            control.get(check).map(String::as_str),
            Some("ok"),
            "control {check} must succeed outside an agent: {control:?}"
        );
    }

    // The registry still holds this launch's own root line.
    let agent = inside.get("agent").expect("agent id line");
    let text = fs::read_to_string(&reg).expect("read registry");
    assert!(
        text.lines().any(|l| l.split('\t').next() == Some(agent)),
        "registry lost this launch's line for cgroup {agent}:\n{text}"
    );

    // No probe file was left behind.
    let leftover: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("probe."))
        .collect();
    assert!(leftover.is_empty(), "probe files left: {leftover:?}");
}
