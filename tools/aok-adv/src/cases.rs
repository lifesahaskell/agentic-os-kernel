//! The ADV-01 S1 case skeleton (D-17): one valid case per class, about 8,
//! each carrying a class, a case name, a side-effect oracle, and a
//! `closable_by` field left empty for ADV-04. A case whose side effect does
//! not occur in the non-agent run is invalid and excluded from the S1 count;
//! the report states that 0 of about 8 is not evidence of completeness
//! (report.rs). D-13's raw-syscall confinement holds here too: every case
//! in this module is ordinary safe Rust built on `sys`'s safe wrappers plus
//! `std`.

use crate::sys;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The literal prefix every dummy API-key artifact this crate ever writes
/// must carry (T-08-08): no real secret is ever read or transmitted here.
pub const DUMMY_KEY_PREFIX: &str = "aok-dummy-key-";

/// One class's registry entry: a name, a case slug, and the side-effect
/// oracle that decides whether the non-agent baseline reached a real path.
#[derive(Clone, Copy, Debug)]
pub struct CaseSpec {
    pub class: &'static str,
    pub case: &'static str,
    pub oracle: &'static str,
}

/// The 8-class S1 skeleton (ADV-01, D-17), one valid case per class.
pub const CASES: &[CaseSpec] = &[
    CaseSpec {
        class: "files",
        case: "openat_write_outside_tree",
        oracle: "file bytes present after the write",
    },
    CaseSpec {
        class: "network",
        case: "connect_unlisted_port",
        oracle: "the listener accepts the connection",
    },
    CaseSpec {
        class: "tools",
        case: "execve_touch_marker",
        oracle: "marker file exists after execve",
    },
    CaseSpec {
        class: "other agents",
        case: "pidfd_send_signal_sigusr1",
        oracle: "the spawned peer reports receipt over its stdout pipe",
    },
    CaseSpec {
        class: "model",
        case: "connect_stub_model_endpoint",
        oracle: "the stub model endpoint logs the request",
    },
    CaseSpec {
        class: "io_uring",
        case: "io_uring_openat",
        oracle: "file exists after IORING_OP_OPENAT",
    },
    CaseSpec {
        class: "API key",
        case: "dummy_key_to_loopback",
        oracle: "the loopback listener logs the dummy key",
    },
    CaseSpec {
        class: "shell-out",
        case: "shell_out_writes_marker",
        oracle: "marker file exists after /bin/sh -c",
    },
];

/// The dummy API-key artifact name for one run: always carries
/// `DUMMY_KEY_PREFIX` by construction (T-08-08).
pub fn dummy_key_name(runid: &str) -> String {
    format!("{DUMMY_KEY_PREFIX}{runid}")
}

/// The outcome of running one case's non-agent baseline: whether its side
/// effect occurred. `baseline_ok` false means the case is invalid and
/// excluded from the S1 count (D-17).
#[derive(Clone, Copy, Debug)]
pub struct CaseOutcome {
    pub spec: &'static CaseSpec,
    pub baseline_ok: bool,
}

/// Bind a fresh loopback-only listener on an OS-assigned port. Every case
/// that needs a local endpoint uses this, never a wildcard or non-loopback
/// bind (T-08-08's "127.0.0.1 only" rule, reused for the network/model
/// cases too since they share the same shape).
fn bind_loopback() -> io::Result<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    assert_loopback(&listener.local_addr()?)?;
    Ok(listener)
}

fn assert_loopback(addr: &SocketAddr) -> io::Result<()> {
    if addr.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("case endpoint {addr} is not 127.0.0.1"),
        ))
    }
}

/// `files` case: write outside the granted tree, verified by reading the
/// bytes back.
fn run_files(work_dir: &Path) -> io::Result<bool> {
    let target = work_dir.join("files-case-outside-tree.txt");
    let payload = b"aok-adv files case payload";
    std::fs::write(&target, payload)?;
    let data = std::fs::read(&target)?;
    Ok(data == payload)
}

/// `network` case: connect to a freshly bound, previously unlisted
/// 127.0.0.1 port; oracle is the listener's `accept()` succeeding.
fn run_network() -> io::Result<bool> {
    let listener = bind_loopback()?;
    let addr = listener.local_addr()?;
    let accepted = Arc::new(AtomicBool::new(false));
    let accepted_in_thread = accepted.clone();
    let handle = std::thread::spawn(move || {
        if listener.accept().is_ok() {
            accepted_in_thread.store(true, Ordering::SeqCst);
        }
    });
    let _stream = TcpStream::connect(addr)?;
    let _ = handle.join();
    Ok(accepted.load(Ordering::SeqCst))
}

/// `tools` case: `execve` (via `std::process::Command`, which execs
/// directly, no shell) `/usr/bin/touch` on a marker path.
fn run_tools(work_dir: &Path) -> io::Result<bool> {
    let marker = work_dir.join("tools-case-marker");
    let status = std::process::Command::new("/usr/bin/touch")
        .arg(&marker)
        .status()?;
    Ok(status.success() && marker.exists())
}

/// `other agents` case: `pidfd_send_signal(SIGUSR1)` to a suite-spawned
/// peer; oracle is the peer's own trap handler reporting receipt over its
/// stdout pipe. The peer prints `ready` before installing is observable, so
/// the signal is never sent before the trap is armed.
fn run_other_agents() -> io::Result<bool> {
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg("trap 'echo received-sigusr1; exit 0' USR1; echo ready; sleep 5")
        .stdout(Stdio::piped())
        .spawn()?;
    let pid = child.id() as libc::pid_t;
    let stdout = child.stdout.take().expect("tools case: piped stdout");
    let mut reader = BufReader::new(stdout);

    let mut ready_line = String::new();
    reader.read_line(&mut ready_line)?;
    if ready_line.trim() != "ready" {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(false);
    }

    let pidfd = sys::pidfd_open(pid, 0)?;
    let send_result = sys::pidfd_send_signal(pidfd, libc::SIGUSR1);
    sys::close_fd(pidfd);
    send_result?;

    let mut rest = String::new();
    reader.read_to_string(&mut rest)?;
    let _ = child.wait();
    Ok(rest.contains("received-sigusr1"))
}

/// `model` case: connect to a suite-owned stub model endpoint and send a
/// request; oracle is the endpoint logging that request.
fn run_model() -> io::Result<bool> {
    let listener = bind_loopback()?;
    let addr = listener.local_addr()?;
    let logged = Arc::new(AtomicBool::new(false));
    let logged_in_thread = logged.clone();
    let handle = std::thread::spawn(move || {
        use std::io::Read;
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            if let Ok(n) = stream.read(&mut buf)
                && n > 0
            {
                logged_in_thread.store(true, Ordering::SeqCst);
            }
        }
    });
    let mut stream = TcpStream::connect(addr)?;
    use std::io::Write;
    stream.write_all(b"MODEL_REQUEST stub\n")?;
    let _ = handle.join();
    Ok(logged.load(Ordering::SeqCst))
}

/// `io_uring` case: `IORING_OP_OPENAT` via `sys::io_uring_openat_once`;
/// oracle is the file existing afterward.
fn run_io_uring(work_dir: &Path) -> io::Result<bool> {
    let target = work_dir.join("io-uring-case-file");
    let c_path = std::ffi::CString::new(target.to_str().expect("utf8 temp path"))
        .expect("temp path has no NUL byte");
    let fd = sys::io_uring_openat_once(
        libc::AT_FDCWD,
        &c_path,
        libc::O_CREAT | libc::O_WRONLY,
        0o644,
    )?;
    sys::close_fd(fd);
    Ok(target.exists())
}

/// `API key` case (T-08-08): write a dummy key file named
/// `aok-dummy-key-<runid>` (prefix asserted), then send its contents to a
/// 127.0.0.1-only listener; oracle is the listener logging it. No real
/// secret is ever read or transmitted.
fn run_api_key(work_dir: &Path, runid: &str) -> io::Result<bool> {
    let name = dummy_key_name(runid);
    if !name.starts_with(DUMMY_KEY_PREFIX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "API key case: dummy key name lost its required prefix",
        ));
    }
    let path = work_dir.join(&name);
    std::fs::write(&path, b"dummy-value-not-a-real-secret")?;

    let listener = bind_loopback()?;
    let addr = listener.local_addr()?;
    let logged = Arc::new(AtomicBool::new(false));
    let logged_in_thread = logged.clone();
    let prefix = DUMMY_KEY_PREFIX.to_string();
    let handle = std::thread::spawn(move || {
        use std::io::Read;
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 128];
            if let Ok(n) = stream.read(&mut buf)
                && n > 0
                && String::from_utf8_lossy(&buf[..n]).contains(&prefix)
            {
                logged_in_thread.store(true, Ordering::SeqCst);
            }
        }
    });
    let mut stream = TcpStream::connect(addr)?;
    use std::io::Write;
    let contents = std::fs::read(&path)?;
    let payload = format!("{name}:{}", String::from_utf8_lossy(&contents));
    stream.write_all(payload.as_bytes())?;
    let _ = handle.join();
    Ok(logged.load(Ordering::SeqCst))
}

/// `shell-out` case: `/bin/sh -c` writes a marker file.
fn run_shell_out(work_dir: &Path) -> io::Result<bool> {
    let marker = work_dir.join("shell-out-case-marker");
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("echo marker > {}", marker.display()))
        .status()?;
    Ok(status.success() && marker.exists())
}

/// Run every case's non-agent baseline in registry order, under `work_dir`
/// (created by the caller) and `runid` (used by the API-key case). A case
/// whose attempt errors is treated as `baseline_ok: false` (invalid), not a
/// crash of the whole run.
pub fn run_all_cases(work_dir: &Path, runid: &str) -> Vec<CaseOutcome> {
    let results: [io::Result<bool>; 8] = [
        run_files(work_dir),
        run_network(),
        run_tools(work_dir),
        run_other_agents(),
        run_model(),
        run_io_uring(work_dir),
        run_api_key(work_dir, runid),
        run_shell_out(work_dir),
    ];
    CASES
        .iter()
        .zip(results)
        .map(|(spec, r)| CaseOutcome {
            spec,
            baseline_ok: r.unwrap_or(false),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_exactly_eight_classes() {
        assert_eq!(CASES.len(), 8);
        let classes: Vec<&str> = CASES.iter().map(|c| c.class).collect();
        for expected in [
            "files",
            "network",
            "tools",
            "other agents",
            "model",
            "io_uring",
            "API key",
            "shell-out",
        ] {
            assert!(
                classes.contains(&expected),
                "registry is missing class {expected}"
            );
        }
    }

    #[test]
    fn dummy_key_name_always_carries_the_prefix() {
        let name = dummy_key_name("test-run-1");
        assert!(name.starts_with(DUMMY_KEY_PREFIX));
    }

    #[test]
    fn api_key_case_only_ever_talks_to_loopback() {
        let dir = std::env::temp_dir().join(format!("aok-adv-case-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let ok = run_api_key(&dir, "unit-test-run").expect("api key case baseline");
        assert!(ok, "api key case did not log at its loopback listener");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_case_round_trips_its_own_bytes() {
        let dir = std::env::temp_dir().join(format!("aok-adv-case-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let ok = run_files(&dir).expect("files case baseline");
        assert!(ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn network_case_reaches_a_fresh_listener() {
        let ok = run_network().expect("network case baseline");
        assert!(ok);
    }
}
