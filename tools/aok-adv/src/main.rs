#![deny(unsafe_code)]
//! aok-adv: the ADV-01/02/03 adversarial suite and stub broker (08-02).
//! Every raw syscall lives in `sys` (D-13); every other module is safe Rust.
//! Subcommands: `stub`, `run`, `list-routes`, `report` (run/list-routes/
//! report are fleshed out across Tasks 1-3 of this plan).

mod stub;
#[allow(unsafe_code)]
mod sys;

use std::process::ExitCode;

fn print_usage() {
    eprintln!("usage: aok-adv <stub|run|list-routes|report> [args...]");
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.collect();
    let result = match cmd.as_str() {
        "stub" => stub::run_cli(&rest),
        "list-routes" => list_routes_cli(&rest),
        "run" => run_cli(&rest),
        "report" => report_cli(&rest),
        other => {
            eprintln!("aok-adv: unknown subcommand '{other}'");
            print_usage();
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aok-adv: {e}");
            ExitCode::FAILURE
        }
    }
}

fn list_routes_cli(_args: &[String]) -> std::io::Result<()> {
    // Task 2 (routes.rs) replaces this body with the D-16 route table.
    Ok(())
}

fn run_cli(_args: &[String]) -> std::io::Result<()> {
    // Task 2/3 (routes.rs, cases.rs, report.rs) fill this in.
    Ok(())
}

fn report_cli(_args: &[String]) -> std::io::Result<()> {
    // Task 3 (report.rs) fills this in.
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::sys;
    use std::process::Command;
    use std::time::Duration;

    /// The kill route, non-agent, --no-cgroup: a suite-spawned child killed
    /// by `pidfd_send_signal(SIGKILL)`, verified by the pidfd reporting exit
    /// (ADV-02). Never touches an existing process: the child is spawned by
    /// this test and nothing else; the pidfd is opened by this test, not
    /// derived from a raw pid handed around elsewhere.
    #[test]
    fn kill_route_pidfd_send_signal_non_agent() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .expect("spawn target child");
        let pid = child.id() as libc::pid_t;

        let pidfd = sys::pidfd_open(pid, 0).expect("pidfd_open the spawned child");
        sys::pidfd_send_signal(pidfd, libc::SIGKILL).expect("pidfd_send_signal SIGKILL");

        let exited =
            sys::poll_readable(pidfd, Duration::from_secs(5)).expect("poll pidfd for exit");
        assert!(exited, "pidfd never reported exit after SIGKILL");

        let _ = child.wait();
        sys::close_fd(pidfd);
    }
}
