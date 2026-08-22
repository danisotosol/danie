use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProbeQuestionDto {
    pub strand: String,
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: usize,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanNodeDto {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlanDto {
    pub nodes: Vec<PlanNodeDto>,
    #[serde(default)]
    pub edges: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    let close = if trimmed.as_bytes()[start] == b'{' {
        '}'
    } else {
        ']'
    };
    match trimmed.rfind(close) {
        Some(end) if end > start => &trimmed[start..=end],
        _ => trimmed,
    }
}

/// Escapes raw control characters (literal newlines, tabs, etc.) that appear
/// unescaped inside JSON strings. Reasoning models sometimes emit real
/// newlines inside string values (e.g. code snippets), which serde_json
/// rejects. Only used as a fallback after a strict parse fails.
pub fn sanitize_json_bare_control_chars(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
                out.push(ch);
            } else {
                match ch {
                    '\\' => {
                        escaped = true;
                        out.push(ch);
                    }
                    '"' => {
                        in_string = false;
                        out.push(ch);
                    }
                    c if (c as u32) < 0x20 => match c {
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        other => out.push_str(&format!("\\u{:04x}", other as u32)),
                    },
                    c => out.push(c),
                }
            }
        } else {
            match ch {
                '"' => {
                    in_string = true;
                    out.push(ch);
                }
                c => out.push(c),
            }
        }
    }
    out
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
        assert!(!ProbeQuestionDto {
            correct_index: 2,
            ..q
        }
        .validate());

        let quiz = QuizDto {
            prompt: "p".into(),
            options: vec!["a".into(), "b".into(), "c".into()],
            correct_index: 1,
            explanation: "e".into(),
        };
        assert!(quiz.validate().is_ok());
        assert!(QuizDto {
            correct_index: 9,
            ..quiz
        }
        .validate()
        .is_err());
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;

    #[test]
    fn escapes_literal_newlines_inside_strings() {
        let raw = "{\"q\":\"line one\nline two\",\"n\":3}";
        let fixed = sanitize_json_bare_control_chars(raw);
        let value: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert_eq!(value["q"], "line one\nline two");
        assert_eq!(value["n"], 3);
    }

    #[test]
    fn keeps_escaped_sequences_working() {
        // Already-valid JSON with escaped sequences must still parse identically.
        let raw = "{\"a\":\"has \\\"quotes\\\" and \\n escapes\"}";
        let fixed = sanitize_json_bare_control_chars(raw);
        let original: serde_json::Value = serde_json::from_str(raw).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert_eq!(original, reparsed);
    }

    #[test]
    fn escapes_tabs_and_other_controls() {
        let raw = "{\"code\":\"\tlet x;\u{1}\"}";
        let fixed = sanitize_json_bare_control_chars(raw);
        let value: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert!(value["code"].as_str().unwrap().contains('\t'));
    }
}
