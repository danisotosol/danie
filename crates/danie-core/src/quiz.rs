//! Lock-in quiz support: SM-2 spaced repetition cards and quiz questions.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// A spaced-repetition card scheduled with the SM-2 algorithm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SrsCard {
    /// Identifier of the plan node this card quizzes.
    pub node: String,
    /// SM-2 easiness factor (minimum 1.3).
    pub ease: f64,
    /// Current interval in days between reviews.
    pub interval_days: u32,
    /// Next due date.
    pub due: DateTime<Utc>,
    /// Number of consecutive successful reviews.
    pub reps: u32,
    /// Number of times the card was forgotten.
    pub lapses: u32,
}

impl SrsCard {
    /// Creates a new card for `node`, due immediately.
    pub fn new(node: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            ease: 2.5,
            interval_days: 0,
            due: Utc::now(),
            reps: 0,
            lapses: 0,
        }
    }

    /// Applies an SM-2 review with quality `quality` in `0..=5`.
    ///
    /// Qualities below 3 count as a lapse and reset the card to a one-day
    /// interval; qualities of 3 or more grow the interval. The ease factor is
    /// always adjusted and clamped to a minimum of 1.3.
    pub fn review(&mut self, quality: u8) -> Result<()> {
        if quality > 5 {
            return Err(CoreError::InvalidFormat(format!(
                "review quality out of range 0..=5: {quality}"
            )));
        }
        let q = f64::from(quality);
        self.ease += 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02);
        if self.ease < 1.3 {
            self.ease = 1.3;
        }
        if quality >= 3 {
            self.interval_days = match self.reps {
                0 => 1,
                1 => 6,
                _ => ((self.interval_days as f64) * self.ease).round() as u32,
            };
            self.reps += 1;
        } else {
            self.lapses += 1;
            self.reps = 0;
            self.interval_days = 1;
        }
        self.due = Utc::now() + Duration::days(i64::from(self.interval_days));
        Ok(())
    }

    /// Returns true when the card is due at `now`.
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        now >= self.due
    }
}

/// The learner's full spaced-repetition queue.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SrsQueue {
    pub cards: Vec<SrsCard>,
}

impl SrsQueue {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a fresh card for `node` if none exists yet; otherwise keeps the
    /// existing scheduling state.
    pub fn upsert_card(&mut self, node: impl Into<String>) {
        let node = node.into();
        if !self.cards.iter().any(|c| c.node == node) {
            self.cards.push(SrsCard::new(node));
        }
    }

    /// Returns all cards due at `now`, sorted by due date ascending.
    pub fn due_cards(&self, now: DateTime<Utc>) -> Vec<&SrsCard> {
        let mut due: Vec<&SrsCard> = self.cards.iter().filter(|c| c.is_due(now)).collect();
        due.sort_by_key(|c| c.due);
        due
    }

    /// Serializes the queue as pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(CoreError::from)
    }

    /// Deserializes a queue from pretty JSON produced by [`SrsQueue::to_json`].
    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(CoreError::from)
    }
}

/// One multiple-choice lock-in question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
}

impl QuizQuestion {
    /// Checks that `correct_index` points at an existing option.
    pub fn validate(&self) -> Result<()> {
        if self.correct_index >= self.options.len() {
            return Err(CoreError::InvalidFormat(format!(
                "correct answer index out of range: {} of {} options",
                self.correct_index,
                self.options.len()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sm2_progression_is_deterministic() {
        let mut card = SrsCard::new("monadas");
        card.review(4).unwrap();
        assert_eq!(card.interval_days, 1);
        assert_eq!(card.reps, 1);
        card.review(4).unwrap();
        assert_eq!(card.interval_days, 6);
        assert_eq!(card.reps, 2);
        assert!((card.ease - 2.5).abs() < 1e-9);
        card.review(5).unwrap();
        assert!((card.ease - 2.6).abs() < 1e-9);
        assert_eq!(card.interval_days, (6_f64 * 2.6).round() as u32);
        assert_eq!(card.interval_days, 16);
    }

    #[test]
    fn low_quality_lapses_and_resets() {
        let mut card = SrsCard::new("tipos");
        card.review(4).unwrap();
        card.review(4).unwrap();
        card.review(2).unwrap();
        assert_eq!(card.lapses, 1);
        assert_eq!(card.reps, 0);
        assert_eq!(card.interval_days, 1);
        assert!((card.ease - (2.5 - 0.32)).abs() < 1e-9);
    }

    #[test]
    fn ease_never_drops_below_floor() {
        let mut card = SrsCard::new("recursion");
        for _ in 0..12 {
            card.review(0).unwrap();
        }
        assert_eq!(card.ease, 1.3);
        assert_eq!(card.interval_days, 1);
    }

    #[test]
    fn quality_above_five_is_rejected() {
        let mut card = SrsCard::new("punteros");
        let err = card.review(6).unwrap_err();
        assert!(matches!(err, CoreError::InvalidFormat(_)));
    }

    #[test]
    fn due_cards_are_sorted_and_filtered() {
        let now = Utc::now();
        let mut queue = SrsQueue::new();
        queue.upsert_card("tarde");
        queue.upsert_card("temprano");
        queue.upsert_card("futuro");
        queue.cards[0].due = now + Duration::days(2);
        queue.cards[1].due = now - Duration::days(1);
        queue.cards[2].due = now + Duration::days(10);

        let due = queue.due_cards(now);
        let names: Vec<&str> = due.iter().map(|c| c.node.as_str()).collect();
        assert_eq!(names, vec!["temprano"]);
        assert!(queue.cards[0].is_due(now + Duration::days(2)));
        assert!(!queue.cards[0].is_due(now));
    }

    #[test]
    fn queue_json_roundtrip() {
        let mut queue = SrsQueue::new();
        queue.upsert_card("variables");
        let json = queue.to_json().unwrap();
        assert_eq!(SrsQueue::from_json(&json).unwrap(), queue);
    }

    #[test]
    fn question_validates_correct_index_bounds() {
        let ok = QuizQuestion {
            id: "q1".into(),
            prompt: "What does fmap do?".into(),
            options: vec!["maps".into(), "filters".into()],
            correct_index: 0,
            explanation: "fmap applies the function".into(),
        };
        assert!(ok.validate().is_ok());
        let bad = QuizQuestion {
            correct_index: 2,
            ..ok
        };
        assert!(matches!(bad.validate(), Err(CoreError::InvalidFormat(_))));
    }
}
