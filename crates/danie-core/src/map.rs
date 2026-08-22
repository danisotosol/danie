//! The knowledge map: strands with mastery status plus a quiz log, persisted
//! as human-readable markdown.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::strand::StrandStatus;
use crate::{CoreError, Result};

/// A single learning strand (topic thread) and its mastery state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strand {
    /// Short name of the strand, e.g. "variables".
    pub name: String,
    /// Current mastery status.
    pub status: StrandStatus,
    /// Free-form evidence note explaining the status.
    pub evidence: String,
}

/// Outcome of one lock-in quiz answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuizOutcome {
    Correct,
    #[serde(rename = "near_miss")]
    NearMiss,
    Wrong,
    Idk,
}

impl std::fmt::Display for QuizOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            QuizOutcome::Correct => "correct",
            QuizOutcome::NearMiss => "near_miss",
            QuizOutcome::Wrong => "wrong",
            QuizOutcome::Idk => "idk",
        };
        f.write_str(text)
    }
}

impl std::str::FromStr for QuizOutcome {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "correct" => Ok(QuizOutcome::Correct),
            "near_miss" => Ok(QuizOutcome::NearMiss),
            "wrong" => Ok(QuizOutcome::Wrong),
            "idk" => Ok(QuizOutcome::Idk),
            other => Err(CoreError::InvalidFormat(format!(
                "unknown quiz outcome: {other}"
            ))),
        }
    }
}

/// One entry of the quiz log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizLogEntry {
    pub strand: String,
    pub answer: String,
    pub outcome: QuizOutcome,
}

/// Snapshot of what the learner knows about a goal, plus its quiz history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeMap {
    pub goal: String,
    pub updated: DateTime<Utc>,
    pub strands: Vec<Strand>,
    pub quiz_log: Vec<QuizLogEntry>,
}

impl KnowledgeMap {
    /// Creates an empty map for `goal` stamped with the current time.
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            updated: Utc::now(),
            strands: Vec::new(),
            quiz_log: Vec::new(),
        }
    }

    /// Inserts or updates the strand matching `name` exactly.
    pub fn upsert_strand(
        &mut self,
        name: impl Into<String>,
        status: StrandStatus,
        evidence: impl Into<String>,
    ) {
        let name = name.into();
        if let Some(existing) = self.strands.iter_mut().find(|s| s.name == name) {
            existing.status = status;
            existing.evidence = evidence.into();
        } else {
            self.strands.push(Strand {
                name,
                status,
                evidence: evidence.into(),
            });
        }
    }

    /// Returns all strands whose status equals `status`.
    pub fn strands_with(&self, status: StrandStatus) -> Vec<&Strand> {
        self.strands.iter().filter(|s| s.status == status).collect()
    }

    /// Appends a quiz outcome to the log.
    pub fn log_quiz(&mut self, entry: QuizLogEntry) {
        self.quiz_log.push(entry);
    }

    /// Renders the map as markdown with exact stable headers.
    ///
    /// Pipe characters (`|`) in strand names and evidence are sanitized to `/`
    /// so table rows stay well-formed; parsing therefore cannot recover literal
    /// pipes from persisted files.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Map — ");
        out.push_str(&sanitize_cell(&self.goal));
        out.push_str("\n\n");
        out.push_str(&format!("Updated: {}\n\n", self.updated.to_rfc3339()));
        out.push_str("## Strands\n\n");
        out.push_str("| strand | status | evidence |\n");
        out.push_str("|---|---|---|\n");
        for s in &self.strands {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                sanitize_cell(&s.name),
                s.status,
                sanitize_cell(&s.evidence)
            ));
        }
        out.push_str("\n## Log\n\n");
        for e in &self.quiz_log {
            out.push_str(&format!("- [{}] {} — {}\n", e.strand, e.answer, e.outcome));
        }
        out
    }

    /// Parses markdown produced by [`KnowledgeMap::to_markdown`].
    ///
    /// Tolerates a missing `## Log` section; strict about the shape of the
    /// `## Strands` table (header, separator, then rows with exactly 3 cells).
    pub fn from_markdown(text: &str) -> Result<Self> {
        let mut goal: Option<String> = None;
        let mut updated: Option<DateTime<Utc>> = None;
        let mut strands = Vec::new();
        let mut quiz_log = Vec::new();

        #[derive(PartialEq)]
        enum Section {
            None,
            Strands,
            Log,
        }
        let mut section = Section::None;
        let mut table_rows = 0usize;

        for raw in text.lines() {
            let line = raw.trim_end();

            if let Some(rest) = line.strip_prefix("# Map — ") {
                goal = Some(rest.trim().to_string());
                continue;
            }
            if let Some(rest) = line.strip_prefix("Updated: ") {
                let dt = chrono::DateTime::parse_from_rfc3339(rest.trim())
                    .map_err(|e| CoreError::InvalidFormat(format!("invalid date: {e}")))?;
                updated = Some(dt.with_timezone(&Utc));
                continue;
            }
            match line.trim() {
                "## Strands" => {
                    section = Section::Strands;
                    table_rows = 0;
                    continue;
                }
                "## Log" => {
                    section = Section::Log;
                    continue;
                }
                other if other.starts_with('#') => {
                    section = Section::None;
                    continue;
                }
                _ => {}
            }

            match section {
                Section::Strands => {
                    if !line.trim_start().starts_with('|') {
                        continue;
                    }
                    let cells = split_table_row(line);
                    table_rows += 1;
                    match table_rows {
                        1 => {
                            let expected = ["strand", "status", "evidence"];
                            if cells.len() != expected.len()
                                || !cells
                                    .iter()
                                    .zip(expected)
                                    .all(|(got, want)| got.eq_ignore_ascii_case(want))
                            {
                                return Err(CoreError::InvalidFormat(
                                    "invalid strand table header".into(),
                                ));
                            }
                        }
                        2 => {
                            if !is_separator_row(&cells) {
                                return Err(CoreError::InvalidFormat(
                                    "invalid strand table separator".into(),
                                ));
                            }
                        }
                        _ => {
                            if cells.len() != 3 {
                                return Err(CoreError::InvalidFormat(format!(
                                    "invalid strand row: expected 3 columns, got {}",
                                    cells.len()
                                )));
                            }
                            strands.push(Strand {
                                name: cells[0].to_string(),
                                status: cells[1].parse()?,
                                evidence: cells[2].to_string(),
                            });
                        }
                    }
                }
                Section::Log => {
                    if let Some(rest) = line.trim().strip_prefix("- [") {
                        if let Some(close) = rest.find(']') {
                            let strand_name = rest[..close].to_string();
                            let tail = rest[close + 1..].trim();
                            if let Some((answer, outcome)) = tail.rsplit_once(" — ") {
                                quiz_log.push(QuizLogEntry {
                                    strand: strand_name,
                                    answer: answer.trim().to_string(),
                                    outcome: outcome.trim().parse()?,
                                });
                            }
                        }
                    }
                }
                Section::None => {}
            }
        }

        let goal = goal
            .ok_or_else(|| CoreError::InvalidFormat("missing '# Map — <goal>' header".into()))?;
        let updated =
            updated.ok_or_else(|| CoreError::InvalidFormat("missing 'Updated:' line".into()))?;
        Ok(Self {
            goal,
            updated,
            strands,
            quiz_log,
        })
    }
}

fn sanitize_cell(text: &str) -> String {
    text.replace('|', "/").replace('\n', " ")
}

fn split_table_row(line: &str) -> Vec<&str> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(str::trim).collect()
}

fn is_separator_row(cells: &[&str]) -> bool {
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_map() -> KnowledgeMap {
        let mut map = KnowledgeMap::new("Functional programming");
        map.upsert_strand("lambdas", StrandStatus::Known, "explained currying unaided");
        map.upsert_strand("monads", StrandStatus::Edge, "confuses bind with fmap");
        map.upsert_strand("effects", StrandStatus::Unknown, "");
        map.log_quiz(QuizLogEntry {
            strand: "lambdas".into(),
            answer: "compose f g = \\x -> f (g x)".into(),
            outcome: QuizOutcome::Correct,
        });
        map.log_quiz(QuizLogEntry {
            strand: "monads".into(),
            answer: "no idea".into(),
            outcome: QuizOutcome::Idk,
        });
        map.log_quiz(QuizLogEntry {
            strand: "monads".into(),
            answer: "almost — associativity law".into(),
            outcome: QuizOutcome::NearMiss,
        });
        map
    }

    #[test]
    fn markdown_roundtrip_preserves_everything() {
        let map = sample_map();
        let back = KnowledgeMap::from_markdown(&map.to_markdown()).unwrap();
        assert_eq!(back, map);
    }

    #[test]
    fn parse_is_tolerant_of_missing_log_section() {
        let map = sample_map();
        let md = map.to_markdown();
        let cut = md.find("## Log").unwrap();
        let without = KnowledgeMap::from_markdown(&md[..cut]).unwrap();
        assert!(without.quiz_log.is_empty());
        assert_eq!(without.strands, map.strands);
        assert_eq!(without.goal, map.goal);
        assert_eq!(without.updated, map.updated);
    }

    #[test]
    fn pipes_in_cells_are_sanitized_to_slash() {
        let mut map = KnowledgeMap::new("shell");
        map.upsert_strand("pipes", StrandStatus::Known, "uses | grep | wc");
        let md = map.to_markdown();
        assert!(md.contains("| uses / grep / wc |"));
        assert!(!md.contains("uses | grep"));
        let mut expected = KnowledgeMap {
            updated: map.updated,
            ..KnowledgeMap::new("shell")
        };
        expected.upsert_strand("pipes", StrandStatus::Known, "uses / grep / wc");
        assert_eq!(KnowledgeMap::from_markdown(&md).unwrap(), expected);
    }

    #[test]
    fn missing_headers_are_invalid_format_errors() {
        assert!(KnowledgeMap::from_markdown("no header at all").is_err());

        let map = KnowledgeMap::new("x");
        let md = map.to_markdown();
        let no_updated = md.replace(
            &format!("Updated: {}", map.updated.to_rfc3339()),
            "Updated: yesterday",
        );
        assert!(KnowledgeMap::from_markdown(&no_updated).is_err());
    }
}
