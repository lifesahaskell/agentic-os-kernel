//! Identity seam. 09-01 fixes the signatures and call sites; 09-02 fills the
//! registry, the pidfd resolution and `whois`.

use std::path::Path;

use crate::cgroup::{self, Role};
use crate::landlock::LayerError;

/// Decide the launch role and refuse a launch that must not happen.
/// Stub (09-01): the role is whatever the own cgroup path says; 09-02 adds
/// the registry half (D-08, D-12).
pub fn resolve_role(_on_behalf_of: Option<&str>) -> Result<Role, LayerError> {
    cgroup::launch_role().map_err(|e| {
        LayerError::refused(
            e.raw_os_error().unwrap_or(libc::EIO),
            "cannot read own cgroup",
        )
    })
}

/// Record a new root agent in the registry. Called for a Root launch only,
/// after the agent directory exists and before the child is cloned.
/// Stub (09-01): records nothing.
pub fn register_root(_agent_dir: &Path, _on_behalf_of: Option<&str>) -> Result<(), LayerError> {
    Ok(())
}

/// `aok-launch whois ARGS...`: `args` is everything after `whois`. Returns
/// the process exit code. Stub (09-01).
pub fn whois(_args: &[String]) -> i32 {
    eprintln!("aok-launch: whois: unimplemented in 09-01");
    125
}
