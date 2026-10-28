//! Integration tests for the 08-02 follow-up (A/B/C): a harness failure is a
//! hard error with no row written, the scm_rights route is wired to
//! agent-b's cgroup dir in agent mode, and agent-mode S1 emits no case rows.
//! These drive the real `aok-adv` binary end to end because the guarantees
//! live in main.rs's process orchestration, not in routes.rs's pure
//! attempt functions.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_aok-adv")
}

fn fresh_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aok-adv-hf-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_row(args: &[&str]) -> (Output, Duration) {
    let started = Instant::now();
    let out = Command::new(bin())
        .arg("run")
        .args(args)
        .output()
        .expect("spawn aok-adv run");
    (out, started.elapsed())
}

/// A: an over-long socket path used to let the stub try (and fail) to bind,
/// producing a void row with `errno: null` instead of a harness error. The
/// AF_UNIX `sun_path` limit is 108 bytes including the NUL.
#[test]
fn over_long_socket_path_exits_nonzero_and_writes_no_row() {
    let dir = fresh_dir("long-socket");
    let out_file = dir.join("o.jsonl");
    let socket = format!("{}/{}/s.sock", dir.display(), "x".repeat(110));
    assert!(socket.len() > 108, "fixture path must exceed the limit");

    let (out, _) = run_row(&[
        "--socket",
        &socket,
        "--cgroup-root",
        dir.join("c").to_str().unwrap(),
        "--routes",
        "kill",
        "--topology",
        "sibling",
        "--mode",
        "nonagent",
        "--no-cgroup",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "must exit non-zero; stderr: {stderr}"
    );
    assert!(
        stderr.contains("bytes"),
        "stderr must name the cause: {stderr}"
    );
    assert!(
        !out_file.exists(),
        "no row may be written on a harness failure"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A: a stub that exits immediately (its socket directory does not exist)
/// used to burn the whole wait and still land as a void "baseline failed"
/// row; now it must be reported at once as a harness failure.
#[test]
fn stub_that_exits_immediately_is_a_hard_error_not_void() {
    let dir = fresh_dir("dead-stub");
    let out_file = dir.join("o.jsonl");

    let (out, took) = run_row(&[
        "--socket",
        "/nonexistent-aok-dir/s.sock",
        "--cgroup-root",
        dir.join("c").to_str().unwrap(),
        "--routes",
        "kill",
        "--topology",
        "sibling",
        "--mode",
        "nonagent",
        "--no-cgroup",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "must exit non-zero; stderr: {stderr}"
    );
    assert!(
        stderr.contains("stub exited"),
        "stderr must name the cause: {stderr}"
    );
    assert!(
        took < Duration::from_secs(4),
        "must fail fast, took {took:?}"
    );
    assert!(
        !out_file.exists(),
        "no row may be written on a harness failure"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn spawn_stub_at(socket: &std::path::Path) -> std::process::Child {
    let stub = Command::new(bin())
        .arg("stub")
        .arg("--socket")
        .arg(socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn stub");
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::os::unix::net::UnixStream::connect(socket).is_err() {
        assert!(Instant::now() < deadline, "stub never became ready");
        std::thread::sleep(Duration::from_millis(20));
    }
    stub
}

fn attempt_scm_rights(
    stub_pid: u32,
    socket: &std::path::Path,
    cgroup_root: &std::path::Path,
    mode: &str,
) -> String {
    let mut attacker = Command::new(bin())
        .arg("attempt")
        .arg("--route")
        .arg("scm_rights")
        .arg("--topology")
        .arg("sibling")
        .arg("--pid")
        .arg(stub_pid.to_string())
        .arg("--socket")
        .arg(socket)
        .arg("--cgroup-root")
        .arg(cgroup_root)
        .arg("--mode")
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn attempt");
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
        .expect("read stdout");
    let mut err_line = String::new();
    let _ =
        BufReader::new(attacker.stderr.take().expect("attacker stderr")).read_line(&mut err_line);
    let _ = attacker.wait();
    stdout.trim().to_string()
}

/// B: in agent mode the scm_rights attempt is handed `<cgroup-root>/agent-b`
/// as the helper's placement target, so a tempdir stand-in's placement
/// guard refusal surfaces as a harness failure naming the helper placement.
/// The pre-fix dispatch passed `None` unconditionally and printed REACHABLE
/// with no placement attempted at all.
#[test]
fn scm_rights_in_agent_mode_places_the_helper_in_agent_b() {
    let dir = fresh_dir("scm-agent");
    let socket = dir.join("s.sock");
    let cgroup_root = dir.join("cg");
    std::fs::create_dir_all(cgroup_root.join("agent-b")).unwrap();
    let mut stub = spawn_stub_at(&socket);

    let verdict = attempt_scm_rights(stub.id(), &socket, &cgroup_root, "agent");
    assert!(
        verdict.starts_with("HARNESS") && verdict.contains("placing helper"),
        "agent mode must try to place the helper in agent-b; verdict: {verdict:?}"
    );

    let _ = stub.kill();
    let _ = stub.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// B: nonagent keeps the pre-existing behaviour unchanged: no placement is
/// attempted, and a real borrowed connected fd is obtained and used.
#[test]
fn scm_rights_in_nonagent_mode_is_unchanged() {
    let dir = fresh_dir("scm-nonagent");
    let socket = dir.join("s.sock");
    let mut stub = spawn_stub_at(&socket);

    let verdict = attempt_scm_rights(stub.id(), &socket, &dir.join("cg"), "nonagent");
    assert_eq!(verdict, "REACHABLE", "nonagent scm_rights must still work");

    let _ = stub.kill();
    let _ = stub.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// C: agent-mode S1 emits no case rows and names why on stderr; this is not
/// a failure (exit 0), and `agent_result` stays untested rather than being
/// fabricated.
#[test]
fn agent_mode_s1_emits_no_case_rows_and_names_the_reason() {
    let dir = fresh_dir("agent-s1");
    let out_file = dir.join("o.jsonl");

    let (out, _) = run_row(&["--mode", "agent", "--out", out_file.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "must exit 0; stderr: {stderr}");
    assert!(
        stderr.contains("ADV-04") && stderr.contains("Phases 11-13"),
        "stderr must explain the skip: {stderr}"
    );
    let body = std::fs::read_to_string(&out_file).unwrap_or_default();
    assert_eq!(
        body.lines().count(),
        0,
        "agent mode must emit zero case rows: {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// C: nonagent with no `--routes` still runs and emits all 8 S1 case rows,
/// unchanged by the agent-mode skip.
#[test]
fn nonagent_s1_still_emits_the_eight_case_rows() {
    let dir = fresh_dir("nonagent-s1");
    let out_file = dir.join("o.jsonl");

    let (out, _) = run_row(&["--mode", "nonagent", "--out", out_file.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "must exit 0; stderr: {stderr}");
    let body = std::fs::read_to_string(&out_file).expect("cases output file must exist");
    assert_eq!(
        body.lines().filter(|l| !l.trim().is_empty()).count(),
        8,
        "expected 8 S1 case rows: {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A: placement failing in agent mode is a harness failure. A tempdir cgroup
/// root sits outside `assert_cgroup_kill_path`'s required prefix, so
/// placement is refused without touching any real cgroup.
#[test]
fn agent_mode_placement_failure_is_a_hard_error() {
    let dir = fresh_dir("placement");
    let out_file = dir.join("o.jsonl");
    let socket = dir.join("s.sock");

    let (out, _) = run_row(&[
        "--socket",
        socket.to_str().unwrap(),
        "--cgroup-root",
        dir.join("c").to_str().unwrap(),
        "--routes",
        "kill",
        "--topology",
        "sibling",
        "--mode",
        "agent",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "must exit non-zero; stderr: {stderr}"
    );
    assert!(
        stderr.contains("harness failure"),
        "stderr must name a harness failure: {stderr}"
    );
    assert!(
        !out_file.exists(),
        "no row may be written on a harness failure"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
