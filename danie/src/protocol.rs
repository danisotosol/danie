use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeQuestionDto {
    pub strand: String,
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: usize,
}

#[derive(Debug, Deserialize)]
pub struct ProbeDto {
    pub questions: Vec<ProbeQuestionDto>,
}

impl ProbeQuestionDto {
    pub fn validate(&self) -> bool {
        !self.strand.trim().is_empty()
            && !self.question.trim().is_empty()
            && self.options.len() >= 2
            && self.correct_index < self.options.len()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanNodeDto {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct PlanDto {
    pub nodes: Vec<PlanNodeDto>,
    #[serde(default)]
    pub edges: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuizDto {
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
}

impl QuizDto {
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.trim().is_empty() {
            return Err("empty quiz prompt".into());
        }
        if self.options.len() < 2 {
            return Err("quiz needs at least two options".into());
        }
        if self.correct_index >= self.options.len() {
            return Err(format!(
                "correct_index {} out of range for {} options",
                self.correct_index,
                self.options.len()
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeachLessonDto {
    pub title: String,
    pub body_md: String,
    pub quiz: QuizDto,
}

impl TeachLessonDto {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("empty lesson title".into());
        }
        if self.body_md.trim().is_empty() {
            return Err("empty lesson body".into());
        }
        self.quiz.validate()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrereqDto {
    pub id: String,
    pub title: String,
    pub summary: String,
}

pub fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    let start = match trimmed.bytes().position(|b| b == b'{' || b == b'[') {
        Some(i) => i,
        None => return trimmed,
    };
    let close = if trimmed.as_bytes()[start] == b'{' { '}' } else { ']' };
    match trimmed.rfind(close) {
        Some(end) if end > start => &trimmed[start..=end],
        _ => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_passes_clean_payload_through() {
        assert_eq!(extract_json(" {\"a\": 1} "), "{\"a\": 1}");
    }

    #[test]
    fn extract_json_strips_fenced_block() {
        let text = "```json\n{\"questions\": []}\n```\n";
        assert_eq!(extract_json(text), "{\"questions\": []}");
    }

    #[test]
    fn extract_json_strips_surrounding_prose() {
        let text = "Here you go:\n[{\"id\": \"x\"}]\nHope that helps!";
        assert_eq!(extract_json(text), "[{\"id\": \"x\"}]");
    }

    #[test]
    fn extract_json_leaves_garbage_for_parser_to_reject() {
        let garbage = extract_json("I cannot answer that right now.");
        assert!(serde_json::from_str::<ProbeDto>(garbage).is_err());
    }

    #[test]
    fn probe_and_quiz_dtos_validate_bounds() {
        let q = ProbeQuestionDto {
            strand: "vars".into(),
            question: "?".into(),
            options: vec!["a".into(), "b".into()],
            correct_index: 0,
        };
        assert!(q.validate());
        assert!(!ProbeQuestionDto { correct_index: 2, ..q }.validate());

        let quiz = QuizDto {
            prompt: "p".into(),
            options: vec!["a".into(), "b".into(), "c".into()],
            correct_index: 1,
            explanation: "e".into(),
        };
        assert!(quiz.validate().is_ok());
        assert!(QuizDto { correct_index: 9, ..quiz }.validate().is_err());
    }
}
