//! Spawn-path tests (LNCH-01). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test spawn -- --test-threads=1`

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use common::*;

fn child_dirs(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn spawn_into_cgroup() {
    let a = Agent::new("spawn");

    // The child's first action reads its own cgroup: it must be <name>/main.
    let o = launch(&[
        "run",
        "--name",
        &a.name,
        "--",
        "sh",
        "-c",
        "cat /proc/self/cgroup",
    ]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert_eq!(o.stdout.trim(), cgroup_line(&a.leaf()));
    assert!(o.stderr.contains("role=root"), "stderr: {}", o.stderr);
    assert!(
        o.stderr.contains("spawn_path=clone3"),
        "stderr: {}",
        o.stderr
    );

    // The kernel-stamped cgroup id of a live agent process equals the
    // main-leaf directory inode (T-09-01).
    let mut l = launch_bg(&["run", "--name", &a.name, "--", "sleep", "30"]);
    let pid = wait_for_pid(&a.leaf(), 5);
    let fd = aok_launch::sys::pidfd_open(pid, 0).expect("pidfd_open");
    let id = aok_launch::sys::pidfd_identity(fd).expect("PIDFD_GET_INFO");
    aok_launch::sys::close_fd(fd);
    assert_eq!(id.pid, pid);
    assert_eq!(id.cgroupid, fs::metadata(a.leaf()).unwrap().ino());

    let k = launch(&["kill", &a.name]);
    assert_eq!(k.code, 0, "stderr: {}", k.stderr);
    let status = l.wait().unwrap();
    assert_eq!(status.code(), Some(137), "stderr: {}", read_stderr(&mut l));
}

#[test]
fn sub_is_descendant() {
    let prime = Agent::new("prime");
    let a = Agent::new("nest");

    // Prime: the first root launch enables the root's controllers and builds
    // aok-mgr, so the snapshot below isolates the sub launch.
    let p = launch(&["run", "--name", &prime.name, "--", "true"]);
    assert_eq!(p.code, 0, "stderr: {}", p.stderr);
    let root = scope_root();
    let ctl_before = fs::read(root.join("cgroup.subtree_control")).unwrap();
    let dirs_before = child_dirs(&root);

    let o = launch(&[
        "nest",
        "--name",
        &a.name,
        "--depth",
        "2",
        "--",
        "sh",
        "-c",
        "cat /proc/self/cgroup",
    ]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);

    // A sub launch misread as a root launch would print <root>/N/main.
    let want = cgroup_line(&a.dir.join("d2").join("main"));
    assert_eq!(o.stdout.trim(), want);
    assert!(o.stderr.contains("role=root"), "stderr: {}", o.stderr);
    assert!(o.stderr.contains("role=sub"), "stderr: {}", o.stderr);

    // The sub launch never touched the delegated root.
    assert_eq!(
        fs::read(root.join("cgroup.subtree_control")).unwrap(),
        ctl_before
    );
    let mut expect = dirs_before;
    expect.insert(a.name.clone());
    assert_eq!(child_dirs(&root), expect);
}

#[test]
fn exit_contract() {
    let a = Agent::new("exit");
    let o = launch(&["run", "--name", &a.name, "--", "sh", "-c", "exit 3"]);
    assert_eq!(o.code, 3, "stderr: {}", o.stderr);

    let o = launch(&["run", "--name", &a.name, "--", "sh", "-c", "kill -9 $$"]);
    assert_eq!(o.code, 137, "stderr: {}", o.stderr);

    assert_eq!(launch(&["run", "--", "true"]).code, 64, "missing --name");
    assert_eq!(launch(&["run", "--name", &a.name]).code, 64, "missing --");
    assert_eq!(launch(&[]).code, 64, "no subcommand");
    assert_eq!(launch(&["bogus"]).code, 64, "unknown subcommand");
    let o = launch(&["nest", "--name", &a.name, "--depth", "0", "--", "true"]);
    assert_eq!(o.code, 64, "stderr: {}", o.stderr);

    // A failed exec arrives over the pre-exec pipe: 125, ENOENT named.
    let o = launch(&["run", "--name", &a.name, "--", "/nonexistent/aok-cmd"]);
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("exec: ENOENT"), "stderr: {}", o.stderr);
}

#[test]
fn name_guard() {
    let long = "a".repeat(129);
    for (name, errno) in [
        ("", "EINVAL"),
        ("a/b", "EINVAL"),
        ("..", "EINVAL"),
        ("main", "EINVAL"),
        (long.as_str(), "ENAMETOOLONG"),
    ] {
        let before = child_dirs(&scope_root());
        let o = launch(&["run", "--name", name, "--", "true"]);
        assert_eq!(o.code, 64, "name {name:?}: {}", o.stderr);
        assert!(o.stderr.contains(errno), "name {name:?}: {}", o.stderr);
        // Rejected before any mkdir.
        assert_eq!(child_dirs(&scope_root()), before, "name {name:?}");
    }

    // A duplicate name is tolerated, not an error.
    let a = Agent::new("dup");
    for _ in 0..2 {
        let o = launch(&["run", "--name", &a.name, "--", "true"]);
        assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    }
}
