//! Mastery statuses for learning strands.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// Mastery level of a learning strand within the Alvar loop.
///
/// - `Known`: solidly understood and locked in.
/// - `Edge`: partially understood, at the frontier of knowledge.
/// - `Unknown`: not yet approached.
/// - `Blocked`: depends on prerequisites the learner does not have yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrandStatus {
    Known,
    Edge,
    Unknown,
    Blocked,
}

impl fmt::Display for StrandStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            StrandStatus::Known => "known",
            StrandStatus::Edge => "edge",
            StrandStatus::Unknown => "unknown",
            StrandStatus::Blocked => "blocked",
        };
        f.write_str(text)
    }
}

impl FromStr for StrandStatus {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "known" => Ok(StrandStatus::Known),
            "edge" => Ok(StrandStatus::Edge),
            "unknown" => Ok(StrandStatus::Unknown),
            "blocked" => Ok(StrandStatus::Blocked),
            other => Err(CoreError::InvalidFormat(format!(
                "estado de hebra desconocido: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse_roundtrip_all_variants() {
        for status in [
            StrandStatus::Known,
            StrandStatus::Edge,
            StrandStatus::Unknown,
            StrandStatus::Blocked,
        ] {
            let rendered = status.to_string();
            assert_eq!(rendered.parse::<StrandStatus>().unwrap(), status);
            assert_eq!(status, StrandStatus::from_str(&rendered).unwrap());
        }
    }

    #[test]
    fn parse_is_case_insensitive_and_trims() {
        assert_eq!("KNOWN".parse::<StrandStatus>().unwrap(), StrandStatus::Known);
        assert_eq!("  Edge ".parse::<StrandStatus>().unwrap(), StrandStatus::Edge);
        assert_eq!("unknown".parse::<StrandStatus>().unwrap(), StrandStatus::Unknown);
        assert_eq!("Blocked".parse::<StrandStatus>().unwrap(), StrandStatus::Blocked);
    }

    #[test]
    fn parse_rejects_unknown_status() {
        let err = "flotante".parse::<StrandStatus>().unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
    }
}
