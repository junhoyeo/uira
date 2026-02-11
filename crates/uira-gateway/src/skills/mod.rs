//! Skill discovery, parsing, and loading for Uira.

mod discovery;
mod error;
mod loader;
mod parser;

pub use discovery::{discover_skills, SkillInfo};
pub use error::SkillError;
pub use loader::{get_context_injection, SkillLoader};
pub use parser::{Skill, SkillMeta, SkillMetadata};
