use serde_json::{json, Value};

use crate::error::LlmError;
use crate::provider::{ChatRequest, ChatResponse, LlmProvider};
use crate::retry::send_with_retry;

/// Provider for any OpenAI-compatible chat completions endpoint
/// (OpenAI, Ollama, LM Studio, vLLM, ...).
#[derive(Debug, Clone)]
pub struct OpenAiCompatProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiCompatProvider {
    /// Creates a provider for `base_url` (e.g. `https://api.openai.com/v1`
    /// or `http://localhost:11434/v1`). When `api_key` is `None` no
    /// Authorization header is sent.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "messages": req.messages.iter().map(|m| json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
            "max_tokens": req.max_tokens,
            // An f32 like 0.7 round-trips through JSON as
            // 0.699999988079071, which strict upstream validators reject.
            // Round in f64 so the wire value is clean.
            "temperature": serde_json::Number::from_f64(
                (req.temperature as f64 * 100.0).round() / 100.0,
            ),
        });

        let response = send_with_retry(|| {
            let mut request = self.client.post(&url).json(&body);
            if let Some(api_key) = &self.api_key {
                request = request.bearer_auth(api_key);
            }
            request.send()
        })
        .await?;

        let payload: Value = response.json().await?;
        Ok(ChatResponse {
            text: payload["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            model: payload["model"].as_str().unwrap_or_default().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::*;
    use crate::error::LlmError;
    use crate::provider::{ChatRequest, Message, Role};

    async fn received(server: &MockServer) -> Vec<Request> {
        server.received_requests().await.unwrap()
    }

    fn user_request() -> ChatRequest {
        ChatRequest::new(vec![
            Message::new(Role::System, "s"),
            Message::new(Role::User, "u"),
        ])
    }

    #[tokio::test]
    async fn sends_bearer_header_when_key_is_set_and_parses_choices() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer sk-local"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "Hello!" } }],
                "model": "llama3",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider =
            OpenAiCompatProvider::new(server.uri(), Some("sk-local".to_string()), "llama3");
        let response = provider.chat(&user_request()).await.unwrap();

        assert_eq!(response.text, "Hello!");
        assert_eq!(response.model, "llama3");

        let requests = received(&server).await;
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body is JSON");
        assert_eq!(body["model"], "llama3");
        assert_eq!(body["max_tokens"], 2048);
        assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(
            body["messages"],
            json!([
                { "role": "system", "content": "s" },
                { "role": "user", "content": "u" },
            ])
        );
    }

    #[tokio::test]
    async fn omits_authorization_header_without_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "ok" } }],
                "model": "m",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = OpenAiCompatProvider::new(server.uri(), None, "m");
        provider.chat(&user_request()).await.unwrap();

        let requests = received(&server).await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].headers.get("authorization").is_none());
    }

    #[tokio::test]
    async fn retries_once_on_server_error_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "recuperado" } }],
                "model": "m",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = OpenAiCompatProvider::new(server.uri(), None, "m");
        let response = provider.chat(&user_request()).await.unwrap();

        assert_eq!(response.text, "recuperado");
        assert_eq!(received(&server).await.len(), 2);
    }

    #[tokio::test]
    async fn retries_once_on_rate_limit_then_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .expect(2)
            .mount(&server)
            .await;

        let provider = OpenAiCompatProvider::new(server.uri(), None, "m");
        let err = provider.chat(&user_request()).await.unwrap_err();

        match err {
            LlmError::Status { code, body } => {
                assert_eq!(code, 429);
                assert_eq!(body, "slow down");
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(received(&server).await.len(), 2);
    }
}
