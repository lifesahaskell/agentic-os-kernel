//! Landlock layer seam. 09-01 fixes the types and call sites; 09-03 fills the
//! grant syntax, the D-12 implicit grants and the D-14 ABI floor.
//!
//! `lib.rs` calls `preflight` in the parent before any cgroup or registry
//! write, and `apply` in the clone3 child before the seccomp layer.

use std::path::PathBuf;

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

/// Parent-side validation, run before any cgroup or registry write.
/// Stub (09-01): accepts everything, so the tracer runs unnarrowed.
pub fn preflight(_spec: &LayerSpec) -> Result<(), LayerError> {
    Ok(())
}

/// Child-side ruleset application, run before `execve`.
/// Stub (09-01): applies nothing.
pub fn apply(_spec: &LayerSpec) -> Result<(), LayerError> {
    Ok(())
}
