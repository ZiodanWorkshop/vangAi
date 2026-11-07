//! Non-Linux stub: same types, enforcement returns [`UnsupportedPlatform`](crate::SandboxError::UnsupportedPlatform).

use std::path::Path;

use crate::{PlatformInfo, SandboxError};

/// Handle for sandbox configuration (not available off Linux).
#[derive(Debug)]
pub struct Sandbox {
    _private: (),
}

impl Sandbox {
    /// Stubs: sandbox not available off Linux.
    ///
    /// # Errors
    ///
    /// Always returns [`SandboxError::UnsupportedPlatform`].
    pub fn new() -> Result<Self, SandboxError> {
        Err(SandboxError::UnsupportedPlatform)
    }

    /// No-op stub.
    ///
    /// # Errors
    ///
    /// Always returns [`SandboxError::UnsupportedPlatform`].
    pub fn apply_read_only_beneath(_root: &Path) -> Result<(), SandboxError> {
        Err(SandboxError::UnsupportedPlatform)
    }
}

/// Non-Linux build: no Landlock.
#[must_use]
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        name: "stub",
        landlock_supported: false,
    }
}
