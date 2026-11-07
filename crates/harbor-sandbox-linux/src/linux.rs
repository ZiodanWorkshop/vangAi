//! Linux implementation: Landlock ABI check and read-only path rule (MVP).

use std::path::Path;

use landlock::{path::PathFd, Access, AccessFs, PathBeneath, Ruleset, RulesetAttr, ABI};

use crate::{PlatformInfo, SandboxError};

/// Handle for sandbox configuration.
#[derive(Debug)]
pub struct Sandbox {
    _private: (),
}

impl Sandbox {
    /// Verifies that Landlock is supported on this kernel.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::Init`] if the Landlock ABI is unavailable on this kernel.
    pub fn new() -> Result<Self, SandboxError> {
        let abi = ABI::V1;
        RulesetAttr::try_from(abi).map_err(|e| SandboxError::Init(e.to_string()))?;
        Ok(Self { _private: () })
    }

    /// Applies Landlock: read-only file access beneath `root` for the current thread.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::Landlock`] if opening `root` or installing rules fails.
    pub fn apply_read_only_beneath(root: &Path) -> Result<(), SandboxError> {
        let fd = PathFd::new(root).map_err(|e| SandboxError::Landlock(e.to_string()))?;
        let access = AccessFs::from_all(Access::FsReadFile);
        let rule =
            PathBeneath::new(&fd, access).map_err(|e| SandboxError::Landlock(e.to_string()))?;
        Ruleset::new()
            .handle_access(access)
            .map_err(|e| SandboxError::Landlock(e.to_string()))?
            .add_rules([rule])
            .map_err(|e| SandboxError::Landlock(e.to_string()))?
            .restrict_self()
            .map_err(|e| SandboxError::Landlock(e.to_string()))
    }
}

/// Returns platform capabilities for observability.
#[must_use]
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        name: "linux",
        landlock_supported: landlock_supported(),
    }
}

fn landlock_supported() -> bool {
    RulesetAttr::try_from(ABI::V1).is_ok()
}
