use std::time::Duration;

use serde_json::{json, Value};

use crate::error::LlmError;
use crate::provider::{ChatRequest, ChatResponse, LlmProvider, Role};
use crate::retry::send_with_retry;

/// Default base URL for the Anthropic API.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

const ANTHROPIC_VERSION: &str = "2023-06-01";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

fn build_client_with(connect_timeout: Duration, request_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .build()
        .expect("valid HTTP client configuration")
}

fn build_client() -> reqwest::Client {
    build_client_with(CONNECT_TIMEOUT, REQUEST_TIMEOUT)
}

/// Provider for the Anthropic Messages API (`/v1/messages`).
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Creates a provider targeting the public Anthropic API.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: build_client(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Overrides the base URL (for proxies or gateways).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

fn split_system(messages: &[crate::provider::Message]) -> (Option<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    let mut chat = Vec::new();
    for message in messages {
        match message.role {
            Role::System => system_parts.push(message.content.clone()),
            Role::User | Role::Assistant => chat.push(json!({
                "role": message.role,
                "content": message.content,
            })),
        }
    }
    let system = (!system_parts.is_empty()).then(|| system_parts.join("\n\n"));
    (system, chat)
}

fn request_body(provider_model: &str, req: &ChatRequest) -> Value {
    let (system, messages) = split_system(&req.messages);
    let mut body = json!({
        "model": provider_model,
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "messages": messages,
    });
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    body
}

fn extract_text(payload: &Value) -> String {
    payload["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block["type"] == "text")
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = request_body(&self.model, req);

        let response = send_with_retry(|| {
            self.client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(&body)
                .send()
        })
        .await?;

        let payload: Value = response.json().await?;
        Ok(ChatResponse {
            text: extract_text(&payload),
            model: payload["model"].as_str().unwrap_or_default().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::error::LlmError;
    use crate::provider::{ChatRequest, Message};

    #[tokio::test]
    async fn sends_expected_request_and_joins_text_blocks() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "secret-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [
                    { "type": "text", "text": "Hola" },
                    { "type": "tool_use", "id": "t1" },
                    { "type": "text", "text": "alumno" },
                ],
                "model": "claude-sonnet-4-5",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider =
            AnthropicProvider::new("secret-key", "claude-sonnet-4-5").with_base_url(server.uri());
        let response = provider
            .chat(&ChatRequest::new(vec![
                Message::new(Role::System, "You are a tutor."),
                Message::new(Role::System, "Be brief."),
                Message::new(Role::User, "Hola"),
            ]))
            .await
            .unwrap();

        assert_eq!(response.text, "Hola\nalumno");
        assert_eq!(response.model, "claude-sonnet-4-5");

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body is JSON");
        assert_eq!(body["model"], "claude-sonnet-4-5");
        assert_eq!(body["max_tokens"], 2048);
        assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(body["system"], "You are a tutor.\n\nBe brief.");
        assert_eq!(
            body["messages"],
            json!([{ "role": "user", "content": "Hola" }])
        );
    }

    #[tokio::test]
    async fn maps_non_success_status_without_retrying() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid request"))
            .expect(1)
            .mount(&server)
            .await;

        let provider = AnthropicProvider::new("k", "m").with_base_url(server.uri());
        let err = provider
            .chat(&ChatRequest::new(vec![Message::new(Role::User, "hola")]))
            .await
            .unwrap_err();

        match err {
            LlmError::Status { code, body } => {
                assert_eq!(code, 400);
                assert_eq!(body, "invalid request");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn summarizes_html_error_pages_and_truncates_long_bodies() {
        let server = MockServer::start().await;
        let html = format!("<html><body>{}</body></html>", "x".repeat(600));
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(404).set_body_string(html))
            .expect(1)
            .mount(&server)
            .await;

        let provider = AnthropicProvider::new("k", "m").with_base_url(server.uri());
        let err = provider
            .chat(&ChatRequest::new(vec![Message::new(Role::User, "hola")]))
            .await
            .unwrap_err();

        match err {
            LlmError::Status { code, body } => {
                assert_eq!(code, 404);
                assert_eq!(body, "non-JSON response (HTML page)");
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let long_plain = "y".repeat(500);
        let server2 = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(503).set_body_string(long_plain.clone()))
            .expect(2)
            .mount(&server2)
            .await;
        let provider2 = AnthropicProvider::new("k", "m").with_base_url(server2.uri());
        let err2 = provider2
            .chat(&ChatRequest::new(vec![Message::new(Role::User, "hola")]))
            .await
            .unwrap_err();
        match err2 {
            LlmError::Status { code, body } => {
                assert_eq!(code, 503);
                assert_eq!(body.chars().count(), 201);
                assert!(body.ends_with('…'));
                assert!(long_plain.starts_with(body.trim_end_matches('…')));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn retries_once_on_server_error_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "recuperado" }],
                "model": "m",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = AnthropicProvider::new("k", "m").with_base_url(server.uri());
        let response = provider
            .chat(&ChatRequest::new(vec![Message::new(Role::User, "hola")]))
            .await
            .unwrap();

        assert_eq!(response.text, "recuperado");
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}
