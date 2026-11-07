//! Catalog parsing for `harbor.toml`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use harbor_core::InferError;
use serde::Deserialize;

/// Wire protocol a catalog entry speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// `OpenAI` Chat Completions (`/v1/chat/completions`).
    OpenAiChat,
}

/// One configured endpoint. The API key is not stored here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    /// Stable id used in `provider/model` and in `fallback`.
    pub id: String,
    /// How to speak to `base_url`.
    pub protocol: Protocol,
    /// Origin plus optional prefix, without the `chat/completions` suffix.
    pub base_url: String,
    /// Model name sent on the wire.
    pub model: String,
    /// Environment variable that holds the bearer token, when the endpoint requires one.
    pub api_key_env: Option<String>,
    /// Context window in tokens, when the catalog names one.
    pub context_window: Option<u32>,
}

impl ProviderSpec {
    /// Logical id `provider/model`.
    #[must_use]
    pub fn model_id(&self) -> String {
        format!("{}/{}", self.id, self.model)
    }
}

/// Parsed provider catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    default_id: String,
    fallback: Vec<String>,
    providers: Vec<ProviderSpec>,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    default: String,
    #[serde(default)]
    fallback: Vec<String>,
    providers: Vec<ProviderFile>,
}

#[derive(Debug, Deserialize)]
struct ProviderFile {
    id: String,
    protocol: String,
    base_url: String,
    model: String,
    api_key_env: Option<String>,
    context_window: Option<u32>,
}

impl Catalog {
    /// Reads a catalog from disk.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::MissingConfig`] when `path` does not exist, and
    /// [`InferError::InvalidConfig`] when the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, InferError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(InferError::MissingConfig {
                    path: path.display().to_string(),
                });
            }
            Err(_) => {
                return Err(InferError::InvalidConfig {
                    message: format!("could not read {}", path.display()),
                });
            }
        };
        Self::parse(&text)
    }

    /// Parses catalog TOML.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::InvalidConfig`] when the document is not a Harbor catalog.
    pub fn parse(text: &str) -> Result<Self, InferError> {
        let file: CatalogFile = toml::from_str(text).map_err(|_| InferError::InvalidConfig {
            message: "catalog is not valid toml".to_string(),
        })?;
        validate(file)
    }

    /// Provider id used when `fallback` is empty.
    #[must_use]
    pub fn default_id(&self) -> &str {
        &self.default_id
    }

    /// Ordered provider ids to try. Empty in the file means "just the default".
    #[must_use]
    pub fn chain_ids(&self) -> Vec<String> {
        if self.fallback.is_empty() {
            vec![self.default_id.clone()]
        } else {
            self.fallback.clone()
        }
    }

    /// Looks up a provider by id.
    #[must_use]
    pub fn provider(&self, id: &str) -> Option<&ProviderSpec> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    /// Context window of a provider, when the catalog set one.
    #[must_use]
    pub fn context_window(&self, id: &str) -> Option<u32> {
        self.provider(id)
            .and_then(|provider| provider.context_window)
    }
}

fn validate(file: CatalogFile) -> Result<Catalog, InferError> {
    if file.providers.is_empty() {
        return Err(invalid("catalog has no providers"));
    }
    let mut seen = HashSet::new();
    let mut providers = Vec::with_capacity(file.providers.len());
    for provider in file.providers {
        providers.push(validate_provider(provider, &mut seen)?);
    }
    if !seen.contains(&file.default) {
        return Err(invalid("default provider is not in the catalog"));
    }
    let mut fallback_seen = HashSet::new();
    for id in &file.fallback {
        if !seen.contains(id) {
            return Err(invalid("fallback names an unknown provider"));
        }
        if !fallback_seen.insert(id.clone()) {
            return Err(invalid("fallback lists a provider more than once"));
        }
    }
    Ok(Catalog {
        default_id: file.default,
        fallback: file.fallback,
        providers,
    })
}

fn validate_provider(
    provider: ProviderFile,
    seen: &mut HashSet<String>,
) -> Result<ProviderSpec, InferError> {
    if provider.id.is_empty() || provider.id.contains('/') {
        return Err(invalid(
            "provider id must be non-empty and must not contain '/'",
        ));
    }
    if !seen.insert(provider.id.clone()) {
        return Err(invalid("provider id is duplicated"));
    }
    if provider.protocol != "openai-chat" {
        return Err(invalid("protocol must be openai-chat"));
    }
    if provider.model.is_empty() {
        return Err(invalid("model must be non-empty"));
    }
    validate_base_url(&provider.base_url)?;
    let api_key_env = match provider.api_key_env {
        Some(name) if name.is_empty() => {
            return Err(invalid("api_key_env must be non-empty when set"));
        }
        other => other,
    };
    Ok(ProviderSpec {
        id: provider.id,
        protocol: Protocol::OpenAiChat,
        base_url: provider.base_url,
        model: provider.model,
        api_key_env,
        context_window: provider.context_window,
    })
}

fn validate_base_url(base_url: &str) -> Result<(), InferError> {
    let parsed = reqwest::Url::parse(base_url).map_err(|_| invalid("base_url is not a URL"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(invalid("base_url must be http or https"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(invalid("base_url must not contain credentials"));
    }
    Ok(())
}

fn invalid(message: &str) -> InferError {
    InferError::InvalidConfig {
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::Catalog;
    use harbor_core::InferError;
    use std::path::Path;

    const SAMPLE: &str = r#"
default = "ollama"
fallback = ["ollama", "openai"]

[[providers]]
id = "ollama"
protocol = "openai-chat"
base_url = "http://127.0.0.1:11434/v1"
model = "llama3.2"
context_window = 8192

[[providers]]
id = "openai"
protocol = "openai-chat"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "gpt-4.1-mini"
"#;

    #[test]
    fn parses_example_shape() {
        let catalog = Catalog::parse(SAMPLE).expect("catalog");
        assert_eq!(catalog.default_id(), "ollama");
        assert_eq!(catalog.chain_ids(), ["ollama", "openai"]);
        assert_eq!(catalog.context_window("ollama"), Some(8192));
        let openai = catalog.provider("openai").expect("openai");
        assert_eq!(openai.model_id(), "openai/gpt-4.1-mini");
        assert_eq!(openai.api_key_env.as_deref(), Some("OPENAI_API_KEY"));
    }

    #[test]
    fn rejects_credentials_in_base_url() {
        let text = r#"
default = "local"
[[providers]]
id = "local"
protocol = "openai-chat"
base_url = "http://user:secret@127.0.0.1:11434/v1"
model = "llama3.2"
"#;
        let error = Catalog::parse(text).expect_err("credentials");
        assert!(matches!(error, InferError::InvalidConfig { .. }));
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn missing_file_does_not_invent_a_catalog() {
        let error = Catalog::load(Path::new("/no/such/harbor.toml")).expect_err("missing");
        assert!(matches!(error, InferError::MissingConfig { .. }));
    }
}
