//! SKILL.md parsing: YAML frontmatter plus a free-form instruction body.

use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// Frontmatter of a SKILL.md file. Unknown YAML keys are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// Parses a SKILL.md document into its frontmatter and body.
///
/// The text must start with a `---` line; everything up to the next `---` line
/// is YAML frontmatter, and the body is the remainder after consuming exactly
/// one newline boundary following the closing delimiter.
pub fn parse_skill_md(text: &str) -> Result<(SkillFrontmatter, String)> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("---") {
        return Err(CoreError::InvalidFormat(
            "missing frontmatter (must start with '---')".into(),
        ));
    }
    let after_open = trim_one_newline(&trimmed[3..]);

    let mut offset = 0usize;
    let mut close = None;
    for line in after_open.split_inclusive('\n') {
        if line.trim_end().trim() == "---" {
            close = Some((offset, line.len()));
            break;
        }
        offset += line.len();
    }
    let (close_offset, close_len) = close
        .ok_or_else(|| CoreError::InvalidFormat("unclosed frontmatter ('---')".into()))?;

    let yaml_text = &after_open[..close_offset];
    let body = &after_open[close_offset + close_len..];

    let fm: SkillFrontmatter = serde_yaml::from_str(yaml_text).map_err(|e| {
        CoreError::InvalidFormat(format!("invalid frontmatter yaml: {e}"))
    })?;
    if fm.name.trim().is_empty() || fm.description.trim().is_empty() {
        return Err(CoreError::InvalidFormat(
            "name and description are required".into(),
        ));
    }
    Ok((fm, body.to_string()))
}

/// Renders frontmatter and body back into a SKILL.md document that
/// [`parse_skill_md`] round-trips exactly.
pub fn render_skill_md(fm: &SkillFrontmatter, body: &str) -> String {
    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(fm.name.clone()),
    );
    mapping.insert(
        serde_yaml::Value::String("description".into()),
        serde_yaml::Value::String(fm.description.clone()),
    );
    if let Some(license) = &fm.license {
        mapping.insert(
            serde_yaml::Value::String("license".into()),
            serde_yaml::Value::String(license.clone()),
        );
    }
    let yaml = serde_yaml::to_string(&mapping).expect("yaml serialization cannot fail");
    format!("---\n{yaml}---\n{body}")
}

fn trim_one_newline(s: &str) -> &str {
    s.strip_prefix("\r\n")
        .unwrap_or_else(|| s.strip_prefix('\n').unwrap_or(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALVAR_SAMPLE: &str = "---\nname: teach\ndescription: >\n  Explains the chosen concept with a concrete analogy,\n  a minimal example and a check question.\nlicense: MIT\n---\n\nBody with teaching instructions.\n";

    #[test]
    fn parses_alvar_style_sample_with_folded_description() {
        let (fm, body) = parse_skill_md(ALVAR_SAMPLE).unwrap();
        assert_eq!(fm.name, "teach");
        assert_eq!(fm.license.as_deref(), Some("MIT"));
        assert!(fm.description.contains("Explains the chosen concept"));
        assert!(fm.description.contains("check question."));
        assert!(fm.description.contains("analogy, a minimal example"));
        assert!(!fm.description.contains("\n  "));
        assert_eq!(body, "\nBody with teaching instructions.\n");
    }

    #[test]
    fn unclosed_frontmatter_is_rejected() {
        let err = parse_skill_md("---\nname: teach\n").unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
    }

    #[test]
    fn missing_frontmatter_is_rejected() {
        let err = parse_skill_md("# Body only\\n").unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
    }

    #[test]
    fn bad_yaml_is_rejected() {
        let err = parse_skill_md("---\nname: [unclosed\n---\nbody").unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
    }

    #[test]
    fn empty_name_or_description_is_rejected() {
        let err = parse_skill_md("---\nname: \"\"\ndescription: something\n---\nx").unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
        let err = parse_skill_md("---\nname: algo\ndescription: \"   \"\n---\nx").unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
    }

    #[test]
    fn render_parse_roundtrip_preserves_frontmatter_and_body() {
        let (fm, body) = parse_skill_md(ALVAR_SAMPLE).unwrap();
        let rendered = render_skill_md(&fm, &body);
        let (fm2, body2) = parse_skill_md(&rendered).unwrap();
        assert_eq!((fm2, body2), (fm, body));
    }

    #[test]
    fn roundtrip_without_license_omits_key() {
        let fm = SkillFrontmatter {
            name: "quiz".into(),
            description: "Generates lock-in questions.".into(),
            license: None,
        };
        let rendered = render_skill_md(&fm, "body\n");
        assert!(!rendered.contains("license"));
        let (fm2, body2) = parse_skill_md(&rendered).unwrap();
        assert_eq!(fm2, fm);
        assert_eq!(body2, "body\n");
    }
}
