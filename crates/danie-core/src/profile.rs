//! The learner profile: background, goals and communication preferences.

use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// Persistent description of the learner used to personalize tutoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnerProfile {
    /// Teaching language tag used by the tutor; defaults to "en".
    pub language: String,
    /// Topics the learner already masters.
    pub solid_ground: Vec<String>,
    /// Learning goals.
    pub goals: Vec<String>,
    /// Free-text notes about preferred pace.
    pub pace_notes: Option<String>,
    /// Free-text notes about what the learner struggles with.
    pub struggle_prefs: Option<String>,
    /// Free-text notes about tone and voice preferences.
    pub voice_prefs: Option<String>,
}

impl Default for LearnerProfile {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            solid_ground: Vec::new(),
            goals: Vec::new(),
            pace_notes: None,
            struggle_prefs: None,
            voice_prefs: None,
        }
    }
}

impl LearnerProfile {
    /// Renders the profile as markdown with exact stable headers.
    ///
    /// Optional free-text sections (`## Pace`, `## Struggle`, `## Voice`) are
    /// omitted when their field is `None`.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Learner Profile\n\n");
        out.push_str(&format!("- Language: {}\n\n", self.language));

        out.push_str("## Solid ground\n");
        for item in &self.solid_ground {
            out.push_str(&format!("- {item}\n"));
        }
        out.push_str("\n## Goals\n");
        for goal in &self.goals {
            out.push_str(&format!("- {goal}\n"));
        }

        for (heading, value) in [
            ("## Pace", &self.pace_notes),
            ("## Struggle", &self.struggle_prefs),
            ("## Voice", &self.voice_prefs),
        ] {
            if let Some(text) = value {
                out.push_str(&format!("\n{heading}\n{}\n", text.trim()));
            }
        }
        out
    }

    /// Parses markdown produced by [`LearnerProfile::to_markdown`].
    ///
    /// Missing optional sections yield `None`; a missing `- Language:` line
    /// falls back to `"en"`.
    pub fn from_markdown(text: &str) -> Result<Self> {
        if !text.lines().any(|l| l.trim() == "# Learner Profile") {
            return Err(CoreError::InvalidFormat(
                "missing '# Learner Profile' header".into(),
            ));
        }
        let lines: Vec<&str> = text.lines().collect();

        let language = lines
            .iter()
            .find_map(|l| l.strip_prefix("- Language: "))
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|| "en".to_string());

        let solid_ground = bullets_of_section(&lines, "## Solid ground");
        let goals = bullets_of_section(&lines, "## Goals");
        let pace_notes = free_text_of_section(&lines, "## Pace");
        let struggle_prefs = free_text_of_section(&lines, "## Struggle");
        let voice_prefs = free_text_of_section(&lines, "## Voice");

        Ok(Self {
            language,
            solid_ground,
            goals,
            pace_notes,
            struggle_prefs,
            voice_prefs,
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
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    }
}

fn free_text_of_section(lines: &[&str], heading: &str) -> Option<String> {
    match section_range(lines, heading) {
        Some(range) => {
            let text = lines[range].join("\n").trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_all_sections() {
        let profile = LearnerProfile {
            language: "es".into(),
            solid_ground: vec!["basic algebra".into(), "python".into()],
            goals: vec!["learn rust".into()],
            pace_notes: Some("Prefers short 20-minute sessions.".into()),
            struggle_prefs: Some("Gets stuck on abstract theory without examples.".into()),
            voice_prefs: Some("Direct tone, no jokes.".into()),
        };
        let back = LearnerProfile::from_markdown(&profile.to_markdown()).unwrap();
        assert_eq!(back, profile);
    }

    #[test]
    fn missing_optional_sections_default_to_none() {
        let profile = LearnerProfile {
            language: "en".into(),
            solid_ground: vec![],
            goals: vec![],
            pace_notes: None,
            struggle_prefs: None,
            voice_prefs: None,
        };
        let md = profile.to_markdown();
        assert!(!md.contains("## Pace"));
        assert!(!md.contains("## Struggle"));
        assert!(!md.contains("## Voice"));
        let back = LearnerProfile::from_markdown(&md).unwrap();
        assert_eq!(back, profile);
    }

    #[test]
    fn language_line_is_optional_and_defaults_to_english() {
        let md = "# Learner Profile\n\n## Goals\n- rust\n";
        let back = LearnerProfile::from_markdown(md).unwrap();
        assert_eq!(back.language, "en");
        assert_eq!(back.goals, vec!["rust"]);
    }

    #[test]
    fn missing_title_is_invalid_format() {
        assert!(LearnerProfile::from_markdown("no header here").is_err());
    }
}
