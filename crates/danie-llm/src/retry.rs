use std::future::Future;
use std::time::Duration;

use crate::error::LlmError;

/// Delay before the single automatic retry.
const RETRY_DELAY: Duration = Duration::from_millis(800);

fn is_retryable(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response, LlmError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let code = status.as_u16();
        let body = response.text().await.unwrap_or_default();
        Err(LlmError::Status { code, body })
    }
}

/// Sends an HTTP request with the shared retry policy.
///
/// On a retryable outcome (HTTP 429/5xx or a transport-level error) the
/// request is retried once after [`RETRY_DELAY`]; if that second attempt
/// also fails, its error is returned.
pub(crate) async fn send_with_retry<F, Fut>(send: F) -> Result<reqwest::Response, LlmError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let result = match send().await {
        Err(_) => {
            tokio::time::sleep(RETRY_DELAY).await;
            send().await
        }
        Ok(response) if is_retryable(response.status()) => {
            tokio::time::sleep(RETRY_DELAY).await;
            send().await
        }
        Ok(response) => Ok(response),
    };
    ensure_success(result?).await
}
