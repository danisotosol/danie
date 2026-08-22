use std::path::PathBuf;

use serde::Deserialize;

use crate::anthropic::AnthropicProvider;
use crate::error::LlmError;
use crate::openai_compat::OpenAiCompatProvider;
use crate::provider::LlmProvider;

/// Directory name under the OS config dir holding `config.toml`.
const CONFIG_DIR: &str = "danie";
const CONFIG_FILE: &str = "config.toml";

/// Default provider selected when none is configured.
const DEFAULT_PROVIDER: &str = "anthropic";
/// Default model when neither `[default].model` nor an override is set.
const DEFAULT_MODEL: &str = "claude-sonnet-4-5";
/// Default env var consulted for the Anthropic API key.
const DEFAULT_ANTHROPIC_KEY_ENV: &str = "ANTHROPIC_API_KEY";
/// Default endpoint for the OpenAI-compatible provider (Ollama).
const DEFAULT_OPENAI_COMPAT_URL: &str = "http://localhost:11434/v1";

/// Full deserialized contents of `danie/config.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub default: DefaultSection,
    pub providers: ProvidersSection,
}

/// The `[default]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DefaultSection {
    /// `"anthropic"` or `"openai-compat"`.
    pub provider: String,
    /// Fallback model used by providers without their own override.
    pub model: String,
}

impl Default for DefaultSection {
    fn default() -> Self {
        Self {
            provider: DEFAULT_PROVIDER.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

/// The `[providers]` table.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ProvidersSection {
    pub anthropic: AnthropicSection,
    pub openai_compat: OpenAiCompatSection,
}

/// The `[providers.anthropic]` table.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AnthropicSection {
    /// Env var holding the Anthropic API key.
    pub api_key_env: String,
    /// Optional base URL for proxies/gateways.
    pub base_url: Option<String>,
}

impl Default for AnthropicSection {
    fn default() -> Self {
        Self {
            api_key_env: DEFAULT_ANTHROPIC_KEY_ENV.to_string(),
            base_url: None,
        }
    }
}

/// The `[providers.openai_compat]` table.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OpenAiCompatSection {
    /// Endpoint exposing `/chat/completions`.
    pub base_url: Option<String>,
    /// Optional env var holding the API key; unset means no auth header.
    pub api_key_env: Option<String>,
    /// Overrides `[default].model` when this provider is selected.
    pub model: Option<String>,
}

impl Default for OpenAiCompatSection {
    fn default() -> Self {
        Self {
            base_url: Some(DEFAULT_OPENAI_COMPAT_URL.to_string()),
            api_key_env: None,
            model: None,
        }
    }
}

fn resolve_env_var(resolve_env: &dyn Fn(&str) -> Option<String>, env_var: &str) -> Option<String> {
    resolve_env(env_var).filter(|value| !value.trim().is_empty())
}

fn require_key(
    resolve_env: &dyn Fn(&str) -> Option<String>,
    env_var: &str,
) -> Result<String, LlmError> {
    resolve_env_var(resolve_env, env_var)
        .ok_or_else(|| LlmError::MissingKey {
            env_var: env_var.to_string(),
        })
        .map(|value| value.trim().to_string())
}

/// Builds a provider from already-parsed config sections plus an
/// environment resolver, so callers (and tests) can control how API keys
/// are looked up without mutating the process environment.
///
/// # Arguments
/// * `provider_name` - value of `[default].provider` (`"anthropic"` or
///   `"openai-compat"`).
/// * `default_model` - value of `[default].model`, used when the selected
///   provider has no model override.
/// * `resolve_env` - lookup function for API key environment variables.
pub fn provider_from_sections(
    provider_name: &str,
    default_model: &str,
    anthropic: &AnthropicSection,
    openai_compat: &OpenAiCompatSection,
    resolve_env: &dyn Fn(&str) -> Option<String>,
) -> Result<Box<dyn LlmProvider>, LlmError> {
    match provider_name {
        "anthropic" => {
            let api_key = require_key(resolve_env, &anthropic.api_key_env)?;
            let mut provider = AnthropicProvider::new(api_key, default_model);
            if let Some(base_url) = &anthropic.base_url {
                provider = provider.with_base_url(base_url);
            }
            Ok(Box::new(provider))
        }
        "openai-compat" => {
            let api_key = openai_compat
                .api_key_env
                .as_deref()
                .and_then(|env_var| resolve_env_var(resolve_env, env_var));
            let base_url = openai_compat
                .base_url
                .clone()
                .unwrap_or_else(|| DEFAULT_OPENAI_COMPAT_URL.to_string());
            Ok(Box::new(OpenAiCompatProvider::new(
                base_url,
                api_key,
                openai_compat
                    .model
                    .clone()
                    .unwrap_or_else(|| default_model.to_string()),
            )))
        }
        other => Err(LlmError::Config(format!(
            "unknown provider `{other}` in [default].provider"
        ))),
    }
}

impl Config {
    /// Absolute path of the config file
    /// (`{config_dir}/danie/{CONFIG_FILE}`).
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(CONFIG_DIR)
            .join(CONFIG_FILE)
    }

    /// Loads the configuration from [`Config::path`]. A missing file yields
    /// the defaults; unreadable files or invalid TOML yield a
    /// [`LlmError::Config`].
    pub fn load() -> Result<Self, LlmError> {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => Self::from_toml_str(&contents),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(LlmError::Config(format!(
                "cannot read {}: {err}",
                path.display()
            ))),
        }
    }

    /// Parses configuration from a TOML string (test-friendly variant of
    /// [`Config::load`]).
    pub fn from_toml_str(contents: &str) -> Result<Self, LlmError> {
        toml::from_str(contents).map_err(|err| LlmError::Config(format!("invalid config: {err}")))
    }

    /// Instantiates the provider selected in the configuration, resolving
    /// its API key from the process environment.
    pub fn create_provider(&self) -> Result<Box<dyn LlmProvider>, LlmError> {
        self.create_provider_with(&|var| std::env::var(var).ok())
    }

    /// Like [`Config::create_provider`] but with a custom environment
    /// resolver (dependency-injection seam for tests).
    pub fn create_provider_with(
        &self,
        resolve_env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Box<dyn LlmProvider>, LlmError> {
        provider_from_sections(
            &self.default.provider,
            &self.default.model,
            &self.providers.anthropic,
            &self.providers.openai_compat,
            resolve_env,
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::provider::{ChatRequest, Message, Role, DEFAULT_MAX_TOKENS};

    const SAMPLE_TOML: &str = r#"
        [default]
        provider = "openai-compat"
        model = "gpt-4o-mini"

        [providers.anthropic]
        api_key_env = "MY_ANTHROPIC_KEY"
        base_url = "https://proxy.example.com"

        [providers.openai_compat]
        base_url = "http://localhost:1234/v1"
        api_key_env = "LM_STUDIO_KEY"
        model = "qwen2.5-coder"
    "#;

    #[test]
    fn parses_sample_toml() {
        let config = Config::from_toml_str(SAMPLE_TOML).unwrap();
        assert_eq!(config.default.provider, "openai-compat");
        assert_eq!(config.default.model, "gpt-4o-mini");
        assert_eq!(config.providers.anthropic.api_key_env, "MY_ANTHROPIC_KEY");
        assert_eq!(
            config.providers.anthropic.base_url.as_deref(),
            Some("https://proxy.example.com")
        );
        assert_eq!(
            config.providers.openai_compat.base_url.as_deref(),
            Some("http://localhost:1234/v1")
        );
        assert_eq!(
            config.providers.openai_compat.api_key_env.as_deref(),
            Some("LM_STUDIO_KEY")
        );
        assert_eq!(
            config.providers.openai_compat.model.as_deref(),
            Some("qwen2.5-coder")
        );
    }

    #[test]
    fn empty_toml_yields_documented_defaults() {
        let config = Config::from_toml_str("").unwrap();
        assert_eq!(config.default.provider, "anthropic");
        assert_eq!(config.default.model, "claude-sonnet-4-5");
        assert_eq!(config.providers.anthropic.api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(config.providers.anthropic.base_url, None);
        assert_eq!(
            config.providers.openai_compat.base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(config.providers.openai_compat.api_key_env, None);
        assert_eq!(config.providers.openai_compat.model, None);
    }

    #[test]
    fn invalid_toml_is_a_config_error() {
        let err = Config::from_toml_str("[default").unwrap_err();
        assert!(matches!(err, LlmError::Config(_)));
    }

    #[test]
    fn anthropic_without_resolvable_key_reports_missing_env_var() {
        let resolve = |_: &str| None;
        let outcome = provider_from_sections(
            "anthropic",
            "claude-sonnet-4-5",
            &AnthropicSection::default(),
            &OpenAiCompatSection::default(),
            &resolve,
        );
        let Err(err) = outcome else {
            panic!("expected MissingKey error");
        };
        match err {
            LlmError::MissingKey { env_var } => assert_eq!(env_var, "ANTHROPIC_API_KEY"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn openai_compat_needs_no_api_key() {
        let resolve = |_: &str| None;
        let provider = provider_from_sections(
            "openai-compat",
            "claude-sonnet-4-5",
            &AnthropicSection::default(),
            &OpenAiCompatSection {
                base_url: Some("http://localhost:11434/v1".to_string()),
                api_key_env: None,
                model: None,
            },
            &resolve,
        )
        .unwrap();
        let _ = provider;
    }

    #[test]
    fn blank_env_value_counts_as_missing() {
        let resolve = |var: &str| (var == "ANTHROPIC_API_KEY").then(|| "   ".to_string());
        let outcome = provider_from_sections(
            "anthropic",
            "claude-sonnet-4-5",
            &AnthropicSection::default(),
            &OpenAiCompatSection::default(),
            &resolve,
        );
        assert!(matches!(outcome, Err(LlmError::MissingKey { .. })));
    }

    #[test]
    fn unknown_provider_name_is_a_config_error() {
        let resolve = |_: &str| None;
        let outcome = provider_from_sections(
            "palm",
            "claude-sonnet-4-5",
            &AnthropicSection::default(),
            &OpenAiCompatSection::default(),
            &resolve,
        );
        assert!(matches!(outcome, Err(LlmError::Config(_))));
    }

    #[tokio::test]
    async fn create_provider_builds_anthropic_with_resolved_key_and_base_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "resolved-key"))
            .and(body_partial_json(json!({
                "model": "claude-sonnet-4-5",
                "max_tokens": DEFAULT_MAX_TOKENS,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "ok" }],
                "model": "claude-sonnet-4-5",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let toml = format!(
            "[providers.anthropic]\napi_key_env = \"TEST_ANTHROPIC_KEY\"\nbase_url = \"{}\"\n",
            server.uri()
        );
        let config = Config::from_toml_str(&toml).unwrap();
        let resolve = |var: &str| (var == "TEST_ANTHROPIC_KEY").then(|| "resolved-key".to_string());

        let provider = config.create_provider_with(&resolve).unwrap();
        let response = provider
            .chat(&ChatRequest::new(vec![Message::new(Role::User, "hola")]))
            .await
            .unwrap();
        assert_eq!(response.text, "ok");
    }

    #[tokio::test]
    async fn create_provider_prefers_openai_compat_model_override() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "hola" } }],
                "model": "qwen2.5-coder",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let toml = format!(
            "[default]\nprovider = \"openai-compat\"\nmodel = \"fallback-model\"\n\n[providers.openai_compat]\nbase_url = \"{}\"\napi_key_env = \"LOCAL_KEY\"\nmodel = \"qwen2.5-coder\"\n",
            server.uri()
        );
        let config = Config::from_toml_str(&toml).unwrap();
        let resolve = |var: &str| (var == "LOCAL_KEY").then(|| "sk".to_string());

        let provider = config.create_provider_with(&resolve).unwrap();
        let response = provider
            .chat(&ChatRequest::new(vec![Message::new(Role::User, "hola")]))
            .await
            .unwrap();
        assert_eq!(response.model, "qwen2.5-coder");
    }
}
