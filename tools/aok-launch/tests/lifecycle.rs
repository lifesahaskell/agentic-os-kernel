//! Lifecycle tests (LNCH-05): freeze, thaw, kill and exit observation. Run
//! inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test lifecycle -- --test-threads=1`
//!
//! No test here kills a cgroup that contains the test process: every target
//! is a throwaway agent the fixture created under the delegated root.

mod common;

use std::fs;
use std::os::fd::AsRawFd;
use std::thread::sleep;
use std::time::Duration;

use aok_launch::sys;
use common::*;

/// utime + stime of `pid` in clock ticks, from `/proc/<pid>/stat`. The comm
/// field may contain spaces and parentheses, so split after the last `)`.
fn cpu_ticks(pid: i32) -> u64 {
    let s = fs::read_to_string(format!("/proc/{pid}/stat")).expect("read stat");
    let rest = &s[s.rfind(')').expect("comm end") + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // After comm: state is f[0]; utime and stime are fields 14 and 15 of the
    // full line, which are f[11] and f[12] here.
    f[11].parse::<u64>().unwrap() + f[12].parse::<u64>().unwrap()
}

#[test]
fn freeze_thaw_kill() {
    let a = Agent::new("life");
    let mut l = launch_bg(&[
        "run",
        "--name",
        &a.name,
        "--",
        "sh",
        "-c",
        "while :; do :; done",
    ]);
    let pid = wait_for_pid(&a.leaf(), 5);
    let pidfd = sys::owned_fd(sys::pidfd_open(pid, 0).expect("pidfd_open"));

    // Alive: the pidfd is not readable and the task burns CPU.
    assert!(!sys::poll_readable(pidfd.as_raw_fd(), Duration::ZERO).unwrap());
    let c0 = cpu_ticks(pid);
    sleep(Duration::from_millis(400));
    let c1 = cpu_ticks(pid);
    assert!(c1 > c0, "busy task made no progress: {c0} -> {c1}");

    // Freeze the non-leaf agent directory: the whole subtree reads frozen 1
    // and the task stops advancing.
    let o = launch(&["freeze", &a.name]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert_eq!(event(&a.dir, "frozen"), 1);
    assert_eq!(event(&a.leaf(), "frozen"), 1);
    let f0 = cpu_ticks(pid);
    sleep(Duration::from_millis(400));
    assert_eq!(cpu_ticks(pid), f0, "frozen task advanced");

    // Thaw: both read frozen 0 and the task runs again.
    let o = launch(&["thaw", &a.name]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert_eq!(event(&a.dir, "frozen"), 0);
    assert_eq!(event(&a.leaf(), "frozen"), 0);
    sleep(Duration::from_millis(400));
    assert!(cpu_ticks(pid) > f0, "thawed task did not advance");

    // Kill: populated 0 observed, directories gone, the leader's pidfd
    // becomes readable, and the launcher reports the leader's SIGKILL.
    let o = launch(&["kill", &a.name]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(o.stdout.contains("populated=0"), "stdout: {}", o.stdout);
    assert!(!a.dir.exists(), "agent directory survived kill");
    assert!(sys::poll_readable(pidfd.as_raw_fd(), Duration::from_secs(5)).unwrap());
    let status = l.wait().unwrap();
    assert_eq!(status.code(), Some(137), "stderr: {}", read_stderr(&mut l));
}

#[test]
fn zero_member_kill() {
    let a = Agent::new("zero");
    // The leader exits at once and leaves an agent with no live task.
    let o = launch(&["run", "--name", &a.name, "--", "true"]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert_eq!(event(&a.dir, "populated"), 0);
    assert_eq!(event(&a.leaf(), "populated"), 0);

    let o = launch(&["kill", &a.name]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(o.stdout.contains("populated=0"), "stdout: {}", o.stdout);
    assert!(!a.dir.exists());
}

#[test]
fn lifecycle_usage_and_missing_agent() {
    let a = Agent::new("none");
    assert_eq!(launch(&["kill"]).code, 64);
    assert_eq!(launch(&["freeze", "a", "b"]).code, 64);
    assert_eq!(launch(&["thaw", "../x"]).code, 64);
    let o = launch(&["kill", &a.name]);
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("ENOENT"), "stderr: {}", o.stderr);
}
