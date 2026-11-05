//! The two degraded paths, forced by the D-13 shim on a host that has both
//! features (LNCH-01, LNCH-02). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test fallback -- --test-threads=1`
//!
//! Every result here is SIMULATED. The shim's seccomp filter makes a real
//! syscall return a real errno on a kernel that HAS the feature, so the
//! launcher takes its degraded branch for real, but the numbers printed are
//! not a measurement of a native old kernel. They are indicative of what the
//! fallback costs, and no threshold is asserted on them.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use common::*;

fn launcher_exe() -> &'static str {
    env!("CARGO_BIN_EXE_aok-launch")
}

/// Run the launcher under the shim with the given fakes.
fn shimmed(fakes: &[&str], launcher_args: &[&str]) -> Out {
    let mut c = Command::new(env!("CARGO_BIN_EXE_aok-shim"));
    for f in fakes {
        c.arg("--fake").arg(f);
    }
    c.arg("--").arg(launcher_exe()).args(launcher_args);
    let o = c.output().expect("spawn aok-shim");
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// `key=value` lines, as `whois` prints them.
fn kv(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The value after `key=` on the launcher's `spawn_path=` line.
fn report_field<'a>(stderr: &'a str, key: &str) -> Option<&'a str> {
    let line = stderr.lines().find(|l| l.contains("spawn_path="))?;
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(key)?.strip_prefix('='))
}

#[test]
fn spawn_fallback_window() {
    let a = Agent::new("spawnwin");
    let note = "SIMULATED (D-13): clone3 is present on this host; the shim made it return \
                ENOSYS, so the fork+move+exec fallback ran on a kernel that has clone3. \
                Not a native-degraded measurement";

    let o = shimmed(
        &["clone3-enosys"],
        &[
            "run",
            "--name",
            &a.name,
            "--",
            "sh",
            "-c",
            "cat /proc/self/cgroup",
        ],
    );
    assert_eq!(o.code, 0, "{note}; stderr: {}", o.stderr);

    // The fallback is correct: the child that ran `cat` was already in the
    // agent's main leaf, so the move happened before the exec.
    assert_eq!(o.stdout.trim(), cgroup_line(&a.leaf()), "{note}");
    assert_eq!(
        report_field(&o.stderr, "spawn_path"),
        Some("fallback"),
        "{note}; stderr: {}",
        o.stderr
    );

    // The launcher reports the fork-to-move window in microseconds
    // (`window_us=`), or `unobserved` when it could not see the child in the
    // leaf before the exec. Either is recorded; neither is gated.
    let window = report_field(&o.stderr, "window_us")
        .unwrap_or_else(|| panic!("{note}; no window_us in stderr: {}", o.stderr));
    if window != "unobserved" {
        window
            .parse::<u128>()
            .unwrap_or_else(|_| panic!("{note}; window_us={window:?} is not a number"));
    }
    println!("spawn_fallback_window: spawn_path=fallback window_us={window} [{note}]");

    // The unshimmed launch of the same shape takes clone3, so the shim is
    // what switched the path.
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
    assert_eq!(
        report_field(&o.stderr, "spawn_path"),
        Some("clone3"),
        "stderr: {}",
        o.stderr
    );
    assert!(
        report_field(&o.stderr, "window_us").is_none(),
        "the clone3 path reports no window: {}",
        o.stderr
    );
}

/// A `whois --listen` server on `sock`, optionally under the shim. Only the
/// listener is shimmed: the agent's side is an ordinary launch.
fn spawn_listener(fakes: &[&str], sock: &Path) -> std::process::Child {
    let mut c = if fakes.is_empty() {
        Command::new(launcher_exe())
    } else {
        let mut c = Command::new(env!("CARGO_BIN_EXE_aok-shim"));
        for f in fakes {
            c.arg("--fake").arg(f);
        }
        c.arg("--").arg(launcher_exe());
        c
    };
    c.arg("whois").arg("--listen").arg(sock);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn whois --listen")
}

fn wait_for_socket(sock: &Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if sock.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("listener socket never appeared: {}", sock.display());
}

#[test]
fn identity_fallback_race() {
    let note = "SIMULATED (D-13): PIDFD_GET_INFO is present on this host; the shim made the \
                ioctl return ENOTTY for the listener only, so whois fell back to SO_PEERCRED \
                (peer pid) plus /proc/PID/cgroup on a kernel that has PIDFD_GET_INFO. Not a \
                native-degraded measurement";
    let a = Agent::new("idrace");
    let scratch = Scratch::new("idrace");
    // Pathname sockets: the agent's AbstractUnixSocket scope refuses abstract
    // ones bound outside its domain (D-17).
    let sock_pidfd = scratch.path().join("pidfd.sock");
    let sock_fallback = scratch.path().join("fallback.sock");

    let pidfd_server = spawn_listener(&[], &sock_pidfd);
    let fallback_server = spawn_listener(&["ioctl-enotty"], &sock_fallback);
    wait_for_socket(&sock_pidfd);
    wait_for_socket(&sock_fallback);

    // One live task connects to both listeners, so each resolves the very
    // same task: the pidfd route on one socket, the forced fallback on the
    // other. The task stays alive until both have answered.
    let client = launch(&[
        "run",
        "--name",
        &a.name,
        "--on-behalf-of",
        "alice@example.org",
        "--",
        "python3",
        "-I",
        "-B",
        "-c",
        "import socket, sys, time\n\
         a = socket.socket(socket.AF_UNIX); a.connect(sys.argv[1])\n\
         b = socket.socket(socket.AF_UNIX); b.connect(sys.argv[2])\n\
         time.sleep(1)",
        sock_pidfd.to_str().unwrap(),
        sock_fallback.to_str().unwrap(),
    ]);
    assert_eq!(client.code, 0, "{note}; client stderr: {}", client.stderr);

    let primary = server_out(pidfd_server);
    let fallback = server_out(fallback_server);
    assert_eq!(primary.code, 0, "stderr: {}", primary.stderr);
    assert_eq!(fallback.code, 0, "{note}; stderr: {}", fallback.stderr);
    let (p, f) = (kv(&primary.stdout), kv(&fallback.stdout));

    // The ENOTTY detection actually switched the route.
    assert_eq!(p["path"], "pidfd", "{}", primary.stdout);
    assert_eq!(f["path"], "fallback", "{note}; {}", fallback.stdout);
    assert!(!p.contains_key("window_ns"), "{}", primary.stdout);

    // One key under both routes: the SO_PEERCRED route reaches the same
    // cgroup_id and agent_id for the same live task.
    for k in ["agent_id", "cgroup_id", "parent", "on_behalf_of"] {
        assert_eq!(p[k], f[k], "{note}; {k} differs between routes");
    }
    assert_eq!(p["agent_id"], a.name);
    assert_eq!(p["on_behalf_of"], "alice@example.org");
    assert_eq!(
        p["cgroup_id"],
        fs::metadata(a.leaf()).unwrap().ino().to_string()
    );

    // The pid-reuse window: the interval between taking the peer pid and
    // reading /proc/PID/cgroup. The fallback cannot close it; the test
    // reports it and asserts nothing about its size or about the race never
    // occurring. Only the pidfd route is race-free, which is why it is the
    // preferred one.
    let window: u128 = f["window_ns"]
        .parse()
        .unwrap_or_else(|_| panic!("{note}; window_ns is not a number: {}", fallback.stdout));
    println!(
        "identity_fallback_race: path=fallback window_ns={window} agent_id={} cgroup_id={} \
         [{note}]; the pid-reuse window is reported, not closed",
        f["agent_id"], f["cgroup_id"]
    );
}

fn server_out(child: std::process::Child) -> Out {
    let o = child.wait_with_output().expect("whois --listen");
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}
