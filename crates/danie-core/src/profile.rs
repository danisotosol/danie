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

    /// Parses learner-profile markdown.
    ///
    /// Accepts the canonical format written by [`LearnerProfile::to_markdown`]
    /// plus common real-world variants: legacy Spanish headers
    /// (`# Perfil del aprendiz`, `## Terreno sólido`, …), free-form document
    /// titles (`# Learner profile for danie`) and bare `Key: value` lines
    /// without the leading dash.
    pub fn from_markdown(text: &str) -> Result<Self> {
        let lines: Vec<&str> = text.lines().collect();

        let header_ok = lines.iter().any(|l| {
            let trimmed = l.trim();
            trimmed.starts_with('#') && {
                let inner = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
                inner.contains("learner") || inner.contains("perfil")
            }
        });

        let language_found = key_line(
            &lines,
            &["- Language:", "- Idioma:", "Language:", "Idioma:"],
        )
        .filter(|v| !v.is_empty());
        let language = language_found
            .clone()
            .unwrap_or_else(|| "en".to_string());

        let mut solid_ground =
            section_bullets(&lines, &["## Solid ground", "## Terreno sólido"]);
        if solid_ground.is_empty() {
            if let Some(value) = key_line(&lines, &["Solid ground:", "Terreno sólido:"]) {
                if !value.is_empty() {
                    solid_ground.push(value);
                }
            }
        }

        let mut goals = section_bullets(&lines, &["## Goals", "## Metas"]);
        if goals.is_empty() {
            if let Some(value) = key_line(&lines, &["Goals:", "Metas:"]) {
                if !value.is_empty() {
                    goals.push(value);
                }
            }
        }

        let pace_notes = section_text(&lines, &["## Pace", "## Ritmo"])
            .or_else(|| key_line(&lines, &["Pace notes:", "Pace:", "Ritmo:"]))
            .filter(|v| !v.is_empty());
        let struggle_prefs = section_text(&lines, &["## Struggle", "## Lucha"])
            .or_else(|| key_line(&lines, &["Struggle notes:", "Struggle:", "Lucha:"]))
            .filter(|v| !v.is_empty());
        let voice_prefs = section_text(&lines, &["## Voice", "## Voz"])
            .or_else(|| key_line(&lines, &["Voice notes:", "Voice:", "Voz:"]))
            .filter(|v| !v.is_empty());

        let recognizable = language_found.is_some()
            || !solid_ground.is_empty()
            || !goals.is_empty()
            || pace_notes.is_some()
            || struggle_prefs.is_some()
            || voice_prefs.is_some();
        if !header_ok && !recognizable {
            return Err(CoreError::InvalidFormat(
                "missing '# Learner Profile' header".into(),
            ));
        }

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

fn heading_matches(line: &str, headings: &[&str]) -> bool {
    let trimmed = line.trim();
    headings
        .iter()
        .any(|heading| trimmed.eq_ignore_ascii_case(heading.trim()))
}

/// Returns the value of the first `Key: value` (or `- Key: value`) line
/// matching any of `keys`. Matching is ASCII-case-insensitive and never
/// panics on multi-byte characters.
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

fn section_bullets(lines: &[&str], headings: &[&str]) -> Vec<String> {
    match section_range(lines, headings) {
        Some(range) => lines[range]
            .iter()
            .filter_map(|l| l.trim().strip_prefix("- "))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => Vec::new(),
    }
}

fn section_text(lines: &[&str], headings: &[&str]) -> Option<String> {
    match section_range(lines, headings) {
        Some(range) => {
            let text = lines[range].join("\n").trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        }
        None => None,
    }
}

fn section_range(lines: &[&str], headings: &[&str]) -> Option<std::ops::Range<usize>> {
    let start = lines.iter().position(|l| heading_matches(l, headings))? + 1;
    let end = lines[start..]
        .iter()
        .position(|l| l.trim_start().starts_with("## "))
        .map(|off| start + off)
        .unwrap_or(lines.len());
    Some(start..end)
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

    #[test]
    fn parses_ad_hoc_profile_without_sections() {
        let md = "# Learner profile for danie\nLanguage: en\nSolid ground: general programming curiosity\nGoals: learn Rust and AI development\nPace notes: prefers short explanations with a check question\n";
        let back = LearnerProfile::from_markdown(md).unwrap();
        assert_eq!(back.language, "en");
        assert_eq!(back.solid_ground, vec!["general programming curiosity"]);
        assert_eq!(back.goals, vec!["learn Rust and AI development"]);
        assert_eq!(
            back.pace_notes.as_deref(),
            Some("prefers short explanations with a check question")
        );
    }

    #[test]
    fn parses_legacy_spanish_profile() {
        let md = "# Perfil del aprendiz\n\n- Idioma: es\n\n## Terreno sólido\n- python\n\n## Metas\n- aprender rust\n\n## Lucha\nTeoría abstracta.\n";
        let back = LearnerProfile::from_markdown(md).unwrap();
        assert_eq!(back.language, "es");
        assert_eq!(back.solid_ground, vec!["python"]);
        assert_eq!(back.goals, vec!["aprender rust"]);
        assert_eq!(back.struggle_prefs.as_deref(), Some("Teoría abstracta."));
    }
}
