//! Acceptance (e), via the real binary rather than `run_cli` in-process
//! (main.rs's own unit test covers the in-process path; this covers the CLI
//! boundary, including argument parsing).

use std::process::Command;

#[test]
fn broker_pid_is_rejected_at_the_cli_with_the_adv05_message() {
    let out_path = std::env::temp_dir().join(format!(
        "aok-adv-broker-pid-cli-test-{}.jsonl",
        std::process::id()
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_aok-adv"))
        .arg("run")
        .arg("--out")
        .arg(&out_path)
        .arg("--broker-pid")
        .arg("1")
        .output()
        .expect("run aok-adv run --broker-pid 1");

    assert!(
        !output.status.success(),
        "--broker-pid must be rejected, not accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ADV-05"),
        "expected the ADV-05 message on stderr, got: {stderr}"
    );
    assert!(
        !out_path.exists(),
        "--broker-pid rejection must not write an output file"
    );
}
