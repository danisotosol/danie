use crate::error::LlmError;
use crate::provider::{ChatRequest, LlmProvider, Message, Role, DEFAULT_TEMPERATURE};

/// Prompt used by the connectivity check; the provider must answer `ok`.
const CHECK_PROMPT: &str = "Reply with exactly: ok";
/// Token budget for the check answer.
const CHECK_MAX_TOKENS: u32 = 16;

/// Verifies that a provider is reachable and answering, returning its
/// (trimmed) reply to a fixed one-word prompt.
pub async fn check(provider: &dyn LlmProvider) -> Result<String, LlmError> {
    let request = ChatRequest {
        messages: vec![Message::new(Role::User, CHECK_PROMPT)],
        max_tokens: CHECK_MAX_TOKENS,
        temperature: DEFAULT_TEMPERATURE,
    };
    let response = provider.chat(&request).await?;
    Ok(response.text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::openai_compat::OpenAiCompatProvider;

    use super::check;

    #[tokio::test]
    async fn check_returns_trimmed_reply_and_sends_bounded_prompt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(json!({
                "messages": [
                    { "role": "user", "content": "Reply with exactly: ok" }
                ],
                "max_tokens": 16,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "  ok\n" } }],
                "model": "test-model",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = OpenAiCompatProvider::new(server.uri(), None, "test-model");
        let reply = check(&provider).await.unwrap();

        assert_eq!(reply, "ok");
    }
}
