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

use std::process::Command;

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
