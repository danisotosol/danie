use std::collections::{BTreeMap, HashSet};
use std::fmt;

use danie_core::{
    CoreError, KnowledgeMap, LearnerProfile, PlanGraph, PlanNode, QuizLogEntry, QuizOutcome,
    StrandStatus,
};
use danie_llm::{ChatRequest, LlmError, LlmProvider, Message, Role};
use tracing::warn;

use crate::protocol::{
    extract_json, PlanDto, PrereqDto, ProbeDto, ProbeQuestionDto, QuizDto, TeachLessonDto,
};

#[derive(Debug)]
pub enum EngineError {
    Llm(LlmError),
    Json(String),
    Cycle,
    Core(CoreError),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Llm(error) => write!(f, "model call failed: {error}"),
            EngineError::Json(message) => write!(f, "invalid model payload: {message}"),
            EngineError::Cycle => write!(f, "the proposed plan contains a prerequisite cycle"),
            EngineError::Core(error) => write!(f, "domain error: {error}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<LlmError> for EngineError {
    fn from(value: LlmError) -> Self {
        EngineError::Llm(value)
    }
}

impl From<CoreError> for EngineError {
    fn from(value: CoreError) -> Self {
        EngineError::Core(value)
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

const REQUEST_TEMPERATURE: f32 = 0.4;

async fn chat_json<T, F>(
    provider: &dyn LlmProvider,
    system: String,
    user: String,
    parse: F,
) -> Result<T>
where
    F: Fn(&str) -> std::result::Result<T, String>,
{
    let mut messages = vec![
        Message::new(Role::System, system),
        Message::new(Role::User, user),
    ];
    let mut request = ChatRequest::new(messages.clone());
    request.temperature = REQUEST_TEMPERATURE;
    let response = provider.chat(&request).await?;
    match parse(extract_json(&response.text)) {
        Ok(value) => Ok(value),
        Err(first_error) => {
            warn!(error = %first_error, raw = %response.text, "model returned invalid JSON; retrying once");
            messages.push(Message::new(Role::Assistant, response.text));
            messages.push(Message::new(
                Role::User,
                format!(
                    "That was not valid JSON for the schema: {first_error}. Reply again with ONLY valid JSON."
                ),
            ));
            let retry = provider.chat(&ChatRequest::new(messages)).await?;
            parse(extract_json(&retry.text)).map_err(|error| {
                EngineError::Json(format!("{error}; last raw reply: {}", clip(&retry.text)))
            })
        }
    }
}

fn clip(text: &str) -> String {
    let limit = 200;
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(limit)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    format!("{}...", &text[..cut])
}

pub async fn generate_probe(
    provider: &dyn LlmProvider,
    goal: &str,
    profile: &LearnerProfile,
) -> Result<Vec<ProbeQuestionDto>> {
    chat_json(
        provider,
        crate::prompts::probe_system(),
        crate::prompts::probe_user(goal, profile),
        |text| {
            let dto: ProbeDto = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let usable: Vec<ProbeQuestionDto> = dto
                .questions
                .into_iter()
                .filter(|question| question.validate())
                .collect();
            if usable.is_empty() {
                Err("no usable probe questions".to_string())
            } else {
                Ok(usable)
            }
        },
    )
    .await
}

pub fn score_probe(
    map: &mut KnowledgeMap,
    question: &ProbeQuestionDto,
    chosen: Option<usize>,
) -> QuizOutcome {
    let outcome = match chosen {
        Some(index) if index == question.correct_index => QuizOutcome::Correct,
        Some(_) => QuizOutcome::Wrong,
        None => QuizOutcome::Idk,
    };
    let status = match outcome {
        QuizOutcome::Correct => StrandStatus::Known,
        QuizOutcome::Wrong => StrandStatus::Edge,
        QuizOutcome::Idk | QuizOutcome::NearMiss => StrandStatus::Unknown,
    };
    let evidence = match (outcome, chosen) {
        (QuizOutcome::Wrong, Some(index)) => format!(
            "diagnostic probe: chose {:?}, expected {:?}",
            option_text(question, Some(index)),
            option_text(question, Some(question.correct_index)),
        ),
        (QuizOutcome::Idk, _) | (QuizOutcome::NearMiss, _) => {
            "diagnostic probe: answered \"I don't know\"".to_string()
        }
        _ => "diagnostic probe: answered correctly".to_string(),
    };
    map.upsert_strand(question.strand.trim().to_string(), status, evidence);
    let answer = option_text(question, chosen).unwrap_or_else(|| "(no answer)".to_string());
    map.log_quiz(QuizLogEntry {
        strand: question.strand.clone(),
        answer,
        outcome,
    });
    outcome
}

fn option_text(question: &ProbeQuestionDto, index: Option<usize>) -> Option<String> {
    question.options.get(index?).cloned()
}

pub struct PlanBundle {
    pub graph: PlanGraph,
    pub nodes: BTreeMap<String, PlanNode>,
    pub edges: Vec<(String, String)>,
}

pub fn normalize_id(id: &str) -> String {
    id.trim().to_lowercase()
}

pub fn build_plan(dto: &PlanDto) -> Result<PlanBundle> {
    let mut graph = PlanGraph::new();
    let mut nodes: BTreeMap<String, PlanNode> = BTreeMap::new();
    let mut seen = HashSet::new();
    for node in &dto.nodes {
        let id = normalize_id(&node.id);
        if id.is_empty() || !seen.insert(id.clone()) {
            warn!(id = %id, "skipping duplicate or empty plan node id");
            continue;
        }
        let plan_node = PlanNode {
            id: id.clone(),
            title: node.title.trim().to_string(),
            summary: node.summary.trim().to_string(),
        };
        graph.add_node(plan_node.clone())?;
        nodes.insert(id, plan_node);
    }
    if nodes.is_empty() {
        return Err(EngineError::Json("plan contained no usable nodes".into()));
    }
    let mut edges = Vec::new();
    for (before, after) in &dto.edges {
        let before = normalize_id(before);
        let after = normalize_id(after);
        if before == after || !nodes.contains_key(&before) || !nodes.contains_key(&after) {
            warn!(before = %before, after = %after, "dropping unusable plan edge");
            continue;
        }
        match graph.add_prereq(&before, &after) {
            Ok(()) => edges.push((before, after)),
            Err(CoreError::Cycle) => return Err(EngineError::Cycle),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(PlanBundle {
        graph,
        nodes,
        edges,
    })
}

pub async fn generate_plan(
    provider: &dyn LlmProvider,
    goal: &str,
    map: &KnowledgeMap,
    profile: &LearnerProfile,
) -> Result<PlanBundle> {
    let dto = chat_json(
        provider,
        crate::prompts::plan_system(),
        crate::prompts::plan_user(goal, map, profile),
        |text| {
            let dto: PlanDto = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if dto.nodes.is_empty() {
                return Err("plan had no nodes".to_string());
            }
            Ok(dto)
        },
    )
    .await?;
    build_plan(&dto)
}

pub async fn generate_lesson(
    provider: &dyn LlmProvider,
    goal: &str,
    node: &PlanNode,
    prereq_titles: &[String],
    map: &KnowledgeMap,
    profile: &LearnerProfile,
) -> Result<TeachLessonDto> {
    chat_json(
        provider,
        crate::prompts::teach_system(&profile.language),
        crate::prompts::teach_user(
            goal,
            &node.title,
            &node.summary,
            prereq_titles,
            map,
            profile,
        ),
        |text| {
            let lesson: TeachLessonDto = serde_json::from_str(text).map_err(|e| e.to_string())?;
            lesson.validate()?;
            Ok(lesson)
        },
    )
    .await
}

pub async fn propose_prerequisite(
    provider: &dyn LlmProvider,
    goal: &str,
    current: &PlanNode,
    existing_ids: &[String],
    profile: &LearnerProfile,
) -> Result<PrereqDto> {
    chat_json(
        provider,
        crate::prompts::prereq_system(&profile.language),
        crate::prompts::prereq_user(
            goal,
            &current.title,
            &current.summary,
            existing_ids,
            profile,
        ),
        |text| {
            let dto: PrereqDto = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if dto.id.trim().is_empty() || dto.title.trim().is_empty() {
                return Err("prerequisite proposal missing id or title".to_string());
            }
            Ok(dto)
        },
    )
    .await
}

pub async fn generate_review_question(
    provider: &dyn LlmProvider,
    node_id: &str,
    context: Option<&str>,
    profile: &LearnerProfile,
) -> Result<QuizDto> {
    chat_json(
        provider,
        crate::prompts::review_system(&profile.language),
        crate::prompts::review_user(node_id, context, profile),
        |text| {
            let quiz: QuizDto = serde_json::from_str(text).map_err(|e| e.to_string())?;
            quiz.validate()?;
            Ok(quiz)
        },
    )
    .await
}

pub const QUALITY_LABELS: [&str; 5] = ["Again", "Hard", "Good", "Easy", "Perfect"];

pub fn quality_value(choice: usize) -> u8 {
    (choice + 1).clamp(1, 5) as u8
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use async_trait::async_trait;
    use danie_llm::{ChatResponse, DEFAULT_MAX_TOKENS};

    use crate::protocol::PlanNodeDto;

    use super::*;

    struct MockProvider {
        replies: Mutex<VecDeque<std::result::Result<ChatResponse, LlmError>>>,
        calls: AtomicUsize,
    }

    impl MockProvider {
        fn ok(replies: &[&str]) -> Self {
            Self {
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
            }
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

    fn probe_question(strand: &str, correct_index: usize) -> ProbeQuestionDto {
        ProbeQuestionDto {
            strand: strand.to_string(),
            question: "Which one stores a value?".to_string(),
            options: vec![
                "a box".to_string(),
                "a shoe".to_string(),
                "I don't know".to_string(),
            ],
            correct_index,
        }
    }

    #[test]
    fn probe_scoring_maps_answers_to_statuses_and_logs_outcomes() {
        let mut map = KnowledgeMap::new("Rust basics");

        assert_eq!(
            score_probe(&mut map, &probe_question("variables", 0), Some(0)),
            QuizOutcome::Correct
        );
        assert_eq!(
            score_probe(&mut map, &probe_question("ownership", 1), Some(0)),
            QuizOutcome::Wrong
        );
        assert_eq!(
            score_probe(&mut map, &probe_question("loops", 0), None),
            QuizOutcome::Idk
        );

        assert_eq!(map.strands_with(StrandStatus::Known).len(), 1);
        assert_eq!(map.strands_with(StrandStatus::Edge).len(), 1);
        assert_eq!(map.strands_with(StrandStatus::Unknown).len(), 1);
        assert_eq!(map.quiz_log.len(), 3);
        assert_eq!(map.quiz_log[0].outcome, QuizOutcome::Correct);
        assert_eq!(map.quiz_log[1].outcome, QuizOutcome::Wrong);
        assert_eq!(map.quiz_log[2].outcome, QuizOutcome::Idk);
        assert_eq!(map.quiz_log[0].answer, "a box");
        let edge = &map.strands_with(StrandStatus::Edge)[0];
        assert!(edge.evidence.contains("expected"));
        assert!(!edge.evidence.is_empty());
    }

    #[tokio::test]
    async fn lesson_parsing_retries_once_then_succeeds() {
        let bad =
            "Sure! Here is your lesson:\n{\"title\":\"Variables\",\"body_md\":\"short body\"}";
        let good = "{\"title\":\"Variables\",\"body_md\":\"body text\",\"quiz\":{\"prompt\":\"pick\",\"options\":[\"a\",\"b\",\"c\",\"d\"],\"correct_index\":2,\"explanation\":\"c wins\"}}";
        let mock = MockProvider::ok(&[bad, good]);
        let node = PlanNode {
            id: "variables".into(),
            title: "Variables".into(),
            summary: "boxes for values".into(),
        };
        let map = KnowledgeMap::new("Rust basics");
        let profile = LearnerProfile::default();

        let lesson = generate_lesson(&mock, "Rust basics", &node, &[], &map, &profile)
            .await
            .unwrap();

        assert_eq!(lesson.title, "Variables");
        assert_eq!(lesson.quiz.correct_index, 2);
        assert_eq!(mock.calls(), 2);
    }

    #[tokio::test]
    async fn invalid_payload_twice_surfaces_clean_error_after_two_calls() {
        let mock = MockProvider::ok(&["not json at all", "still not json"]);
        let profile = LearnerProfile::default();

        let error = generate_review_question(&mock, "recursion", None, &profile)
            .await
            .unwrap_err();

        assert!(matches!(error, EngineError::Json(_)));
        assert_eq!(mock.calls(), 2);
        let message = error.to_string();
        assert!(message.contains("invalid model payload"));
    }

    #[test]
    fn quality_labels_map_to_sm2_scale_in_order() {
        let values: Vec<u8> = (0..QUALITY_LABELS.len()).map(quality_value).collect();
        assert_eq!(values, [1, 2, 3, 4, 5]);
        assert_eq!(QUALITY_LABELS.first(), Some(&"Again"));
        assert_eq!(QUALITY_LABELS.last(), Some(&"Perfect"));
    }

    #[test]
    fn plan_dto_builds_graph_with_expected_unlock_sequence() {
        let dto = PlanDto {
            nodes: vec![
                PlanNodeDto {
                    id: "Types".into(),
                    title: "Types".into(),
                    summary: "kinds of values".into(),
                },
                PlanNodeDto {
                    id: "functions".into(),
                    title: "Functions".into(),
                    summary: "reusable steps".into(),
                },
                PlanNodeDto {
                    id: "Variables".into(),
                    title: "Variables".into(),
                    summary: "named boxes".into(),
                },
            ],
            edges: vec![
                ("variables".into(), "Functions".into()),
                ("Functions".into(), "types".into()),
                ("ghost".into(), "types".into()),
            ],
        };

        let bundle = build_plan(&dto).unwrap();
        assert_eq!(bundle.graph.node_count(), 3);
        assert_eq!(bundle.edges.len(), 2);
        assert_eq!(
            bundle.nodes.keys().cloned().collect::<Vec<_>>(),
            vec!["functions", "types", "variables"]
        );

        let none_known: HashSet<String> = HashSet::new();
        assert_eq!(
            bundle.graph.next_unlocked(&none_known).unwrap().id,
            "variables"
        );
        let one_known: HashSet<String> = ["variables"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            bundle.graph.next_unlocked(&one_known).unwrap().id,
            "functions"
        );
        let two_known: HashSet<String> = ["variables", "functions"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(bundle.graph.next_unlocked(&two_known).unwrap().id, "types");
        let all_known: HashSet<String> = ["variables", "functions", "types"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(bundle.graph.next_unlocked(&all_known).is_none());
    }

    #[test]
    fn cycle_edge_is_reported_as_cycle_error() {
        let dto = PlanDto {
            nodes: vec![
                PlanNodeDto {
                    id: "a".into(),
                    title: "A".into(),
                    summary: String::new(),
                },
                PlanNodeDto {
                    id: "b".into(),
                    title: "B".into(),
                    summary: String::new(),
                },
            ],
            edges: vec![("a".into(), "b".into()), ("b".into(), "a".into())],
        };
        assert!(matches!(build_plan(&dto), Err(EngineError::Cycle)));
    }

    #[tokio::test]
    async fn probe_generation_parses_mock_payload_and_drops_bad_rows() {
        let payload = "{\"questions\":[{\"strand\":\"vars\",\"question\":\"q?\",\"options\":[\"a\",\"b\",\"I don't know\"],\"correct_index\":0},{\"strand\":\"broken\",\"question\":\"x\",\"options\":[\"only\"],\"correct_index\":5}]}";
        let mock = MockProvider::ok(&[payload]);

        let questions = generate_probe(&mock, "Rust basics", &LearnerProfile::default())
            .await
            .unwrap();

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].strand, "vars");
        assert_eq!(questions[0].options.len(), 3);
        assert_eq!(mock.calls(), 1);
    }

    #[test]
    fn default_request_settings_are_used_for_engine_calls() {
        let request = ChatRequest::new(vec![]);
        assert_eq!(request.max_tokens, DEFAULT_MAX_TOKENS);
        assert_eq!(REQUEST_TEMPERATURE, 0.4);
    }
}
