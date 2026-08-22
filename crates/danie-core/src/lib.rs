//! danie-core: domain layer of the danie terminal AI tutor.
//!
//! Implements the Alvar learning loop primitives — knowledge strands, plan
//! DAGs, lock-in quiz scheduling (SM-2), skill parsing and `.danie/` storage —
//! without depending on any other workspace crate or on LLM access.

pub mod dag;
pub mod map;
pub mod profile;
pub mod quiz;
pub mod session;
pub mod skills;
pub mod store;
pub mod strand;

/// Errors produced by domain operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("cycle detected in plan")]
    Cycle,
    #[error("invalid format: {0}")]
    InvalidFormat(String),
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, CoreError>;

pub use dag::{PlanGraph, PlanNode};
pub use map::{KnowledgeMap, QuizLogEntry, QuizOutcome, Strand};
pub use profile::LearnerProfile;
pub use quiz::{QuizQuestion, SrsCard, SrsQueue};
pub use session::SessionSummary;
pub use skills::{parse_skill_md, render_skill_md, SkillFrontmatter};
pub use store::{slugify, DanieStore};
pub use strand::StrandStatus;
