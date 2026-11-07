//! `OpenAI`-compatible inference for Harbor.
//!
//! The agent loop depends on [`Inference`], not on a vendor SDK.

#![forbid(unsafe_code)]

mod config;
mod openai;
mod scripted;

pub use config::{Catalog, Protocol, ProviderSpec};
pub use openai::{http_client, ApiKeySource, EnvApiKeys, OpenAiChatClient};
pub use scripted::ScriptedInference;

use async_trait::async_trait;
use harbor_core::{Completion, CompletionRequest, InferError};

/// One non-streaming completion. Callers must not assume a vendor wire format.
#[async_trait]
pub trait Inference: Send + Sync {
    /// Completes `request`.
    ///
    /// # Errors
    ///
    /// Returns [`InferError`] when the provider cannot answer. Retryable failures
    /// are marked with [`InferError::is_retryable`].
    async fn complete(&self, request: &CompletionRequest) -> Result<Completion, InferError>;
}

/// Tries each client in order. A non-retryable error stops the chain.
pub struct FallbackInference<I> {
    steps: Vec<I>,
}

impl<I> FallbackInference<I> {
    /// Builds a chain. An empty list is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::NoProvider`] when `steps` is empty.
    pub fn new(steps: Vec<I>) -> Result<Self, InferError> {
        if steps.is_empty() {
            return Err(InferError::NoProvider);
        }
        Ok(Self { steps })
    }

    /// Clients in attempt order.
    #[must_use]
    pub fn steps(&self) -> &[I] {
        &self.steps
    }
}

#[async_trait]
impl<I> Inference for FallbackInference<I>
where
    I: Inference + Sync,
{
    async fn complete(&self, request: &CompletionRequest) -> Result<Completion, InferError> {
        let mut last = None;
        for (index, step) in self.steps.iter().enumerate() {
            match step.complete(request).await {
                Ok(completion) => return Ok(completion),
                Err(error) if error.is_retryable() && index + 1 < self.steps.len() => {
                    last = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last.unwrap_or(InferError::NoProvider))
    }
}

impl Catalog {
    /// Builds the configured `OpenAI`-compatible chain.
    ///
    /// Keys are not read here. Each call reads `api_key_env` from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`InferError`] when the HTTP client or a catalog URL cannot be built.
    pub fn open_ai_chain(&self) -> Result<FallbackInference<OpenAiChatClient>, InferError> {
        let http = http_client()?;
        let mut steps = Vec::new();
        for id in self.chain_ids() {
            let Some(spec) = self.provider(&id) else {
                return Err(InferError::InvalidConfig {
                    message: format!("fallback provider {id} is missing"),
                });
            };
            steps.push(OpenAiChatClient::from_spec(spec, http.clone())?);
        }
        FallbackInference::new(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::{FallbackInference, Inference, ScriptedInference};
    use harbor_core::{Completion, CompletionRequest, InferError, Message, Usage};

    fn request() -> CompletionRequest {
        CompletionRequest {
            model: "ollama/llama3.2".to_string(),
            messages: vec![Message::user("ping")],
        }
    }

    fn pong() -> Completion {
        Completion {
            text: "pong".to_string(),
            tool_calls: Vec::new(),
            usage: Usage {
                input_tokens: 1,
                output_tokens: 1,
            },
        }
    }

    #[tokio::test]
    async fn scripted_completion_is_returned_as_queued() {
        let scripted = ScriptedInference::new(vec![Ok(pong())]);
        let completion = scripted.complete(&request()).await.expect("scripted");
        assert_eq!(completion.text, "pong");
        assert!(completion.tool_calls.is_empty());
        assert_eq!(scripted.calls(), 1);
    }

    #[tokio::test]
    async fn fallback_skips_a_retryable_failure_and_stops_on_auth() {
        let first = ScriptedInference::new(vec![Err(InferError::Retryable {
            provider: "ollama".to_string(),
            status: Some(503),
        })]);
        let second = ScriptedInference::new(vec![Ok(pong())]);
        let chain = FallbackInference::new(vec![first, second]).expect("chain");
        let completion = chain.complete(&request()).await.expect("fallback");
        assert_eq!(completion.text, "pong");
        assert_eq!(chain.steps()[0].calls(), 1);
        assert_eq!(chain.steps()[1].calls(), 1);

        let blocked = ScriptedInference::new(vec![Err(InferError::Unauthorized {
            provider: "openai".to_string(),
        })]);
        let unused = ScriptedInference::new(vec![Ok(pong())]);
        let chain = FallbackInference::new(vec![blocked, unused]).expect("chain");
        let error = chain.complete(&request()).await.expect_err("auth");
        assert!(matches!(error, InferError::Unauthorized { .. }));
        assert_eq!(chain.steps()[1].calls(), 0);
    }
}
