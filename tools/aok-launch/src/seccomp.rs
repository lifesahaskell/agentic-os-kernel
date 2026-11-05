//! Seccomp layer seam. 09-01 fixes the call site; 09-03 installs the
//! hand-assembled filter.

use crate::landlock::LayerError;

/// Child-side filter installation, run after `landlock::apply` and before
/// `sys::set_no_new_privs`. Stub (09-01): installs nothing.
pub fn apply() -> Result<(), LayerError> {
    Ok(())
}
