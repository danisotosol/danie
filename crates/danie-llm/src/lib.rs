//! Multi-provider LLM abstraction for the danie CLI.
//!
//! Supports the Anthropic Messages API and any OpenAI-compatible chat
//! completions endpoint (OpenAI, Ollama, LM Studio, vLLM, ...), with a
//! shared retry policy and TOML-based configuration.

pub mod anthropic;
pub mod config;
pub mod doctor;
pub mod error;
pub mod openai_compat;
pub mod provider;
mod retry;

pub use anthropic::AnthropicProvider;
pub use config::{Config, OpenAiCompatSection, ProvidersSection};
pub use doctor::check;
pub use error::LlmError;
pub use openai_compat::OpenAiCompatProvider;
pub use provider::{
    ChatRequest, ChatResponse, LlmProvider, Message, Role, DEFAULT_MAX_TOKENS, DEFAULT_TEMPERATURE,
};
