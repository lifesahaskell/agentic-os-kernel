//! D-13 test-only shim. Installs a seccomp filter that makes the kernel look
//! older than it is, then execs the real `aok-launch` so the launcher sees a
//! genuine errno from a genuine syscall and carries no test branch.
//!
//! ```text
//! aok-shim --fake clone3-enosys|clone3-eperm|ioctl-enotty|landlock-enosys [--fake ...] -- CMD...
//! ```
//!
//! - `clone3-enosys`: `clone3` returns ENOSYS (a pre-5.7 kernel).
//! - `ioctl-enotty`: `ioctl` returns ENOTTY when its request argument
//!   (args[1]) is `PIDFD_GET_INFO` (a pre-6.13 kernel).
//! - `clone3-eperm`: `clone3` returns EPERM (a real refusal, not a degraded
//!   kernel; the launcher must not fall back on it).
//! - `landlock-enosys`: `landlock_create_ruleset` returns ENOSYS (Landlock
//!   absent or disabled).
//!
//! This is the only place in the crate that matches on a syscall argument.
//! The production launcher filter (09-03) never reads an argument. The filter
//! is inherited across exec and by every descendant, so it also covers the
//! processes the launcher starts; it never ships in the launcher.

#![deny(unsafe_code)]

use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::process::Command;

use aok_launch::sys::PIDFD_GET_INFO_REQUEST;
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule, apply_filter,
};

fn die(msg: &str) -> ! {
    eprintln!("aok-shim: {msg}");
    std::process::exit(64)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut fakes = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fake" => {
                i += 1;
                match args.get(i) {
                    Some(f) => fakes.push(f.clone()),
                    None => die("--fake needs a value"),
                }
            }
            "--" => break,
            other => die(&format!("unknown argument {other:?}")),
        }
        i += 1;
    }
    let cmd = args.get(i + 1..).unwrap_or(&[]);
    if cmd.is_empty() || fakes.is_empty() {
        die("usage: aok-shim --fake KIND [--fake KIND]... -- CMD...");
    }

    // One filter carries one match action, so group the fakes by errno and
    // stack one filter per errno: seccomp takes the most restrictive result
    // and the faked syscalls are disjoint.
    let mut by_errno: BTreeMap<i32, BTreeMap<i64, Vec<SeccompRule>>> = BTreeMap::new();
    for f in &fakes {
        match f.as_str() {
            // An empty rule list matches the syscall unconditionally.
            "clone3-enosys" => {
                by_errno
                    .entry(libc::ENOSYS)
                    .or_default()
                    .insert(libc::SYS_clone3, vec![]);
            }
            // Not a degraded kernel: a clone3 that fails for a real reason,
            // to check the launcher propagates it instead of falling back.
            "clone3-eperm" => {
                by_errno
                    .entry(libc::EPERM)
                    .or_default()
                    .insert(libc::SYS_clone3, vec![]);
            }
            "landlock-enosys" => {
                by_errno
                    .entry(libc::ENOSYS)
                    .or_default()
                    .insert(libc::SYS_landlock_create_ruleset, vec![]);
            }
            "ioctl-enotty" => {
                // The request is the second argument; compare its low 32 bits.
                let cond = SeccompCondition::new(
                    1,
                    SeccompCmpArgLen::Dword,
                    SeccompCmpOp::Eq,
                    PIDFD_GET_INFO_REQUEST & 0xffff_ffff,
                )
                .unwrap_or_else(|e| die(&format!("condition: {e}")));
                let rule =
                    SeccompRule::new(vec![cond]).unwrap_or_else(|e| die(&format!("rule: {e}")));
                by_errno
                    .entry(libc::ENOTTY)
                    .or_default()
                    .insert(libc::SYS_ioctl, vec![rule]);
            }
            other => die(&format!("unknown fake {other:?}")),
        }
    }
    for (errno, set) in by_errno {
        let filter = SeccompFilter::new(
            set,
            SeccompAction::Allow,
            SeccompAction::Errno(errno as u32),
            std::env::consts::ARCH
                .try_into()
                .unwrap_or_else(|e| die(&format!("arch: {e}"))),
        )
        .unwrap_or_else(|e| die(&format!("filter: {e}")));
        let prog: BpfProgram = filter
            .try_into()
            .unwrap_or_else(|e| die(&format!("compile: {e}")));
        apply_filter(&prog).unwrap_or_else(|e| die(&format!("apply: {e}")));
    }

    let err = Command::new(&cmd[0]).args(&cmd[1..]).exec();
    die(&format!("exec {}: {err}", cmd[0]))
}
