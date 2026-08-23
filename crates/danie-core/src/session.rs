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
    ///
    /// Also accepts legacy Spanish sessions (`# Sesión`, `- Fecha:`, `## Fijado`, …)
    /// and key lines without the leading dash.
    pub fn from_markdown(text: &str) -> Result<Self> {
        let lines: Vec<&str> = text.lines().collect();
        let has_header = lines.iter().any(|l| {
            let trimmed = l.trim();
            trimmed.starts_with('#') && {
                let inner = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
                inner.contains("session") || inner.contains("sesi")
            }
        });
        if !has_header {
            return Err(CoreError::InvalidFormat("missing '# Session' header".into()));
        }

        fn key_line(lines: &[&str], keys: &[&str]) -> Option<String> {
            fn starts_with_ignore_case(hay: &str, needle: &str) -> bool {
                let (h, n) = (hay.as_bytes(), needle.as_bytes());
                h.len() >= n.len()
                    && h.iter()
                        .zip(n)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
            }
            for line in lines {
                let trimmed = line.trim();
                let without_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed);
                for key in keys {
                    if starts_with_ignore_case(without_dash, key) {
                        return Some(without_dash[key.len()..].trim().to_string());
                    }
                }
            }
            None
        }

        let date = key_line(&lines, &["Date:", "Fecha:"])
            .ok_or_else(|| CoreError::InvalidFormat("missing '- Date:' line".into()))
            .and_then(|rest| {
                DateTime::parse_from_rfc3339(rest.trim())
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| CoreError::InvalidFormat(format!("invalid date: {e}")))
            })?;
        let topic =
            key_line(&lines, &["Topic:", "Tema:"]).ok_or_else(|| {
                CoreError::InvalidFormat("missing '- Topic:' line".into())
            })?;

        let locked = bullets_of_section(&lines, &["## Locked", "## Fijado"]);
        let edge = bullets_of_section(&lines, &["## On the edge", "## En el borde"]);
        let next_node = bullets_of_section(&lines, &["## Next node", "## Siguiente nodo"])
            .into_iter()
            .next();
        let notes = free_text_of_section(&lines, &["## Notes", "## Notas"]);

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

fn section_range(lines: &[&str], headings: &[&str]) -> Option<std::ops::Range<usize>> {
    let start = lines
        .iter()
        .position(|l| {
            let trimmed = l.trim();
            headings
                .iter()
                .any(|heading| trimmed.eq_ignore_ascii_case(heading.trim()))
        })?
        + 1;
    let end = lines[start..]
        .iter()
        .position(|l| l.trim_start().starts_with("## "))
        .map(|off| start + off)
        .unwrap_or(lines.len());
    Some(start..end)
}

fn bullets_of_section(lines: &[&str], headings: &[&str]) -> Vec<String> {
    match section_range(lines, headings) {
        Some(range) => lines[range]
            .iter()
            .filter_map(|l| l.trim().strip_prefix("- "))
            .map(|s| s.trim().to_string())
            .collect(),
        None => Vec::new(),
    }
}

fn free_text_of_section(lines: &[&str], headings: &[&str]) -> String {
    match section_range(lines, headings) {
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

    #[test]
    fn parses_legacy_spanish_session() {
        let md = "# Sesión\n\n- Fecha: 2026-08-22T10:00:00+00:00\n- Tema: rust\n\n## Fijado\n- ownership\n\n## En el borde\n- borrowing\n\n## Notas\nok\n";
        let back = SessionSummary::from_markdown(md).unwrap();
        assert_eq!(back.topic, "rust");
        assert_eq!(back.locked, vec!["ownership"]);
        assert_eq!(back.edge, vec!["borrowing"]);
        assert_eq!(back.notes, "ok");
    }

    #[test]
    fn parses_key_lines_without_dash() {
        let md = "# Session\nDate: 2026-08-22T10:00:00+00:00\nTopic: rust\n## Locked\n- ownership\n";
        let back = SessionSummary::from_markdown(md).unwrap();
        assert_eq!(back.topic, "rust");
        assert_eq!(back.locked, vec!["ownership"]);
    }
}
