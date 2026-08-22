//! Filesystem storage under a `.danie/` directory.
//!
//! Layout:
//!
//! ```text
//! <root>/
//!   profile.md          learner profile
//!   maps/<slug>.md       knowledge maps, one per goal
//!   sessions/<YYYYMMDD>-<slug>.md   session summaries
//!   srs.json             spaced-repetition queue
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use crate::map::KnowledgeMap;
use crate::profile::LearnerProfile;
use crate::quiz::SrsQueue;
use crate::session::SessionSummary;
use crate::{CoreError, Result};

/// A handle to an on-disk `.danie/` store.
pub struct DanieStore {
    root: PathBuf,
}

impl DanieStore {
    /// Opens (or creates) the directory tree rooted at `dir`, idempotently.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        fs::create_dir_all(dir.join("maps"))?;
        fs::create_dir_all(dir.join("sessions"))?;
        Ok(Self { root: dir.to_path_buf() })
    }

    /// Loads `perfil.md`; returns the default profile when missing.
    pub fn load_profile(&self) -> Result<LearnerProfile> {
        let path = self.root.join("profile.md");
        if !path.exists() {
            return Ok(LearnerProfile::default());
        }
        LearnerProfile::from_markdown(&fs::read_to_string(path)?)
    }

    /// Saves the profile to `perfil.md`.
    pub fn save_profile(&self, profile: &LearnerProfile) -> Result<()> {
        fs::write(self.root.join("profile.md"), profile.to_markdown())?;
        Ok(())
    }

    /// Saves the map to `maps/<slug>.md` and returns the path written.
    pub fn save_map(&self, map: &KnowledgeMap) -> Result<PathBuf> {
        let slug = slugify(&map.goal);
        let path = self.root.join("maps").join(format!("{slug}.md"));
        fs::write(&path, map.to_markdown())?;
        Ok(path)
    }

    /// Loads the map stored at `maps/<goal_slug>.md`.
    pub fn load_map(&self, goal_slug: &str) -> Result<KnowledgeMap> {
        let path = self.root.join("maps").join(format!("{goal_slug}.md"));
        if !path.exists() {
            return Err(CoreError::NotFound(path.display().to_string()));
        }
        KnowledgeMap::from_markdown(&fs::read_to_string(&path)?)
    }

    /// Lists map slugs found in `maps/`, sorted alphabetically.
    pub fn list_maps(&self) -> Vec<String> {
        markdown_stems(&self.root.join("maps"))
    }

    /// Saves the session to `sessions/<YYYYMMDD>-<slug>.md` and returns the
    /// path written. The date prefix uses the session's UTC date.
    pub fn save_session(&self, session: &SessionSummary) -> Result<PathBuf> {
        let name = format!(
            "{}-{}.md",
            session.date.format("%Y%m%d"),
            slugify(&session.topic)
        );
        let path = self.root.join("sessions").join(name);
        fs::write(&path, session.to_markdown())?;
        Ok(path)
    }

    /// Lists session file stems in `sessions/`, sorted alphabetically.
    pub fn list_sessions(&self) -> Vec<String> {
        markdown_stems(&self.root.join("sessions"))
    }

    /// Loads `srs.json`; returns an empty queue when missing.
    pub fn load_queue(&self) -> Result<SrsQueue> {
        let path = self.root.join("srs.json");
        if !path.exists() {
            return Ok(SrsQueue::default());
        }
        SrsQueue::from_json(&fs::read_to_string(path)?)
    }

    /// Saves the queue to `srs.json`.
    pub fn save_queue(&self, queue: &SrsQueue) -> Result<()> {
        fs::write(self.root.join("srs.json"), queue.to_json()?)?;
        Ok(())
    }
}

fn markdown_stems(dir: &Path) -> Vec<String> {
    let mut stems = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    stems.push(stem.to_string());
                }
            }
        }
    }
    stems.sort();
    stems
}

/// Converts arbitrary text into a URL-safe slug: lowercased, ASCII-folded
/// accents (`á` to `a`), non-alphanumeric runs collapsed into single hyphens,
/// leading/trailing hyphens trimmed.
pub fn slugify(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for ch in text.chars().flat_map(|c| c.to_lowercase()) {
        match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => folded.push('a'),
            'é' | 'è' | 'ê' | 'ë' => folded.push('e'),
            'í' | 'ì' | 'î' | 'ï' => folded.push('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => folded.push('o'),
            'ú' | 'ù' | 'û' | 'ü' => folded.push('u'),
            'ñ' => folded.push('n'),
            'ç' => folded.push('c'),
            'ý' | 'ÿ' => folded.push('y'),
            'ß' => folded.push_str("ss"),
            c if c.is_ascii_alphanumeric() => folded.push(c),
            _ => folded.push('-'),
        }
    }
    let mut out = String::with_capacity(folded.len());
    let mut prev_dash = false;
    for ch in folded.chars() {
        if ch == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
        } else {
            prev_dash = false;
        }
        out.push(ch);
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store(tag: &str) -> (DanieStore, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "danie-core-{}-{}-{nanos}",
            tag,
            std::process::id()
        ));
        let store = DanieStore::open(&dir).unwrap();
        (store, dir)
    }

    #[test]
    fn slugify_folds_accents_and_collapses_separators() {
        assert_eq!(slugify("Árbol Genealógico Ñandú"), "arbol-genealogico-nandu");
        assert_eq!(slugify("Hola, Mundo!!!"), "hola-mundo");
        assert_eq!(slugify("  --A  B--  "), "a-b");
        assert_eq!(slugify("straße"), "strasse");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn profile_roundtrip_with_missing_file_defaulting() {
        let (store, dir) = temp_store("profile");
        let loaded = store.load_profile().unwrap();
        assert_eq!(loaded, LearnerProfile::default());
        assert_eq!(loaded.language, "es");

        let mut profile = LearnerProfile::default();
        profile.solid_ground.push("python".into());
        profile.goals.push("learn rust".into());
        profile.pace_notes = Some("Short sessions.".into());
        store.save_profile(&profile).unwrap();
        assert_eq!(store.load_profile().unwrap(), profile);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn map_roundtrip_slug_and_listing() {
        let (store, dir) = temp_store("map");
        let mut map = KnowledgeMap::new("Rust programming!");
        map.upsert_strand("variables", crate::strand::StrandStatus::Known, "clear mastery");
        let path = store.save_map(&map).unwrap();
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            "rust-programming.md"
        );

        let loaded = store.load_map("rust-programming").unwrap();
        assert_eq!(loaded.goal, "Rust programming!");
        assert_eq!(loaded.strands, map.strands);

        assert_eq!(store.list_maps(), vec!["rust-programming"]);
        assert!(matches!(
            store.load_map("no-existe"),
            Err(CoreError::NotFound(_))
        ));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn session_save_uses_date_prefixed_filename() {
        let (store, dir) = temp_store("session");
        let session = SessionSummary {
            date: chrono::Utc::now(),
            topic: "Basic recursion".into(),
            locked: vec!["recursion".into()],
            edge: vec![],
            next_node: Some("memoization".into()),
            notes: "All good.".into(),
        };
        let path = store.save_session(&session).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        assert!(name.ends_with("-basic-recursion.md"));
        assert_eq!(name.len(), "20260101-basic-recursion.md".len());
        assert_eq!(store.list_sessions(), vec![name.trim_end_matches(".md")]);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn srs_queue_roundtrip_and_empty_default() {
        let (store, dir) = temp_store("srs");
        let empty = store.load_queue().unwrap();
        assert!(empty.cards.is_empty());

        let mut queue = SrsQueue::default();
        queue.upsert_card("variables");
        queue.upsert_card("tipos");
        store.save_queue(&queue).unwrap();
        let loaded = store.load_queue().unwrap();
        assert_eq!(loaded, queue);
        assert_eq!(loaded.cards.len(), 2);

        let _ = fs::remove_dir_all(dir);
    }
}
