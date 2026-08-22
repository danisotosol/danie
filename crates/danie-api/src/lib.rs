//! danie-api: stateless HTTP facade over the danie tutor engine.
//!
//! Every request carries the full learning context (goal, strand statuses,
//! learner profile); the server keeps no `.danie/` storage. Endpoints map one
//! to one onto [`danie_engine`] calls and translate [`danie_engine::EngineError`]
//! onto HTTP status codes with a uniform `{"error": "..."}` JSON body.

pub mod dto;
pub mod error;
pub mod routes;

use std::sync::Arc;

use danie_llm::{Config, LlmProvider};

pub use dto::ProfileDto;
pub use error::{bad_request, unprocessable, EngineApiError};
pub use routes::router;

/// Default TCP port served by [`bootstrap`] when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 8787;

/// Shared handler state handed to every request.
#[derive(Clone)]
pub struct ApiState {
    pub provider: Arc<dyn LlmProvider>,
}

impl ApiState {
    /// Creates state around a shared provider.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }
}

/// Loads provider configuration, binds the listener and serves the router
/// until shutdown. The port comes from the `PORT` environment variable and
/// falls back to 8787 on `0.0.0.0`.
pub async fn bootstrap() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "danie_api=info,danie_engine=info".into()),
        )
        .init();

    let config = Config::load()?;
    let provider = config.create_provider()?;
    let state = ApiState::new(Arc::from(provider));

    let port = std::env::var("PORT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "danie-api listening");
    println!("danie-api listening on http://{addr}");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
