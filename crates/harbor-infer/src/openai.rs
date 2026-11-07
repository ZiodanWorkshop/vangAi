//! `OpenAI` Chat Completions client.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use harbor_core::{Completion, CompletionRequest, InferError, Role, ToolCall, Usage};
use serde::Deserialize;

use crate::config::ProviderSpec;
use crate::Inference;

/// Reads a bearer token at call time. Implementations must not log the value.
pub trait ApiKeySource: Send + Sync {
    /// Returns the variable's value when it is set and non-empty.
    fn get(&self, name: &str) -> Option<String>;
}

/// Reads keys from the process environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvApiKeys;

impl ApiKeySource for EnvApiKeys {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }
}

/// One Chat Completions endpoint. Sends its configured model name, not the caller's prefix.
#[derive(Clone)]
pub struct OpenAiChatClient {
    provider_id: String,
    model: String,
    endpoint: reqwest::Url,
    api_key_env: Option<String>,
    keys: Arc<dyn ApiKeySource>,
    http: reqwest::Client,
}

impl OpenAiChatClient {
    /// Builds a client that reads keys from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::InvalidConfig`] when `spec.base_url` is not a usable HTTP URL.
    pub fn from_spec(spec: &ProviderSpec, http: reqwest::Client) -> Result<Self, InferError> {
        Ok(Self {
            provider_id: spec.id.clone(),
            model: spec.model.clone(),
            endpoint: chat_endpoint(&spec.base_url)?,
            api_key_env: spec.api_key_env.clone(),
            keys: Arc::new(EnvApiKeys),
            http,
        })
    }

    /// Replaces the key source. Used so tests never touch the process environment.
    #[must_use]
    pub fn with_keys(mut self, keys: Arc<dyn ApiKeySource>) -> Self {
        self.keys = keys;
        self
    }

    /// Logical id `provider/model`.
    #[must_use]
    pub fn model_id(&self) -> String {
        format!("{}/{}", self.provider_id, self.model)
    }

    /// Provider id from the catalog.
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }
}

impl std::fmt::Debug for OpenAiChatClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiChatClient")
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .field("endpoint", &self.endpoint.as_str())
            .field("api_key_env", &self.api_key_env)
            .finish_non_exhaustive()
    }
}

/// Shared HTTP client. Redirects are disabled so a bearer token cannot leave the catalog host.
///
/// # Errors
///
/// Returns [`InferError::InvalidConfig`] when the client cannot be constructed.
pub fn http_client() -> Result<reqwest::Client, InferError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| InferError::InvalidConfig {
            message: "failed to build http client".to_string(),
        })
}

fn chat_endpoint(base_url: &str) -> Result<reqwest::Url, InferError> {
    let mut base = base_url.trim().to_string();
    if !base.ends_with('/') {
        base.push('/');
    }
    let parsed = reqwest::Url::parse(&base).map_err(|_| InferError::InvalidConfig {
        message: "base_url is not a URL".to_string(),
    })?;
    parsed
        .join("chat/completions")
        .map_err(|_| InferError::InvalidConfig {
            message: "base_url cannot join chat/completions".to_string(),
        })
}

#[async_trait]
impl Inference for OpenAiChatClient {
    async fn complete(&self, request: &CompletionRequest) -> Result<Completion, InferError> {
        let mut builder = self
            .http
            .post(self.endpoint.clone())
            .json(&payload(self, request));
        if let Some(env) = &self.api_key_env {
            let Some(key) = self.keys.get(env) else {
                return Err(InferError::MissingApiKey { env: env.clone() });
            };
            builder = builder.bearer_auth(key);
        }
        let response = match builder.send().await {
            Ok(response) => response,
            Err(error) => {
                return Err(InferError::Retryable {
                    provider: self.provider_id.clone(),
                    status: error.status().map(|status| status.as_u16()),
                });
            }
        };
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(InferError::Unauthorized {
                provider: self.provider_id.clone(),
            });
        }
        let client_error =
            status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS;
        if client_error {
            return Err(InferError::BadRequest {
                provider: self.provider_id.clone(),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(InferError::Retryable {
                provider: self.provider_id.clone(),
                status: Some(status.as_u16()),
            });
        }
        if !status.is_success() {
            return Err(InferError::Retryable {
                provider: self.provider_id.clone(),
                status: Some(status.as_u16()),
            });
        }
        let body = response.text().await.map_err(|_| InferError::Retryable {
            provider: self.provider_id.clone(),
            status: Some(status.as_u16()),
        })?;
        parse_completion(&self.provider_id, &body)
    }
}

fn payload(client: &OpenAiChatClient, request: &CompletionRequest) -> serde_json::Value {
    let messages: Vec<_> = request
        .messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "role": role_name(message.role),
                "content": message.content,
            })
        })
        .collect();
    serde_json::json!({
        "model": client.model,
        "messages": messages,
    })
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

fn parse_completion(provider: &str, body: &str) -> Result<Completion, InferError> {
    let parsed: ChatResponse = serde_json::from_str(body).map_err(|_| InferError::Protocol {
        provider: provider.to_string(),
    })?;
    let Some(choice) = parsed.choices.into_iter().next() else {
        return Err(InferError::Protocol {
            provider: provider.to_string(),
        });
    };
    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|call| ToolCall {
            id: call.id.unwrap_or_default(),
            name: call.function.name,
            arguments: call.function.arguments,
        })
        .collect();
    let usage = parsed.usage.map_or_else(Usage::default, |usage| Usage {
        input_tokens: tokens(usage.prompt_tokens),
        output_tokens: tokens(usage.completion_tokens),
    });
    Ok(Completion {
        text: choice.message.content.unwrap_or_default(),
        tool_calls,
        usage,
    })
}

fn tokens(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<UsageBody>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    content: Option<String>,
    tool_calls: Option<Vec<WireToolCall>>,
}

#[derive(Debug, Deserialize)]
struct WireToolCall {
    id: Option<String>,
    function: WireFunction,
}

#[derive(Debug, Deserialize)]
struct WireFunction {
    name: String,
    #[serde(default)]
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct UsageBody {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::{parse_completion, OpenAiChatClient};
    use crate::config::{Catalog, ProviderSpec};
    use crate::http_client;
    use crate::{FallbackInference, Inference};
    use harbor_core::{CompletionRequest, InferError, Message};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct MapKeys {
        values: Mutex<Vec<(String, String)>>,
    }

    impl MapKeys {
        fn new(values: Vec<(&str, &str)>) -> Self {
            Self {
                values: Mutex::new(
                    values
                        .into_iter()
                        .map(|(name, value)| (name.to_string(), value.to_string()))
                        .collect(),
                ),
            }
        }
    }

    impl super::ApiKeySource for MapKeys {
        fn get(&self, name: &str) -> Option<String> {
            let values = self
                .values
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            values
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    struct LocalServer {
        base_url: String,
        hits: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<String>>>,
    }

    async fn spawn_server(status: u16, body: &'static str) -> LocalServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("addr");
        let hits = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let hits_task = Arc::clone(&hits);
        let requests_task = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                hits_task.fetch_add(1, Ordering::SeqCst);
                let mut buf = Vec::new();
                let mut tmp = [0_u8; 2048];
                loop {
                    let read =
                        tokio::time::timeout(Duration::from_secs(2), socket.read(&mut tmp)).await;
                    match read {
                        Ok(Ok(0) | Err(_)) | Err(_) => break,
                        Ok(Ok(count)) => {
                            buf.extend_from_slice(&tmp[..count]);
                            if request_complete(&buf) {
                                break;
                            }
                        }
                    }
                }
                requests_task
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(String::from_utf8_lossy(&buf).into_owned());
                let header = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(header.as_bytes()).await;
                let _ = socket.write_all(body.as_bytes()).await;
            }
        });
        LocalServer {
            base_url: format!("http://{address}/v1"),
            hits,
            requests,
        }
    }

    fn request_complete(buf: &[u8]) -> bool {
        let Some(split) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&buf[..split]);
        let length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        });
        match length {
            Some(length) => buf.len() >= split + 4 + length,
            None => true,
        }
    }

    fn client(base_url: &str, api_key_env: Option<&str>) -> OpenAiChatClient {
        let spec = ProviderSpec {
            id: "local".to_string(),
            protocol: crate::config::Protocol::OpenAiChat,
            base_url: base_url.to_string(),
            model: "llama3.2".to_string(),
            api_key_env: api_key_env.map(str::to_string),
            context_window: Some(1024),
        };
        OpenAiChatClient::from_spec(&spec, http_client().expect("http"))
            .expect("client")
            .with_keys(Arc::new(MapKeys::new(vec![])))
    }

    fn request() -> CompletionRequest {
        CompletionRequest {
            model: "local/llama3.2".to_string(),
            messages: vec![Message::user("ping")],
        }
    }

    #[test]
    fn parses_text_and_leaves_tool_calls_empty() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#;
        let completion = parse_completion("local", body).expect("parse");
        assert_eq!(completion.text, "pong");
        assert!(completion.tool_calls.is_empty());
        assert_eq!(completion.usage.input_tokens, 3);
        assert_eq!(completion.usage.output_tokens, 1);
    }

    #[tokio::test]
    async fn missing_key_does_not_call_the_network() {
        let server = spawn_server(200, r#"{"choices":[{"message":{"content":"no"}}]}"#).await;
        let client = client(&server.base_url, Some("HARBOR_TEST_ABSENT_KEY"));
        let error = client.complete(&request()).await.expect_err("missing key");
        assert!(matches!(error, InferError::MissingApiKey { .. }));
        assert_eq!(server.hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn posts_chat_completions_without_authorization() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#;
        let server = spawn_server(200, body).await;
        let client = client(&server.base_url, None);
        let completion = client.complete(&request()).await.expect("completion");
        assert_eq!(completion.text, "pong");
        assert!(completion.tool_calls.is_empty());
        let recorded = server
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let raw = recorded.first().expect("request");
        assert!(raw.starts_with("POST /v1/chat/completions "));
        assert!(raw.contains(r#""model":"llama3.2""#));
        assert!(!raw.to_ascii_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn bearer_header_is_sent_and_not_copied_into_the_error() {
        let server = spawn_server(401, r#"{"error":"no"}"#).await;
        let spec = Catalog::parse(&format!(
            r#"
default = "local"
[[providers]]
id = "local"
protocol = "openai-chat"
base_url = "{}"
api_key_env = "HARBOR_TEST_KEY"
model = "llama3.2"
"#,
            server.base_url
        ))
        .expect("catalog");
        let provider = spec.provider("local").expect("provider");
        let client = OpenAiChatClient::from_spec(provider, http_client().expect("http"))
            .expect("client")
            .with_keys(Arc::new(MapKeys::new(vec![(
                "HARBOR_TEST_KEY",
                "super-secret-token",
            )])));
        let error = client.complete(&request()).await.expect_err("unauthorized");
        assert!(matches!(error, InferError::Unauthorized { .. }));
        assert!(!error.to_string().contains("super-secret-token"));
        let recorded = server
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let raw = recorded.first().expect("request");
        assert!(raw
            .to_ascii_lowercase()
            .contains("authorization: bearer super-secret-token"));
    }

    #[tokio::test]
    async fn fallback_tries_the_next_server_after_a_5xx() {
        let down = spawn_server(500, r#"{"error":"down"}"#).await;
        let up = spawn_server(200, r#"{"choices":[{"message":{"content":"pong"}}]}"#).await;
        let chain = FallbackInference::new(vec![
            client(&down.base_url, None),
            client(&up.base_url, None),
        ])
        .expect("chain");
        let completion = chain.complete(&request()).await.expect("fallback");
        assert_eq!(completion.text, "pong");
        assert_eq!(down.hits.load(Ordering::SeqCst), 1);
        assert_eq!(up.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn bad_request_does_not_call_the_next_server() {
        let bad = spawn_server(400, r#"{"error":"bad"}"#).await;
        let up = spawn_server(200, r#"{"choices":[{"message":{"content":"pong"}}]}"#).await;
        let chain = FallbackInference::new(vec![
            client(&bad.base_url, None),
            client(&up.base_url, None),
        ])
        .expect("chain");
        let error = chain.complete(&request()).await.expect_err("bad request");
        assert!(matches!(error, InferError::BadRequest { .. }));
        assert_eq!(up.hits.load(Ordering::SeqCst), 0);
    }
}
