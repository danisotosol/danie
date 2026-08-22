//! danie-engine: the tutor loop engine of the danie AI tutor.
//!
//! Drives one-to-one tutoring sessions over any [`danie_llm::LlmProvider`]:
//! diagnostic probing, prerequisite-ordered plan generation (Alvar method),
//! lesson writing, missing-prerequisite proposals and spaced-repetition
//! review questions. Pure library code — no I/O, no storage.

pub mod engine;
pub mod prompts;
pub mod protocol;

pub use engine::*;
