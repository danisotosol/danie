use danie_core::{KnowledgeMap, LearnerProfile};

const PURE_JSON_RULE: &str = "Output ONLY one JSON value matching the schema: no markdown fences, no commentary, no trailing text.";

fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        "(none recorded)".to_string()
    } else {
        items.join("; ")
    }
}

fn optional_text(value: &Option<String>) -> String {
    value
        .clone()
        .unwrap_or_else(|| "(none recorded)".to_string())
}

pub fn profile_digest(profile: &LearnerProfile) -> String {
    format!(
        "Learner profile:\n- Language: {}\n- Solid ground: {}\n- Goals: {}\n- Pace notes: {}\n- Struggles: {}\n- Voice preferences: {}",
        profile.language,
        bullet_list(&profile.solid_ground),
        bullet_list(&profile.goals),
        optional_text(&profile.pace_notes),
        optional_text(&profile.struggle_prefs),
        optional_text(&profile.voice_prefs),
    )
}

pub fn status_digest(map: &KnowledgeMap) -> String {
    if map.strands.is_empty() {
        return "Strand statuses: none yet (fresh goal).".to_string();
    }
    let mut out = String::from("Strand statuses:");
    for strand in &map.strands {
        let evidence = if strand.evidence.is_empty() {
            String::new()
        } else {
            format!(" ({})", strand.evidence)
        };
        out.push_str(&format!("\n- {}: {}{}", strand.name, strand.status, evidence));
    }
    out
}

pub fn probe_system() -> String {
    format!(
        "You are danie, an expert tutor running a short diagnostic placement quiz.\n\
         Rules:\n\
         - Produce 3 to 6 questions; each targets exactly one micro-concept (strand) of the goal.\n\
         - Every question has 3 or 4 plausible options plus a final option that explicitly means \"I don't know\", phrased in the learner's language.\n\
         - Exactly one option is correct; the \"I don't know\" option must NEVER be the correct one.\n\
         - Wrong options should encode common misconceptions.\n\
         - Write every question and every option in the learner's language given below.\n\
         - {PURE_JSON_RULE}\n\
         Schema:\n\
         {{\"questions\":[{{\"strand\":\"short-slug-id\",\"question\":\"...\",\"options\":[\"option\",\"option\",\"option\",\"I don't know\"],\"correct_index\":0}}]}}"
    )
}

pub fn probe_user(goal: &str, profile: &LearnerProfile) -> String {
    format!(
        "Goal: {goal}\n\n{}\n\nDesign the diagnostic quiz now.",
        profile_digest(profile)
    )
}

pub fn plan_system() -> String {
    format!(
        "You are danie, a curriculum planner using the Alvar method.\n\
         Rules:\n\
         - Break the goal into 3 to 8 prerequisite-ordered nodes (micro-topics).\n\
         - Node ids are unique lowercase kebab-case slugs.\n\
         - Edges are [before_id, after_id] pairs declaring prerequisites; keep the graph acyclic and minimal.\n\
         - Calibrate the plan to what the learner already knows, using the statuses provided.\n\
         - Titles and summaries must be written in the learner's language given in the profile.\n\
         - {PURE_JSON_RULE}\n\
         Schema:\n\
         {{\"nodes\":[{{\"id\":\"variables\",\"title\":\"Variables\",\"summary\":\"one line\"}}],\"edges\":[[\"variables\",\"types\"]]}}"
    )
}

pub fn plan_user(goal: &str, map: &KnowledgeMap, profile: &LearnerProfile) -> String {
    format!(
        "Goal: {goal}\n\n{}\n{}\n\nProduce the learning plan now.",
        profile_digest(profile),
        status_digest(map)
    )
}

pub fn teach_system(language: &str) -> String {
    format!(
        "You are danie, a one-to-one tutor applying the Alvar method.\n\
         Method rules:\n\
         - Teach in the learner's language: {language}.\n\
         - ONE reasoning step per turn: introduce exactly one idea, then check it with the quiz.\n\
         - No walls of text: keep body_md under about 120 words.\n\
         - Verify facts; hedge or admit uncertainty rather than inventing.\n\
         - The quiz has exactly 4 options with exactly one correct; distractors target common misconceptions.\n\
         - Write everything (title, body, quiz) in {language}.\n\
         - {PURE_JSON_RULE}\n\
         Schema:\n\
         {{\"title\":\"...\",\"body_md\":\"markdown lesson body\",\"quiz\":{{\"prompt\":\"...\",\"options\":[\"a\",\"b\",\"c\",\"d\"],\"correct_index\":0,\"explanation\":\"why the correct option wins\"}}}}"
    )
}

pub fn teach_user(
    goal: &str,
    node_title: &str,
    node_summary: &str,
    prereq_titles: &[String],
    map: &KnowledgeMap,
    profile: &LearnerProfile,
) -> String {
    let prereqs = if prereq_titles.is_empty() {
        "(none)".to_string()
    } else {
        prereq_titles.join(", ")
    };
    format!(
        "Goal: {goal}\nCurrent node: {node_title}\nNode summary: {node_summary}\nPrerequisites already taught: {prereqs}\n\n{}\n{}\n\nWrite the lesson for the current node now.",
        profile_digest(profile),
        status_digest(map)
    )
}

pub fn prereq_system(language: &str) -> String {
    format!(
        "You are danie, a tutor diagnosing a missing foundation.\n\
         The learner failed the lock-in quiz for the current node. Propose ONE missing prerequisite micro-topic that would unblock them.\n\
         Rules:\n\
         - The id must be a lowercase kebab-case slug that does not appear in the existing ids list.\n\
         - Title and summary written in the learner's language: {language}.\n\
         - {PURE_JSON_RULE}\n\
         Schema:\n\
         {{\"id\":\"new-slug\",\"title\":\"...\",\"summary\":\"one line\"}}"
    )
}

pub fn prereq_user(
    goal: &str,
    current_title: &str,
    current_summary: &str,
    existing_ids: &[String],
    profile: &LearnerProfile,
) -> String {
    let ids = if existing_ids.is_empty() {
        "(none)".to_string()
    } else {
        existing_ids.join(", ")
    };
    format!(
        "Goal: {goal}\nCurrent node: {current_title}\nNode summary: {current_summary}\nExisting node ids: {ids}\n\n{}\n\nPropose the missing prerequisite now.",
        profile_digest(profile)
    )
}

pub fn review_system(language: &str) -> String {
    format!(
        "You are danie, running a spaced-repetition recall check.\n\
         Rules:\n\
         - One multiple-choice question with exactly 4 options and exactly one correct answer.\n\
         - Test genuine recall of the node, not trivia wording.\n\
         - Write everything in the learner's language: {language}.\n\
         - {PURE_JSON_RULE}\n\
         Schema:\n\
         {{\"prompt\":\"...\",\"options\":[\"a\",\"b\",\"c\",\"d\"],\"correct_index\":0,\"explanation\":\"why the correct option wins\"}}"
    )
}

pub fn review_user(node_id: &str, context: Option<&str>, profile: &LearnerProfile) -> String {
    let context = context.unwrap_or(
        "No prior context available; write a general recall question about this node.",
    );
    format!(
        "Node: {node_id}\nContext: {context}\n\n{}\n\nProduce the review question now.",
        profile_digest(profile)
    )
}
