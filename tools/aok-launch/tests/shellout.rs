//! Shell-out inheritance, the ABI floor, and the launcher-grant falsifier
//! (LNCH-03, LNCH-04). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test shellout -- --test-threads=1`

mod common;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::process::Command;

use common::*;

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_aok-launch")
}

/// A fresh `/tmp` directory holding `A/f` and `B/f`.
fn two_dirs(tag: &str) -> (Scratch, String, String) {
    let s = Scratch::new(tag);
    let a = s.path().join("A");
    let b = s.path().join("B");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("f"), "content-of-a\n").unwrap();
    fs::write(b.join("f"), "content-of-b\n").unwrap();
    (
        s,
        a.to_str().unwrap().to_string(),
        b.to_str().unwrap().to_string(),
    )
}

#[test]
fn shellout_inherits() {
    let a = Agent::new("shell");
    let (_s, dir_a, dir_b) = two_dirs("shell");
    let grant = format!("rx:{dir_a}");
    // Three things from a grandchild shell: its cgroup, the inode of that
    // cgroup directory, and a read of the ungranted file. The identity half
    // uses /proc and stat only, not the 09-02 resolver. The process status
    // lines show seccomp and NoNewPrivs came across the exec too.
    let script = format!(
        "p=$(sh -c 'sed -n \"s/^0:://p\" /proc/self/cgroup'); \
         echo cg=$p; \
         echo ino=$(stat -c %i /sys/fs/cgroup$p); \
         grep -E '^(NoNewPrivs|Seccomp):' /proc/self/status; \
         cat {dir_b}/f; echo brc=$?"
    );
    let o = launch(&[
        "run", "--name", &a.name, "--grant", &grant, "--", "sh", "-c", &script,
    ]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);

    let leaf = a.leaf();
    let want_cg = cgroup_line(&leaf)[3..].to_string();
    assert!(
        o.stdout.lines().any(|l| l == format!("cg={want_cg}")),
        "grandchild is not in the agent's main leaf {want_cg}: {}",
        o.stdout
    );
    let want_ino = fs::metadata(&leaf).unwrap().ino();
    assert!(
        o.stdout.lines().any(|l| l == format!("ino={want_ino}")),
        "grandchild cgroup id is not the agent's ({want_ino}): {}",
        o.stdout
    );
    assert!(o.stdout.contains("NoNewPrivs:\t1"), "stdout: {}", o.stdout);
    assert!(
        o.stdout.contains("Seccomp:\t2"),
        "seccomp filter mode not inherited: {}",
        o.stdout
    );
    assert!(
        o.stderr.contains("Permission denied"),
        "the ungranted read was not EACCES: {}",
        o.stderr
    );
    assert!(!o.stdout.contains("content-of-b"), "leaked: {}", o.stdout);
    assert!(o.stdout.contains("brc=1"), "stdout: {}", o.stdout);
}

#[test]
fn floor_refusal() {
    let a = Agent::new("floor");
    let host_abi = aok_launch::sys::landlock_create_ruleset_version().unwrap_or(0);
    let note = format!(
        "SIMULATED (D-13): the shim makes landlock_create_ruleset return ENOSYS on a kernel that \
         has Landlock ABI {host_abi}; no real below-floor kernel was used"
    );
    println!("floor_refusal: {note}");

    let o = Command::new(env!("CARGO_BIN_EXE_aok-shim"))
        .args(["--fake", "landlock-enosys", "--", exe()])
        .args(["run", "--name", &a.name, "--", "true"])
        .output()
        .expect("spawn aok-shim");
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(125), "{note}; stderr: {stderr}");
    assert!(stderr.contains("EOPNOTSUPP"), "{note}; stderr: {stderr}");
    assert!(
        stderr.contains("missing rights") && stderr.contains("SIGNAL"),
        "{note}; stderr: {stderr}"
    );
    assert!(
        !a.dir.exists(),
        "{note}: the refusal must come before any cgroup write"
    );
}

#[test]
fn launch_cannot_widen() {
    let outer = Agent::new("widen");
    let (_s, dir_a, dir_b) = two_dirs("widen");
    let bin_dir = std::path::Path::new(exe())
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let (rx_a, rx_bin, rx_b) = (
        format!("rx:{dir_a}"),
        format!("rx:{bin_dir}"),
        format!("rx:{dir_b}"),
    );
    let script = format!("cat {dir_a}/f; echo arc=$?; cat {dir_b}/f; echo brc=$?");

    // The parent holds A and an explicit grant on the launcher's directory.
    // The sub launch asks for B, which the parent lacks. A grant that lets
    // the agent run the launcher must not let it name a wider authority.
    let o = launch(&[
        "run",
        "--name",
        &outer.name,
        "--grant",
        &rx_a,
        "--grant",
        &rx_bin,
        "--",
        exe(),
        "run",
        "--name",
        "inner",
        "--grant",
        &rx_b,
        "--",
        "sh",
        "-c",
        &script,
    ]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("role=sub"), "no sub launch: {}", o.stderr);
    assert!(
        !o.stdout.contains("content-of-b"),
        "the sub-agent reached B through a launcher grant: {}",
        o.stdout
    );
    assert!(o.stdout.contains("brc=1"), "stdout: {}", o.stdout);
    // The child layer only holds B, so it narrows A away as well.
    assert!(
        !o.stdout.contains("content-of-a"),
        "the sub-agent kept A, which its own layer did not grant: {}",
        o.stdout
    );
    assert!(
        o.stderr.contains("Permission denied"),
        "stderr: {}",
        o.stderr
    );
}
