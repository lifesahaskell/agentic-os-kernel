//! Identity tests (LNCH-02). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test identity -- --test-threads=1`

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;

use common::*;

/// Parse `key=value` lines from `whois`.
fn kv(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn whois(args: &[&str]) -> Out {
    let mut a = vec!["whois"];
    a.extend_from_slice(args);
    launch(&a)
}

/// Start a long-lived root agent and wait for its first process.
struct Live {
    child: std::process::Child,
    pid: i32,
}

fn start_root(a: &Agent, extra: &[&str]) -> Live {
    let mut args = vec!["run", "--name", &a.name];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["--", "sleep", "60"]);
    let child = launch_bg(&args);
    let pid = wait_for_pid(&a.leaf(), 5);
    Live { child, pid }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn identity_from_pidfd() {
    let a = Agent::new("ident");
    let live = start_root(&a, &["--on-behalf-of", "alice@example.org"]);

    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(m["agent_id"], a.name, "{}", o.stdout);
    assert_eq!(m["on_behalf_of"], "alice@example.org");
    assert_eq!(m["parent"], "none");
    assert_eq!(m["path"], "pidfd");
    // The key is the cgroup id the kernel stamped: the main leaf's inode.
    assert_eq!(
        m["cgroup_id"],
        fs::metadata(a.leaf()).unwrap().ino().to_string()
    );

    // Usage errors are 64 and name the problem.
    assert_eq!(whois(&[]).code, 64);
    assert_eq!(whois(&["--pid", "0"]).code, 64);
    assert_eq!(whois(&["--bogus", "1"]).code, 64);

    // A pid that is not in any registered agent is refused, not guessed.
    let o = whois(&["--pid", "1"]);
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
}

/// RESEARCH item 9, recorded as a note: moving a task into a sibling leaf of
/// the agent's own subtree re-attributes its agent_id. That is an ADV
/// candidate (an agent can re-label itself within its own subtree), not a
/// defect in resolution: the id follows the cgroup, which is the point.
#[test]
fn moving_a_task_within_the_subtree_reattributes_it() {
    let a = Agent::new("move");
    let live = start_root(&a, &[]);

    let sibling = a.dir.join("elsewhere");
    fs::create_dir(&sibling).expect("mkdir sibling leaf");
    fs::write(sibling.join("cgroup.procs"), live.pid.to_string()).expect("move task");

    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(
        m["agent_id"],
        format!("{}/elsewhere", a.name),
        "{}",
        o.stdout
    );
    assert_eq!(
        m["cgroup_id"],
        fs::metadata(&sibling).unwrap().ino().to_string()
    );

    // Put it back so teardown is plain.
    fs::write(a.leaf().join("cgroup.procs"), live.pid.to_string()).expect("move back");
    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(kv(&o.stdout)["agent_id"], a.name);
}

/// RESEARCH item 8: renaming a cgroup2 directory is EPERM, so an agent_id,
/// which is a path, cannot change by rename. This confirms D-06.
#[test]
fn cgroup_directory_rename_is_eperm() {
    let a = Agent::new("rename");
    let _live = start_root(&a, &[]);
    let to = a.dir.with_file_name(format!("{}-renamed", a.name));
    let e = fs::rename(&a.dir, &to).expect_err("cgroup2 rename must fail");
    assert_eq!(e.raw_os_error(), Some(libc::EPERM), "{e}");
    assert!(a.dir.exists() && !to.exists());
}
