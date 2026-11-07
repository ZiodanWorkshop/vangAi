//! Completion types and errors shared by every inference backend.

use std::fmt;

use crate::Message;

/// A tool invocation requested by the model. Empty until tool use is wired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// Provider id for this call, when the wire format includes one.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// JSON arguments as a string.
    pub arguments: String,
}

/// Token counts reported by the provider. Missing counts stay at zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    /// Tokens consumed by the prompt.
    pub input_tokens: u32,
    /// Tokens produced by the model.
    pub output_tokens: u32,
}

/// One non-streaming model result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Assistant text. Empty when the model returned only tool calls.
    pub text: String,
    /// Tool calls from this completion. Empty for a plain text reply.
    pub tool_calls: Vec<ToolCall>,
    /// Usage reported with the reply.
    pub usage: Usage,
}

/// Input to a single completion call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    /// Logical model id, `provider/model`.
    pub model: String,
    /// Conversation so far, oldest first.
    pub messages: Vec<Message>,
}

/// Failure while resolving a provider or completing a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferError {
    /// The catalog file does not exist.
    MissingConfig {
        /// Path that was requested.
        path: String,
    },
    /// The catalog text is not a valid Harbor catalog.
    InvalidConfig {
        /// Human-readable reason. Never includes an API key.
        message: String,
    },
    /// The named environment variable is missing or empty.
    MissingApiKey {
        /// Environment variable the catalog asked for.
        env: String,
    },
    /// The provider rejected credentials. The fallback chain stops.
    Unauthorized {
        /// Provider id from the catalog.
        provider: String,
    },
    /// The provider rejected the request. The fallback chain stops.
    BadRequest {
        /// Provider id from the catalog.
        provider: String,
    },
    /// Timeout, connection failure, HTTP 429, or HTTP 5xx.
    Retryable {
        /// Provider id from the catalog.
        provider: String,
        /// HTTP status when the failure came from a response.
        status: Option<u16>,
    },
    /// The response was not a chat completion.
    Protocol {
        /// Provider id from the catalog.
        provider: String,
    },
    /// The catalog did not name a provider to call.
    NoProvider,
}

impl InferError {
    /// Whether another provider in the chain may be tried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable { .. })
    }
}

impl fmt::Display for InferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfig { path } => write!(f, "missing config: {path}"),
            Self::InvalidConfig { message } => write!(f, "invalid config: {message}"),
            Self::MissingApiKey { env } => write!(f, "missing api key: {env}"),
            Self::Unauthorized { provider } => write!(f, "provider {provider} unauthorized"),
            Self::BadRequest { provider } => write!(f, "provider {provider} rejected the request"),
            Self::Retryable {
                provider,
                status: Some(status),
            } => write!(f, "provider {provider} unavailable (status {status})"),
            Self::Retryable {
                provider,
                status: None,
            } => write!(f, "provider {provider} unavailable"),
            Self::Protocol { provider } => {
                write!(f, "provider {provider} returned an unexpected response")
            }
            Self::NoProvider => f.write_str("no provider configured"),
        }
    }
}

impl std::error::Error for InferError {}
