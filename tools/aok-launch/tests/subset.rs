//! Subset and confinement tests (LNCH-03). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test subset -- --test-threads=1`
//!
//! Every agent here runs under the real Landlock layer. A sub launch is made
//! by running the launcher inside an agent; it execs inside the parent's
//! domain, so the kernel ANDs the parent's layers with the new one.

mod common;

use std::fs;

use common::*;

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_aok-launch")
}

#[test]
fn nnp() {
    let a = Agent::new("nnp");
    let o = launch(&["run", "--name", &a.name, "--", "cat", "/proc/self/status"]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(
        o.stdout.contains("NoNewPrivs:\t1\n"),
        "NoNewPrivs not set in the agent:\n{}",
        o.stdout
    );
    // The test process itself is not confined: the layers are the agent's.
    let own = fs::read_to_string("/proc/self/status").unwrap();
    assert!(
        own.contains("NoNewPrivs:\t0\n"),
        "test process was confined"
    );
}

#[test]
fn subset_landlock() {
    let outer = Agent::new("sub");
    let s = Scratch::new("subset");
    let a = s.path().join("A");
    let b = s.path().join("B");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("f"), "content-of-a\n").unwrap();
    fs::write(b.join("f"), "content-of-b\n").unwrap();
    let (a_s, b_s) = (a.to_str().unwrap(), b.to_str().unwrap());
    let (rx_a, rx_b) = (format!("rx:{a_s}"), format!("rx:{b_s}"));
    let script = format!("cat {a_s}/f; echo arc=$?; cat {b_s}/f; echo brc=$?");

    // The inner launch asks for A and B; the outer holds only A. The inner
    // agent's authority is the AND of the two layers.
    let o = launch(&[
        "run",
        "--name",
        &outer.name,
        "--grant",
        &rx_a,
        "--",
        exe(),
        "run",
        "--name",
        "inner",
        "--grant",
        &rx_a,
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
        o.stdout.contains("content-of-a"),
        "A unreadable: {}",
        o.stdout
    );
    assert!(o.stdout.contains("arc=0"), "stdout: {}", o.stdout);
    assert!(
        !o.stdout.contains("content-of-b"),
        "the sub-agent read B, which its parent lacks: {}",
        o.stdout
    );
    assert!(o.stdout.contains("brc=1"), "stdout: {}", o.stdout);
    assert!(
        o.stderr.contains("Permission denied"),
        "B denial was not EACCES: {}",
        o.stderr
    );
    // The control: the test itself can read B.
    assert_eq!(fs::read_to_string(b.join("f")).unwrap(), "content-of-b\n");
}

#[test]
fn zero_grant_denies() {
    let a = Agent::new("zero");
    let s = Scratch::new("zero");
    let f = s.path().join("secret");
    fs::write(&f, "secret\n").unwrap();
    let f_s = f.to_str().unwrap();

    // The control: the test reads the file.
    assert_eq!(fs::read_to_string(&f).unwrap(), "secret\n");

    // No --grant: only the default and implicit grants apply, and /tmp is
    // in neither.
    let o = launch(&[
        "run",
        "--name",
        &a.name,
        "--",
        "sh",
        "-c",
        &format!("cat {f_s}"),
    ]);
    assert_ne!(o.code, 0, "an ungranted /tmp read succeeded: {}", o.stdout);
    assert!(
        o.stderr.contains("Permission denied"),
        "expected EACCES: {}",
        o.stderr
    );
    assert!(!o.stdout.contains("secret"), "leaked: {}", o.stdout);
}
