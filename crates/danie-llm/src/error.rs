use thiserror::Error;

/// Errors produced while talking to an LLM provider.
#[derive(Debug, Error)]
pub enum LlmError {
    /// Transport-level failure (DNS, connect, timeout, body decode).
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// The provider answered with a non-success status code.
    #[error("provider returned status {code}: {body}")]
    Status { code: u16, body: String },

    /// The configured environment variable holding the API key is not set.
    #[error("missing API key: set the `{env_var}` environment variable")]
    MissingKey { env_var: String },

    /// A JSON (de)serialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid or inconsistent configuration.
    #[error("configuration error: {0}")]
    Config(String),
}
