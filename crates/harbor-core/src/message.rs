//! Chat messages shared by the agent loop and inference clients.

/// Speaker of a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Instructions that frame the conversation.
    System,
    /// The person or channel talking to the agent.
    User,
    /// A previous model reply.
    Assistant,
}

/// One textual chat turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Who produced this turn.
    pub role: Role,
    /// Plain text. Tool payloads stay on [`crate::ToolCall`].
    pub content: String,
}

impl Message {
    /// Builds a system message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// Builds a user message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// Builds an assistant message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}
