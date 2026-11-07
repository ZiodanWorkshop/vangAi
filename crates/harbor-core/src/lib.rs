//! Harbor core types shared across crates (sessions, workspace ids, messages).

#![forbid(unsafe_code)]

mod id;
mod infer;
mod message;

pub use id::{IdError, SessionId, WorkspaceId};
pub use infer::{Completion, CompletionRequest, InferError, ToolCall, Usage};
pub use message::{Message, Role};
