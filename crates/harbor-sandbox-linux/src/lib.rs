//! Linux-focused sandbox for tool execution.
//!
//! On non-Linux targets this crate compiles stubs so the workspace builds everywhere;
//! the real isolation policy is implemented only for `target_os = "linux"`.

#![forbid(unsafe_code)]

mod error;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
mod stub;

pub use error::SandboxError;

/// Describe what the current build supports.
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    /// OS / backend label (`linux` or `stub`).
    pub name: &'static str,
    /// Whether Landlock rules can be applied on this host.
    pub landlock_supported: bool,
}

#[cfg(target_os = "linux")]
pub use linux::{platform_info, Sandbox};

#[cfg(not(target_os = "linux"))]
pub use stub::{platform_info, Sandbox};
