//! Route table and JSON handlers.

use axum::extract::{rejection::JsonRejection, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use danie_core::{KnowledgeMap, PlanNode, Strand};
use danie_engine::{self as engine, EngineError};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::dto::{
    LessonRequest, PlanRequest, PrereqRequest, ProbeRequest, ProfileDto, ReviewQuestionRequest,
    StrandInputDto,
};
use crate::error::{bad_request, unprocessable, EngineApiError, ErrorResponse};
use crate::ApiState;

type HandlerResult = Result<Json<Value>, ErrorResponse>;

/// Builds the application router: permissive CORS plus the JSON endpoints.
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/probe", post(probe))
        .route("/v1/plan", post(plan))
        .route("/v1/lesson", post(lesson))
        .route("/v1/prereq", post(prereq))
        .route("/v1/review-question", post(review_question))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

fn decode<T>(payload: Result<Json<T>, JsonRejection>) -> Result<T, ErrorResponse> {
    payload.map(|Json(value)| value).map_err(|rejection| {
        tracing::debug!(error = %rejection, "rejecting malformed request body");
        bad_request(rejection.body_text())
    })
}

fn engine_error(error: EngineError) -> ErrorResponse {
    ErrorResponse(Box::new(EngineApiError(error).into_response()))
}

fn strands_from_inputs(inputs: Vec<StrandInputDto>) -> Result<Vec<Strand>, ErrorResponse> {
    inputs
        .into_iter()
        .map(StrandInputDto::into_strand)
        .collect::<danie_core::Result<Vec<_>>>()
        .map_err(unprocessable)
}

async fn probe(
    State(state): State<ApiState>,
    payload: Result<Json<ProbeRequest>, JsonRejection>,
) -> HandlerResult {
    let request = decode(payload)?;
    let profile = profile_of(request.profile);
    let questions = engine::generate_probe(state.provider.as_ref(), &request.goal, &profile)
        .await
        .map_err(engine_error)?;
    Ok(Json(json!({ "questions": questions })))
}

async fn plan(
    State(state): State<ApiState>,
    payload: Result<Json<PlanRequest>, JsonRejection>,
) -> HandlerResult {
    let request = decode(payload)?;
    let strands = strands_from_inputs(request.strands)?;
    let map = map_for(&request.goal, strands);
    let profile = profile_of(request.profile);
    let bundle = engine::generate_plan(state.provider.as_ref(), &request.goal, &map, &profile)
        .await
        .map_err(engine_error)?;
    let nodes: Vec<&PlanNode> = bundle.nodes.values().collect();
    let edges: Vec<[String; 2]> = bundle
        .edges
        .iter()
        .map(|(before, after)| [before.clone(), after.clone()])
        .collect();
    let mermaid = bundle.graph.to_mermaid();
    Ok(Json(
        json!({ "nodes": nodes, "edges": edges, "mermaid": mermaid }),
    ))
}

async fn lesson(
    State(state): State<ApiState>,
    payload: Result<Json<LessonRequest>, JsonRejection>,
) -> HandlerResult {
    let request = decode(payload)?;
    let strands = strands_from_inputs(request.strands)?;
    let map = map_for(&request.goal, strands);
    let node = PlanNode::from(request.node);
    let profile = profile_of(request.profile);
    let lesson = engine::generate_lesson(
        state.provider.as_ref(),
        &request.goal,
        &node,
        &request.prereq_titles,
        &map,
        &profile,
    )
    .await
    .map_err(engine_error)?;
    to_json(serde_json::to_value(lesson))
}

async fn prereq(
    State(state): State<ApiState>,
    payload: Result<Json<PrereqRequest>, JsonRejection>,
) -> HandlerResult {
    let request = decode(payload)?;
    let current = PlanNode::from(request.current);
    let profile = profile_of(request.profile);
    let proposal = engine::propose_prerequisite(
        state.provider.as_ref(),
        &request.goal,
        &current,
        &request.existing_ids,
        &profile,
    )
    .await
    .map_err(engine_error)?;
    to_json(serde_json::to_value(proposal))
}

async fn review_question(
    State(state): State<ApiState>,
    payload: Result<Json<ReviewQuestionRequest>, JsonRejection>,
) -> HandlerResult {
    let request = decode(payload)?;
    let profile = profile_of(request.profile);
    let quiz = engine::generate_review_question(
        state.provider.as_ref(),
        &request.node_id,
        request.context.as_deref(),
        &profile,
    )
    .await
    .map_err(engine_error)?;
    to_json(serde_json::to_value(quiz))
}

fn profile_of(profile: Option<ProfileDto>) -> danie_core::LearnerProfile {
    profile.map(ProfileDto::into_profile).unwrap_or_default()
}

fn map_for(goal: &str, strands: Vec<Strand>) -> KnowledgeMap {
    KnowledgeMap {
        goal: goal.to_string(),
        updated: chrono::Utc::now(),
        strands,
        quiz_log: Vec::new(),
    }
}

fn to_json(value: Result<Value, serde_json::Error>) -> HandlerResult {
    match value {
        Ok(value) => Ok(Json(value)),
        Err(error) => Err(unprocessable(format!(
            "response serialization failed: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{header::CONTENT_TYPE, Method, Request, StatusCode};
    use axum::Router;
    use danie_llm::{ChatRequest, ChatResponse, LlmError, LlmProvider};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::ApiState;

    const PROBE_PAYLOAD: &str = "{\"questions\":[{\"strand\":\"vars\",\"question\":\"q?\",\"options\":[\"a\",\"b\",\"I don't know\"],\"correct_index\":0}]}";
    const PLAN_PAYLOAD: &str = "{\"nodes\":[{\"id\":\"Types\",\"title\":\"Types\",\"summary\":\"kinds of values\"},{\"id\":\"functions\",\"title\":\"Functions\",\"summary\":\"reusable steps\"},{\"id\":\"Variables\",\"title\":\"Variables\",\"summary\":\"named boxes\"}],\"edges\":[[\"variables\",\"Functions\"],[\"Functions\",\"types\"],[\"ghost\",\"types\"]]}";
    const LESSON_GOOD: &str = "{\"title\":\"Variables\",\"body_md\":\"body text\",\"quiz\":{\"prompt\":\"pick\",\"options\":[\"a\",\"b\",\"c\",\"d\"],\"correct_index\":2,\"explanation\":\"c wins\"}}";
    const PREREQ_PAYLOAD: &str =
        "{\"id\":\"loops-basics\",\"title\":\"Loop Basics\",\"summary\":\"for and while\"}";
    const QUIZ_PAYLOAD: &str = "{\"prompt\":\"pick\",\"options\":[\"a\",\"b\",\"c\",\"d\"],\"correct_index\":2,\"explanation\":\"c wins\"}";

    struct MockProvider {
        replies: Mutex<VecDeque<std::result::Result<ChatResponse, LlmError>>>,
        calls: AtomicUsize,
    }

    impl MockProvider {
        fn ok(replies: &[&str]) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(
                    replies
                        .iter()
                        .map(|text| {
                            Ok(ChatResponse {
                                text: (*text).to_string(),
                                model: "mock-model".to_string(),
                            })
                        })
                        .collect(),
                ),
                calls: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn chat(&self, _req: &ChatRequest) -> std::result::Result<ChatResponse, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("mock ran out of scripted replies")
        }
    }

    fn app_with(mock: &Arc<MockProvider>) -> Router {
        router(ApiState::new(Arc::clone(mock) as Arc<dyn LlmProvider>))
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    fn post_json(uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn call(app: Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn health_reports_ok_and_crate_version() {
        let app = app_with(&MockProvider::ok(&[]));

        let (status, body) = call(app, get("/health")).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn probe_returns_questions_from_provider_payload() {
        let mock = MockProvider::ok(&[PROBE_PAYLOAD]);
        let payload = r#"{"goal":"Rust basics","profile":{"language":"es","goals":["learn rust"],"solid_ground":["algebra"]}}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/probe", payload)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["questions"].as_array().map(Vec::len), Some(1));
        assert_eq!(body["questions"][0]["strand"], "vars");
        assert_eq!(body["questions"][0]["correct_index"], 0);
        assert_eq!(mock.calls(), 1);
    }

    #[tokio::test]
    async fn plan_returns_nodes_edges_and_mermaid() {
        let mock = MockProvider::ok(&[PLAN_PAYLOAD]);
        let payload = r#"{"goal":"Rust basics","strands":[{"name":"variables","status":"known","evidence":"aced the probe"}]}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/plan", payload)).await;

        assert_eq!(status, StatusCode::OK);
        let nodes = body["nodes"].as_array().unwrap();
        let ids: Vec<&str> = nodes.iter().filter_map(|n| n["id"].as_str()).collect();
        assert_eq!(ids, vec!["functions", "types", "variables"]);
        let edges = body["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0][0], "variables");
        assert_eq!(edges[0][1], "functions");
        let mermaid = body["mermaid"].as_str().unwrap();
        assert!(mermaid.starts_with("flowchart TD"));
        assert!(mermaid.contains("variables --> functions"));
    }

    #[tokio::test]
    async fn plan_rejects_unknown_strand_status_with_422() {
        let mock = MockProvider::ok(&[]);
        let payload = r#"{"goal":"Rust basics","strands":[{"name":"vars","status":"flotante","evidence":""}]}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/plan", payload)).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"].is_string());
        assert_eq!(mock.calls(), 0);
    }

    #[tokio::test]
    async fn lesson_returns_dto_verbatim() {
        let mock = MockProvider::ok(&[LESSON_GOOD]);
        let payload = r#"{"goal":"Rust basics","node":{"id":"variables","title":"Variables","summary":"named boxes"},"prereq_titles":["Functions"],"strands":[{"name":"variables","status":"edge","evidence":"probe"}],"profile":{"pace_notes":"short sessions"}}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/lesson", payload)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["title"], "Variables");
        assert_eq!(body["body_md"], "body text");
        assert_eq!(body["quiz"]["correct_index"], 2);
        assert_eq!(body["quiz"]["options"][3], "d");
        assert_eq!(mock.calls(), 1);
    }

    #[tokio::test]
    async fn lesson_garbage_payload_maps_to_422_after_exactly_two_calls() {
        let mock = MockProvider::ok(&["not json at all", "still not json"]);
        let payload = r#"{"goal":"Rust basics","node":{"id":"variables","title":"Variables","summary":"named boxes"},"prereq_titles":[],"strands":[]}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/lesson", payload)).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = body["error"].as_str().unwrap();
        assert!(error.contains("invalid model payload"));
        assert_eq!(mock.calls(), 2);
    }

    #[tokio::test]
    async fn prereq_returns_proposal_verbatim() {
        let mock = MockProvider::ok(&[PREREQ_PAYLOAD]);
        let payload = r#"{"goal":"Rust basics","current":{"id":"variables","title":"Variables","summary":"named boxes"},"existing_ids":["variables"],"profile":null}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/prereq", payload)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "loops-basics");
        assert_eq!(body["title"], "Loop Basics");
        assert_eq!(body["summary"], "for and while");
        assert_eq!(mock.calls(), 1);
    }

    #[tokio::test]
    async fn review_question_returns_quiz_verbatim() {
        let mock = MockProvider::ok(&[QUIZ_PAYLOAD]);
        let payload = r#"{"node_id":"recursion","context":"failed lock-in quiz","profile":null}"#;

        let (status, body) = call(app_with(&mock), post_json("/v1/review-question", payload)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["prompt"], "pick");
        assert_eq!(body["correct_index"], 2);
        assert_eq!(body["explanation"], "c wins");
        assert_eq!(mock.calls(), 1);
    }

    #[tokio::test]
    async fn missing_required_field_yields_json_rejection_not_a_panic() {
        let mock = MockProvider::ok(&[]);

        let (status, body) = call(app_with(&mock), post_json("/v1/probe", "{}")).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let error = body["error"].as_str().unwrap().to_string();
        assert!(error.contains("goal"));
    }

    #[tokio::test]
    async fn malformed_body_yields_json_error_not_a_panic() {
        let mock = MockProvider::ok(&[]);

        let (status, body) = call(
            app_with(&mock),
            post_json("/v1/probe", "definitely { not json"),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string());
        assert_eq!(mock.calls(), 0);
    }
}
