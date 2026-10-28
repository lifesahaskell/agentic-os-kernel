//! Integration tests for acceptance (b) and (c) (08-02 gap closure, D2/D3):
//! these exercise the real `aok-adv` binary end to end, not routes.rs's
//! injected-closure unit tests, because the topology and stub-lifecycle
//! guarantees live in main.rs's process orchestration, not in routes.rs's
//! pure attempt functions.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_aok-adv")
}

fn fresh_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aok-adv-it-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Field 4 (ppid) of `/proc/<pid>/stat`, parsed the same way routes.rs's own
/// `count_children_of` test helper does (comm may contain spaces/parens).
fn ppid_of(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    fields.get(1)?.parse::<i32>().ok()
}

/// Acceptance (b), sibling half: the runner spawns the stub and the
/// attacker as its own two children, so both report the test process (the
/// stand-in runner here) as their parent. Fails on pre-fix code, which had
/// no `attempt` subcommand and no stub-spawned-by-runner topology at all.
#[test]
fn sibling_topology_shares_the_runner_as_parent_for_stub_and_attacker() {
    let dir = fresh_dir("sibling-topology");
    let socket = dir.join("stub.sock");
    let cgroup_root = dir.join("cgroup-root"); // unused (--no-cgroup route), never created

    let mut stub = Command::new(bin())
        .arg("stub")
        .arg("--socket")
        .arg(&socket)
        .spawn()
        .expect("spawn stub");
    let stub_pid = stub.id() as i32;

    // Wait for the stub to actually be listening before attacking it.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::os::unix::net::UnixStream::connect(&socket).is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "stub never became ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut attacker = Command::new(bin())
        .arg("attempt")
        .arg("--route")
        .arg("PTRACE_ATTACH")
        .arg("--topology")
        .arg("sibling")
        .arg("--pid")
        .arg(stub_pid.to_string())
        .arg("--socket")
        .arg(&socket)
        .arg("--cgroup-root")
        .arg(&cgroup_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn attempt");
    let attacker_pid = attacker.id() as i32;
    let test_pid = std::process::id() as i32;

    // Confirm the parentage before sending "go", while both children are
    // guaranteed alive (the attacker is blocked on the handshake read).
    assert_eq!(
        ppid_of(stub_pid),
        Some(test_pid),
        "sibling topology: stub's parent must be the runner (this test), not the attacker"
    );
    assert_eq!(
        ppid_of(attacker_pid),
        Some(test_pid),
        "sibling topology: attacker's parent must be the runner (this test)"
    );

    attacker
        .stdin
        .take()
        .expect("attacker stdin")
        .write_all(b"go\n")
        .expect("send go");
    let mut stdout = String::new();
    attacker
        .stdout
        .take()
        .expect("attacker stdout")
        .read_to_string(&mut stdout)
        .expect("read attacker stdout");
    let _ = attacker.wait();
    assert!(
        stdout.trim().starts_with("REACHABLE") || stdout.trim().starts_with("VOID"),
        "unexpected attempt verdict: {stdout:?}"
    );

    let _ = stub.kill();
    let _ = stub.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Acceptance (b), ancestor half: the attacker spawns the stub as its own
/// child, so the stub's ppid is the attacker's pid, never the runner's.
/// Fails on pre-fix code, which had no ancestor-topology stub spawn at all
/// (D2).
#[test]
fn ancestor_topology_makes_the_attacker_the_stubs_parent() {
    let dir = fresh_dir("ancestor-topology");
    let socket = dir.join("stub.sock");
    let cgroup_root = dir.join("cgroup-root"); // unused (--no-cgroup route), never created

    let mut attacker = Command::new(bin())
        .arg("attempt")
        .arg("--route")
        .arg("PTRACE_ATTACH")
        .arg("--topology")
        .arg("ancestor")
        .arg("--socket")
        .arg(&socket)
        .arg("--cgroup-root")
        .arg(&cgroup_root)
        .arg("--mode")
        .arg("nonagent")
        .arg("--no-cgroup")
        .env("AOK_ADV_TEST_PAUSE_MS", "400")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn attempt");
    let attacker_pid = attacker.id() as i32;

    attacker
        .stdin
        .take()
        .expect("attacker stdin")
        .write_all(b"go\n")
        .expect("send go");

    let mut stderr_reader = BufReader::new(attacker.stderr.take().expect("attacker stderr"));
    let mut diag_line = String::new();
    stderr_reader
        .read_line(&mut diag_line)
        .expect("read stub_pid diagnostic line");
    let stub_pid: i32 = diag_line
        .trim()
        .strip_prefix("stub_pid=")
        .and_then(|rest| rest.split_whitespace().next())
        .expect("diagnostic line carries stub_pid=<n>")
        .parse()
        .expect("stub_pid is an integer");

    // The attempt process is paused (AOK_ADV_TEST_PAUSE_MS) before it
    // dispatches the route, so the stub it just spawned is still alive and
    // still parented to it right now.
    assert_eq!(
        ppid_of(stub_pid),
        Some(attacker_pid),
        "ancestor topology: stub's parent must be the attacker, not the runner"
    );

    let mut stdout = String::new();
    attacker
        .stdout
        .take()
        .expect("attacker stdout")
        .read_to_string(&mut stdout)
        .expect("read attacker stdout");
    let _ = attacker.wait();
    assert!(
        stdout.trim().starts_with("REACHABLE") || stdout.trim().starts_with("VOID"),
        "unexpected attempt verdict: {stdout:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Acceptance (c): the stub pid differs between two consecutive rows of the
/// same `run` invocation (D3's "fresh stub per row" policy). Fails on
/// pre-fix code, which never started a stub from `run` at all.
#[test]
fn consecutive_rows_get_a_fresh_stub_pid() {
    let dir = fresh_dir("fresh-stub-per-row");
    let socket = dir.join("stub.sock");
    let cgroup_root = dir.join("cgroup-root"); // unused (--no-cgroup routes), never created
    let out = dir.join("out.jsonl");

    let child = Command::new(bin())
        .arg("run")
        .arg("--socket")
        .arg(&socket)
        .arg("--cgroup-root")
        .arg(&cgroup_root)
        .arg("--routes")
        .arg("PTRACE_ATTACH,PTRACE_SEIZE")
        .arg("--topology")
        .arg("sibling")
        .arg("--mode")
        .arg("nonagent")
        .arg("--no-cgroup")
        .arg("--out")
        .arg(&out)
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run");

    let output = child.wait_with_output().expect("wait for run");
    let stderr = String::from_utf8_lossy(&output.stderr);

    let stub_pids: Vec<i32> = stderr
        .lines()
        .filter_map(|line| {
            line.split_whitespace()
                .find_map(|tok| tok.strip_prefix("stub_pid="))
                .and_then(|v| v.parse::<i32>().ok())
        })
        .collect();

    assert_eq!(
        stub_pids.len(),
        2,
        "expected one stub_pid diagnostic per row; stderr was: {stderr}"
    );
    assert_ne!(
        stub_pids[0], stub_pids[1],
        "consecutive rows must spawn a fresh stub, not reuse one"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
