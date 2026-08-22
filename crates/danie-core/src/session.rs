//! Session summaries: what was locked in during one tutoring session.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// A summary of one teaching session, persisted as human-readable markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub date: DateTime<Utc>,
    pub topic: String,
    pub locked: Vec<String>,
    pub edge: Vec<String>,
    pub next_node: Option<String>,
    pub notes: String,
}

impl SessionSummary {
    /// Renders the summary as markdown with exact stable headers.
    ///
    /// The `## Next node` section is omitted when `next_node` is `None`.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Session\n\n");
        out.push_str(&format!("- Date: {}\n", self.date.to_rfc3339()));
        out.push_str(&format!("- Topic: {}\n\n", self.topic));
        out.push_str("## Locked\n");
        for node in &self.locked {
            out.push_str(&format!("- {node}\n"));
        }
        out.push_str("\n## On the edge\n");
        for node in &self.edge {
            out.push_str(&format!("- {node}\n"));
        }
        out.push('\n');
        if let Some(node) = &self.next_node {
            out.push_str("## Next node\n");
            out.push_str(&format!("- {node}\n\n"));
        }
        out.push_str("## Notes\n");
        out.push_str(self.notes.trim());
        out.push('\n');
        out
    }

    /// Parses markdown produced by [`SessionSummary::to_markdown`].
    pub fn from_markdown(text: &str) -> Result<Self> {
        let lines: Vec<&str> = text.lines().collect();
        if !lines.iter().any(|l| l.trim() == "# Session") {
            return Err(CoreError::InvalidFormat(
                "missing '# Session' header".into(),
            ));
        }

        let mut date = None;
        let mut topic = None;
        for line in &lines {
            if let Some(rest) = line.strip_prefix("- Date: ") {
                let dt = DateTime::parse_from_rfc3339(rest.trim())
                    .map_err(|e| CoreError::InvalidFormat(format!("invalid date: {e}")))?;
                date = Some(dt.with_timezone(&Utc));
            } else if let Some(rest) = line.strip_prefix("- Topic: ") {
                topic = Some(rest.trim().to_string());
            }
        }
        let date = date.ok_or_else(|| CoreError::InvalidFormat("missing '- Date:' line".into()))?;
        let topic =
            topic.ok_or_else(|| CoreError::InvalidFormat("missing '- Topic:' line".into()))?;

        let locked = bullets_of_section(&lines, "## Locked");
        let edge = bullets_of_section(&lines, "## On the edge");
        let next_node = bullets_of_section(&lines, "## Next node")
            .into_iter()
            .next();
        let notes = free_text_of_section(&lines, "## Notes");

        Ok(Self {
            date,
            topic,
            locked,
            edge,
            next_node,
            notes,
        })
    }
}

fn section_range(lines: &[&str], heading: &str) -> Option<std::ops::Range<usize>> {
    let start = lines.iter().position(|l| l.trim() == heading)? + 1;
    let end = lines[start..]
        .iter()
        .position(|l| l.trim_start().starts_with("## "))
        .map(|off| start + off)
        .unwrap_or(lines.len());
    Some(start..end)
}

fn bullets_of_section(lines: &[&str], heading: &str) -> Vec<String> {
    match section_range(lines, heading) {
        Some(range) => lines[range]
            .iter()
            .filter_map(|l| l.trim().strip_prefix("- "))
            .map(|s| s.trim().to_string())
            .collect(),
        None => Vec::new(),
    }
}

fn free_text_of_section(lines: &[&str], heading: &str) -> String {
    match section_range(lines, heading) {
        Some(range) => lines[range].join("\n").trim().to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(next_node: Option<String>) -> SessionSummary {
        SessionSummary {
            date: Utc::now(),
            topic: "Recursion in Rust".into(),
            locked: vec!["basic-recursion".into(), "base-case".into()],
            edge: vec!["tail-recursion".into()],
            next_node,
            notes: "Struggled to spot the base case.\nPractice with fibonacci.".into(),
        }
    }

    #[test]
    fn roundtrip_with_next_node() {
        let s = sample(Some("memoization".into()));
        let back = SessionSummary::from_markdown(&s.to_markdown()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn roundtrip_without_next_node_omits_section() {
        let s = sample(None);
        let md = s.to_markdown();
        assert!(!md.contains("## Next node"));
        assert!(md.contains("## Notes"));
        let back = SessionSummary::from_markdown(&md).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.next_node, None);
    }

    #[test]
    fn missing_title_is_invalid_format() {
        assert!(SessionSummary::from_markdown("loose text").is_err());
    }
}
