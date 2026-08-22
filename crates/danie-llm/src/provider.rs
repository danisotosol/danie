use serde::{Deserialize, Serialize};

use crate::error::LlmError;

/// Author of a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A single chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    /// Convenience constructor for a message with owned content.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// A chat completion request.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl ChatRequest {
    /// Creates a request with the default sampling settings
    /// (`max_tokens` 2048, `temperature` 0.7).
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
        }
    }
}

/// Default `max_tokens` applied by [`ChatRequest::new`].
pub const DEFAULT_MAX_TOKENS: u32 = 2048;

/// Default `temperature` applied by [`ChatRequest::new`].
pub const DEFAULT_TEMPERATURE: f32 = 0.7;

/// The assistant's answer to a [`ChatRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub text: String,
    pub model: String,
}

/// A transport-agnostic chat provider (Anthropic, OpenAI-compatible, ...).
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, LlmError>;
}
