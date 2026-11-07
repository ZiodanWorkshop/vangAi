//! Sandbox error types.

use thiserror::Error;

/// Errors from sandbox initialization or enforcement.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// Running on a non-Linux OS without the real backend.
    #[error("sandbox backend unsupported on this platform (build for Linux)")]
    UnsupportedPlatform,
    /// Landlock or configuration failure.
    #[error("sandbox init: {0}")]
    Init(String),
    /// Failure applying Landlock rules.
    #[error("landlock: {0}")]
    Landlock(String),
}
