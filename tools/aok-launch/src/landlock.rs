//! Landlock layer: the grant syntax, the default and D-12 implicit grants, the
//! D-17 ABI-6 ruleset, and the D-14 ABI floor.
//!
//! `lib.rs` calls `preflight` in the parent before any cgroup or registry
//! write, and `apply` in the clone3 child before the seccomp layer. This module
//! is safe Rust over the `landlock` crate; the only raw syscall (the ABI
//! version probe) lives in `sys.rs`.
//!
//! # What an agent can reach
//!
//! One ruleset per spawn (D-17), applied pre-exec and kept in (D-15). It
//! handles every ABI-6 filesystem right (V1-V5), `ConnectTcp` (V4) and the
//! `Signal` and `AbstractUnixSocket` scopes (V6); the V7 audit flags are not
//! enabled. `CompatLevel::HardRequirement` means no right silently degrades.
//! Exactly three sets of paths are granted:
//!
//! 1. The `--grant SPEC` values: `rx:PATH` is read-file, read-dir and execute
//!    beneath PATH; `rw:PATH` is every handled fs right except execute beneath
//!    PATH; `tcp:PORT` allows TCP connect to that port (with no `tcp` grant
//!    every TCP connect is denied).
//! 2. The default grant (decided here, Claude's discretion under D-12):
//!    `rx` on `/usr`, `/bin`, `/lib` and `/lib64` where they exist, read-only
//!    on `/etc` and `/proc`, `rw` on `/dev/null`. Never `/tmp`, never `$HOME`,
//!    and nothing under `/run`, so the D-08 registry under
//!    `/run/user/UID/aok-launch` is out of every default agent's reach (T-09-15).
//! 3. The D-12 implicit grants built from the `LayerSpec`: execute and
//!    read-file on the launcher executable, so the agent can exec `aok-launch`
//!    for a sub launch, and `rw` beneath the agent's own cgroup directory, so a
//!    sub launch can create `<agent>/<N>/main` and move a child into it.
//!
//! A sub launch execs inside its parent's domain, so the kernel ANDs the
//! parent's layers with the new one: a layer only ever narrows (T-09-06). The
//! kernel allows at most 16 stacked layers; the 17th fails `E2BIG` (D-18).

use std::error::Error as StdError;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use landlock::{
    ABI, Access, AccessFs, AccessNet, BitFlags, CompatLevel, Compatible, NetPort, PathBeneath,
    PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetError, RulesetStatus, Scope,
};

use crate::sys;

/// The D-14 floor lives in `aok-launch.conf` and nowhere else. The file is
/// embedded at build time, so the running launcher opens no config file an
/// agent could rewrite, and cargo rebuilds when the file changes.
const CONF: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/aok-launch.conf"));

/// The one key the config carries.
const FLOOR_KEY: &str = "landlock_abi_floor";

/// Default grant, `rx`: read-file, read-dir and execute.
pub const DEFAULT_RX: [&str; 4] = ["/usr", "/bin", "/lib", "/lib64"];
/// Default grant, read-only: read-file and read-dir.
pub const DEFAULT_RO: [&str; 2] = ["/etc", "/proc"];
/// Default grant, `rw` on single files.
pub const DEFAULT_RW_FILES: [&str; 1] = ["/dev/null"];

/// What the layer seam needs to build a ruleset.
#[derive(Debug, Clone)]
pub struct LayerSpec {
    /// The `--grant SPEC` values, unparsed and in command-line order.
    pub grants: Vec<String>,
    /// The agent's cgroup directory, not yet created when `preflight` runs.
    pub agent_dir: PathBuf,
    /// The resolved path of the running launcher executable.
    pub launcher_exe: PathBuf,
}

/// How a layer failure maps onto the exit contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerErrorKind {
    /// Bad flags or grant syntax: exit 64.
    Usage,
    /// The launcher refuses or cannot proceed: exit 125, errno named.
    Refused,
}

/// A layer, identity or setup failure carrying its exit-contract class.
#[derive(Debug, Clone)]
pub struct LayerError {
    pub kind: LayerErrorKind,
    pub errno: i32,
    pub msg: String,
}

impl LayerError {
    pub fn usage(errno: i32, msg: impl Into<String>) -> Self {
        LayerError {
            kind: LayerErrorKind::Usage,
            errno,
            msg: msg.into(),
        }
    }

    pub fn refused(errno: i32, msg: impl Into<String>) -> Self {
        LayerError {
            kind: LayerErrorKind::Refused,
            errno,
            msg: msg.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// The D-14 floor.
// ---------------------------------------------------------------------------

/// Parse the `landlock_abi_floor` value out of config text. Blank lines and
/// `#` comments are skipped; exactly one `landlock_abi_floor = N` line with N
/// in 1..=255 is required, and any other key is an error.
pub fn floor_from_conf(text: &str) -> Result<u32, LayerError> {
    let bad = |why: &str| {
        LayerError::refused(libc::EINVAL, format!("aok-launch.conf: {FLOOR_KEY}: {why}"))
    };
    let mut floor = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(bad("line is not key = value"));
        };
        if key.trim() != FLOOR_KEY {
            return Err(bad("unknown key"));
        }
        if floor.is_some() {
            return Err(bad("duplicate key"));
        }
        let value = value.trim();
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad("value is not an integer"));
        }
        let n: u32 = value.parse().map_err(|_| bad("value is out of range"))?;
        if !(1..=255).contains(&n) {
            return Err(bad("value must be 1..=255"));
        }
        floor = Some(n);
    }
    floor.ok_or_else(|| bad("key is missing"))
}

/// The rights ABI `abi` adds over ABI `abi - 1`, for the refusal message.
fn rights_added_by(abi: u32) -> &'static str {
    match abi {
        1 => "ABI 1: the base filesystem rights (Landlock itself)",
        2 => "ABI 2: filesystem REFER",
        3 => "ABI 3: filesystem TRUNCATE",
        4 => "ABI 4: network BIND_TCP and CONNECT_TCP",
        5 => "ABI 5: filesystem IOCTL_DEV",
        6 => "ABI 6: scopes SIGNAL and ABSTRACT_UNIX_SOCKET",
        7 => "ABI 7: restrict-self audit logging flags",
        _ => "an ABI newer than this launcher knows",
    }
}

/// Compare the host's reported ABI against the floor. At or above the floor is
/// `Ok`; below it is a refusal naming `EOPNOTSUPP` and each right the host
/// lacks, one per ABI from `reported + 1` to `floor`. Reported 0 means
/// Landlock is absent or disabled.
pub fn floor_check(reported: u32, floor: u32) -> Result<(), LayerError> {
    if reported >= floor {
        return Ok(());
    }
    let missing: Vec<&str> = (reported + 1..=floor).map(rights_added_by).collect();
    let state = if reported == 0 {
        "Landlock is absent or disabled".to_string()
    } else {
        format!("host Landlock ABI is {reported}")
    };
    Err(LayerError::refused(
        libc::EOPNOTSUPP,
        format!(
            "landlock: {state}, below the floor of ABI {floor}; missing rights: {}",
            missing.join("; ")
        ),
    ))
}

// ---------------------------------------------------------------------------
// Grant SPEC syntax.
// ---------------------------------------------------------------------------

/// One parsed `--grant SPEC`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// `rx:PATH`.
    Rx(PathBuf),
    /// `rw:PATH`.
    Rw(PathBuf),
    /// `tcp:PORT`.
    Tcp(u16),
}

/// Parse one SPEC: `rx:PATH`, `rw:PATH` or `tcp:PORT`, PATH absolute and PORT
/// in 1..=65535. Any other shape is a usage error (exit 64).
pub fn parse_grant(spec: &str) -> Result<Grant, LayerError> {
    let bad = |why: &str| LayerError::usage(libc::EINVAL, format!("--grant {spec:?}: {why}"));
    let Some((kind, rest)) = spec.split_once(':') else {
        return Err(bad("expected rx:PATH, rw:PATH or tcp:PORT"));
    };
    match kind {
        "rx" | "rw" => {
            if !rest.starts_with('/') {
                return Err(bad("PATH must be absolute"));
            }
            let p = PathBuf::from(rest);
            Ok(if kind == "rx" {
                Grant::Rx(p)
            } else {
                Grant::Rw(p)
            })
        }
        "tcp" => {
            if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad("PORT must be a decimal number"));
            }
            match rest.parse::<u16>() {
                Ok(p) if p >= 1 => Ok(Grant::Tcp(p)),
                _ => Err(bad("PORT must be in 1..=65535")),
            }
        }
        _ => Err(bad("expected rx:PATH, rw:PATH or tcp:PORT")),
    }
}

fn parse_all(specs: &[String]) -> Result<Vec<Grant>, LayerError> {
    specs.iter().map(|s| parse_grant(s)).collect()
}

// ---------------------------------------------------------------------------
// preflight
// ---------------------------------------------------------------------------

/// The host's Landlock ABI; `ENOSYS` and `EOPNOTSUPP` count as 0.
fn host_abi() -> Result<u32, LayerError> {
    match sys::landlock_create_ruleset_version() {
        Ok(v) => Ok(v),
        Err(e) if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EOPNOTSUPP)) => Ok(0),
        Err(e) => Err(LayerError::refused(
            e.raw_os_error().unwrap_or(libc::EIO),
            "landlock: version probe",
        )),
    }
}

/// Parent-side validation, run before any cgroup or registry write: grant
/// syntax (exit 64), the ABI floor (exit 125, `EOPNOTSUPP`), and that every
/// grant path exists (exit 125, `ENOENT`).
pub fn preflight(spec: &LayerSpec) -> Result<(), LayerError> {
    let grants = parse_all(&spec.grants)?;
    floor_check(host_abi()?, floor_from_conf(CONF)?)?;
    for g in &grants {
        if let Grant::Rx(p) | Grant::Rw(p) = g
            && let Err(e) = fs::metadata(p)
        {
            return Err(LayerError::refused(
                e.raw_os_error().unwrap_or(libc::EIO),
                format!("--grant path {}", p.display()),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// apply
// ---------------------------------------------------------------------------

/// The rights a regular file (not a directory) can carry in a rule.
fn file_rights() -> BitFlags<AccessFs> {
    AccessFs::Execute
        | AccessFs::WriteFile
        | AccessFs::ReadFile
        | AccessFs::Truncate
        | AccessFs::IoctlDev
}

/// Every handled fs right except execute: the `rw` grant.
fn rw_rights() -> BitFlags<AccessFs> {
    AccessFs::from_all(ABI::V6) & !BitFlags::from(AccessFs::Execute)
}

fn rx_rights() -> BitFlags<AccessFs> {
    AccessFs::ReadFile | AccessFs::ReadDir | AccessFs::Execute
}

fn ro_rights() -> BitFlags<AccessFs> {
    AccessFs::ReadFile | AccessFs::ReadDir
}

/// The first `io::Error` in an error's source chain, as an errno.
fn errno_in_chain(e: &(dyn StdError + 'static)) -> Option<i32> {
    let mut cur: Option<&(dyn StdError + 'static)> = Some(e);
    while let Some(err) = cur {
        if let Some(io) = err.downcast_ref::<io::Error>()
            && let Some(n) = io.raw_os_error()
        {
            return Some(n);
        }
        cur = err.source();
    }
    None
}

/// A ruleset-builder failure. A kernel errno anywhere in the chain (`E2BIG`
/// from the 17th stacked layer, say) is kept; a compatibility refusal with no
/// errno behind it is `EOPNOTSUPP`.
fn ruleset_err(what: &str, e: &RulesetError) -> LayerError {
    LayerError::refused(
        errno_in_chain(e).unwrap_or(libc::EOPNOTSUPP),
        format!("landlock: {what}"),
    )
}

/// A rule for `path` carrying `access`, masked to what the path's type can
/// carry. `None` if nothing is left after masking.
fn rule(
    path: &Path,
    access: BitFlags<AccessFs>,
) -> Result<Option<PathBeneath<PathFd>>, LayerError> {
    let fd = PathFd::new(path).map_err(|e| {
        LayerError::refused(
            errno_in_chain(&e).unwrap_or(libc::ENOENT),
            format!("landlock: grant path {}", path.display()),
        )
    })?;
    let access = match fs::metadata(path) {
        Ok(m) if !m.is_dir() => access & file_rights(),
        _ => access,
    };
    Ok((!access.is_empty()).then(|| PathBeneath::new(fd, access)))
}

/// Child-side ruleset application, run before `execve`. The order the caller
/// fixed is landlock, then seccomp, then NoNewPrivs.
pub fn apply(spec: &LayerSpec) -> Result<(), LayerError> {
    let grants = parse_all(&spec.grants)?;

    // (path, rights) in rule order: operator grants, default grant, D-12.
    let mut fs_rules: Vec<(PathBuf, BitFlags<AccessFs>, bool)> = Vec::new();
    let mut ports = Vec::new();
    for g in &grants {
        match g {
            Grant::Rx(p) => fs_rules.push((p.clone(), rx_rights(), true)),
            Grant::Rw(p) => fs_rules.push((p.clone(), rw_rights(), true)),
            Grant::Tcp(port) => ports.push(*port),
        }
    }
    // The default grant. Directories that do not exist on this host are
    // skipped; nothing here is /tmp, $HOME or under /run.
    for p in DEFAULT_RX {
        fs_rules.push((PathBuf::from(p), rx_rights(), false));
    }
    for p in DEFAULT_RO {
        fs_rules.push((PathBuf::from(p), ro_rights(), false));
    }
    for p in DEFAULT_RW_FILES {
        fs_rules.push((PathBuf::from(p), rw_rights(), false));
    }
    // D-12 implicit grants: exec the launcher, write beneath the own agent dir.
    fs_rules.push((
        spec.launcher_exe.clone(),
        AccessFs::Execute | AccessFs::ReadFile,
        true,
    ));
    fs_rules.push((spec.agent_dir.clone(), rw_rights(), true));

    let abi = ABI::V6;
    let mut created = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(abi))
        .and_then(|r| r.handle_access(AccessNet::ConnectTcp))
        .and_then(|r| r.scope(Scope::from_all(abi)))
        .and_then(|r| r.create())
        .map_err(|e| ruleset_err("create ruleset", &e))?;

    for (path, access, required) in fs_rules {
        // Operator grants and the D-12 grants must exist; a default
        // directory a host lacks (/lib64 on some systems) is skipped.
        if !required && !path.exists() {
            continue;
        }
        if let Some(r) = rule(&path, access)? {
            created = created
                .add_rule(r)
                .map_err(|e| ruleset_err("add path rule", &e))?;
        }
    }
    for port in ports {
        created = created
            .add_rule(NetPort::new(port, AccessNet::ConnectTcp))
            .map_err(|e| ruleset_err("add port rule", &e))?;
    }

    let status = created
        .restrict_self()
        .map_err(|e| ruleset_err("restrict self", &e))?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        return Err(LayerError::refused(
            libc::EOPNOTSUPP,
            "landlock: ruleset not fully enforced",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_from_the_real_conf_is_six() {
        assert_eq!(floor_from_conf(CONF).unwrap(), 6);
    }

    #[test]
    fn floor_from_conf_rejects_junk() {
        for (text, why) in [
            ("", "no key"),
            ("# only a comment\n", "no key"),
            (
                "landlock_abi_floor = 6\nlandlock_abi_floor = 7\n",
                "duplicate",
            ),
            ("landlock_abi_floor = six\n", "non-integer"),
            ("landlock_abi_floor =\n", "empty value"),
            ("landlock_abi_floor = 0\n", "zero"),
            ("landlock_abi_floor = 256\n", "too big"),
            ("landlock_abi_floor = -1\n", "negative"),
            ("landlock_abi_flor = 6\n", "typo key"),
            ("landlock_abi_floor 6\n", "no equals"),
        ] {
            let e = floor_from_conf(text).expect_err(why);
            assert_eq!(e.errno, libc::EINVAL, "{why}");
            assert!(e.msg.contains(FLOOR_KEY), "{why}: {}", e.msg);
        }
        assert_eq!(
            floor_from_conf("\n# c\n  landlock_abi_floor=3  \n\n").unwrap(),
            3
        );
    }

    #[test]
    fn floor_check_refuses_below_and_names_the_scopes() {
        let e = floor_check(5, 6).unwrap_err();
        assert_eq!(e.kind, LayerErrorKind::Refused);
        assert_eq!(e.errno, libc::EOPNOTSUPP);
        assert!(e.msg.contains("SIGNAL"), "{}", e.msg);
        assert!(e.msg.contains("ABSTRACT_UNIX_SOCKET"), "{}", e.msg);
        assert!(!e.msg.contains("IOCTL_DEV"), "5 has IOCTL_DEV: {}", e.msg);
    }

    #[test]
    fn floor_check_passes_at_and_above() {
        assert!(floor_check(7, 6).is_ok());
        assert!(floor_check(6, 6).is_ok());
    }

    #[test]
    fn floor_check_zero_means_absent_and_names_every_right() {
        let e = floor_check(0, 6).unwrap_err();
        assert!(e.msg.contains("absent"), "{}", e.msg);
        for right in ["REFER", "TRUNCATE", "CONNECT_TCP", "IOCTL_DEV", "SIGNAL"] {
            assert!(e.msg.contains(right), "{right} missing: {}", e.msg);
        }
    }

    #[test]
    fn parse_grant_accepts_the_three_shapes() {
        assert_eq!(
            parse_grant("rx:/usr").unwrap(),
            Grant::Rx(PathBuf::from("/usr"))
        );
        assert_eq!(
            parse_grant("rw:/tmp/a b").unwrap(),
            Grant::Rw(PathBuf::from("/tmp/a b"))
        );
        assert_eq!(parse_grant("tcp:1").unwrap(), Grant::Tcp(1));
        assert_eq!(parse_grant("tcp:65535").unwrap(), Grant::Tcp(65535));
    }

    #[test]
    fn parse_grant_rejects_every_other_shape_as_usage() {
        for spec in [
            "",
            "rx",
            "rx:",
            "rx:relative/path",
            "rx:./x",
            "rw:",
            "ro:/usr",
            "RX:/usr",
            "tcp:",
            "tcp:0",
            "tcp:65536",
            "tcp:+80",
            "tcp:-1",
            "tcp:80x",
            "tcp:/usr",
            "/usr",
            "tcp",
        ] {
            let e = parse_grant(spec).expect_err(spec);
            assert_eq!(e.kind, LayerErrorKind::Usage, "{spec}");
            assert_eq!(e.errno, libc::EINVAL, "{spec}");
        }
    }

    #[test]
    fn the_default_grant_never_reaches_tmp_home_or_run() {
        for p in DEFAULT_RX
            .iter()
            .chain(DEFAULT_RO.iter())
            .chain(DEFAULT_RW_FILES.iter())
        {
            assert!(
                !p.starts_with("/tmp") && !p.starts_with("/home") && !p.starts_with("/run"),
                "default grant {p} is too wide"
            );
        }
    }

    #[test]
    fn rw_is_every_handled_right_but_execute() {
        let rw = rw_rights();
        assert!(!rw.contains(AccessFs::Execute));
        assert!(rw.contains(AccessFs::WriteFile));
        assert!(rw.contains(AccessFs::MakeDir));
        assert!(rw.contains(AccessFs::IoctlDev));
        assert_eq!(rw | AccessFs::Execute, AccessFs::from_all(ABI::V6));
    }

    #[test]
    fn preflight_rejects_bad_specs_before_probing_the_kernel() {
        let spec = LayerSpec {
            grants: vec!["rx:relative".into()],
            agent_dir: PathBuf::from("/nonexistent/agent"),
            launcher_exe: PathBuf::from("/nonexistent/aok-launch"),
        };
        assert_eq!(preflight(&spec).unwrap_err().kind, LayerErrorKind::Usage);
    }

    #[test]
    fn preflight_reports_a_missing_grant_path_as_enoent() {
        let spec = LayerSpec {
            grants: vec!["rw:/nonexistent/aok-grant-path".into()],
            agent_dir: PathBuf::from("/nonexistent/agent"),
            launcher_exe: PathBuf::from("/nonexistent/aok-launch"),
        };
        // Passes the floor on a Landlock host; on a host below the floor the
        // refusal would be EOPNOTSUPP, so only assert the ENOENT case when
        // the host meets the floor.
        match preflight(&spec) {
            Err(e) if e.errno == libc::ENOENT => assert_eq!(e.kind, LayerErrorKind::Refused),
            Err(e) => assert_eq!(e.errno, libc::EOPNOTSUPP, "{}", e.msg),
            Ok(()) => panic!("a missing grant path must be refused"),
        }
    }
}
