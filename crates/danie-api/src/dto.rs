//! Request DTOs for the HTTP API and their conversion into domain types.

use danie_core::{LearnerProfile, PlanNode, Strand, StrandStatus};
use serde::Deserialize;

/// Learner profile as supplied per request; omitted fields fall back to the
/// documented defaults of [`LearnerProfile`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProfileDto {
    pub language: Option<String>,
    #[serde(default)]
    pub solid_ground: Vec<String>,
    #[serde(default)]
    pub goals: Vec<String>,
    pub pace_notes: Option<String>,
    pub struggle_prefs: Option<String>,
    pub voice_prefs: Option<String>,
}

impl ProfileDto {
    /// Builds the domain profile, applying defaults for omitted fields.
    pub fn into_profile(self) -> LearnerProfile {
        LearnerProfile {
            language: self.language.unwrap_or_else(|| "en".to_string()),
            solid_ground: self.solid_ground,
            goals: self.goals,
            pace_notes: self.pace_notes,
            struggle_prefs: self.struggle_prefs,
            voice_prefs: self.voice_prefs,
        }
    }
}

/// One strand of prior knowledge supplied by the client. `status` must parse
/// as a [`StrandStatus`] (`known`, `edge`, `unknown`, `blocked`).
#[derive(Debug, Clone, Deserialize)]
pub struct StrandInputDto {
    pub name: String,
    pub status: String,
    pub evidence: String,
}

impl StrandInputDto {
    /// Parses the status and converts to a domain [`Strand`].
    pub fn into_strand(self) -> danie_core::Result<Strand> {
        Ok(Strand {
            name: self.name,
            status: self.status.parse::<StrandStatus>()?,
            evidence: self.evidence,
        })
    }
}

/// A plan node as supplied by the client (`id`, `title`, `summary`).
#[derive(Debug, Clone, Deserialize)]
pub struct NodeDto {
    pub id: String,
    pub title: String,
    pub summary: String,
}

impl From<NodeDto> for PlanNode {
    fn from(value: NodeDto) -> Self {
        PlanNode {
            id: value.id,
            title: value.title,
            summary: value.summary,
        }
    }
}

/// Body of `POST /v1/probe`.
#[derive(Debug, Deserialize)]
pub struct ProbeRequest {
    pub goal: String,
    pub profile: Option<ProfileDto>,
}

/// Body of `POST /v1/plan`.
#[derive(Debug, Deserialize)]
pub struct PlanRequest {
    pub goal: String,
    pub strands: Vec<StrandInputDto>,
    pub profile: Option<ProfileDto>,
}

/// Body of `POST /v1/lesson`.
#[derive(Debug, Deserialize)]
pub struct LessonRequest {
    pub goal: String,
    pub node: NodeDto,
    pub prereq_titles: Vec<String>,
    pub strands: Vec<StrandInputDto>,
    pub profile: Option<ProfileDto>,
}

/// Body of `POST /v1/prereq`.
#[derive(Debug, Deserialize)]
pub struct PrereqRequest {
    pub goal: String,
    pub current: NodeDto,
    pub existing_ids: Vec<String>,
    pub profile: Option<ProfileDto>,
}

/// Body of `POST /v1/review-question`.
#[derive(Debug, Deserialize)]
pub struct ReviewQuestionRequest {
    pub node_id: String,
    pub context: Option<String>,
    pub profile: Option<ProfileDto>,
}
