//! Unguessable workspace and session identifiers.

use std::fmt;

/// Failure while drawing an identifier from the system CSPRNG.
#[derive(Debug)]
pub struct IdError {
    source: getrandom::Error,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to generate identifier")
    }
}

impl std::error::Error for IdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn random_bytes() -> Result<[u8; 16], IdError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|source| IdError { source })?;
    Ok(bytes)
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8; 16]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

/// Opaque workspace identifier (local isolation boundary for memory / policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId([u8; 16]);

impl WorkspaceId {
    /// Allocates a new unguessable workspace id.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] when the system CSPRNG cannot fill the identifier.
    #[must_use = "a failed draw must not be treated as an identifier"]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Result<Self, IdError> {
        Ok(Self(random_bytes()?))
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ws-")?;
        write_hex(f, &self.0)
    }
}

/// Logical agent session (bounded history, goal state will attach here later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId([u8; 16]);

impl SessionId {
    /// Allocates a new unguessable session id.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] when the system CSPRNG cannot fill the identifier.
    #[must_use = "a failed draw must not be treated as an identifier"]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Result<Self, IdError> {
        Ok(Self(random_bytes()?))
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sess-")?;
        write_hex(f, &self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionId, WorkspaceId};

    fn assert_opaque(prefix: &str, rendered: &str) {
        let rest = rendered
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("display {rendered} missing {prefix}"));
        assert_eq!(rest.len(), 32, "{rendered}");
        assert!(
            rest.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{rendered}"
        );
        assert_ne!(rendered, format!("{prefix}1"));
    }

    #[test]
    fn workspace_ids_are_distinct_and_not_sequential() {
        let first = WorkspaceId::new().expect("csprng");
        let second = WorkspaceId::new().expect("csprng");
        assert_ne!(first, second);
        assert_opaque("ws-", &first.to_string());
        assert_opaque("ws-", &second.to_string());
    }

    #[test]
    fn session_ids_are_distinct_and_not_sequential() {
        let first = SessionId::new().expect("csprng");
        let second = SessionId::new().expect("csprng");
        assert_ne!(first, second);
        assert_opaque("sess-", &first.to_string());
        assert_opaque("sess-", &second.to_string());
    }
}
