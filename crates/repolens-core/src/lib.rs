//! RepoLens domain model.
//!
//! This crate answers "what is an analysis, and what may it claim?" It knows
//! nothing about HTTP, PostgreSQL, GitHub, or Tokei, and it must stay that way:
//! an analyzer rule that could reach a socket or a connection pool could no
//! longer be reasoned about as a pure function of the evidence it was given.
//!
//! Forbidden dependencies: `axum`, `sqlx`, `reqwest`, any async runtime.
//!
//! # Scope today
//!
//! Types marked **PROVISIONAL** exist to express a boundary, not to fix a wire
//! format. The analysis and report DTO shapes are owned by the executable
//! fixtures in `contracts/fixtures/` (issue #14) and will be derived from them,
//! not from this crate.

pub mod composition;
pub mod finding;
pub mod ids;
pub mod repository;
pub mod reproducibility;
pub mod rule;

pub use composition::{
    CompositionLimitBreach, CompositionOutcome, LanguageComposition, RepositoryComposition,
    RepositoryCompositionCounter,
};
pub use finding::{Confidence, Evidence, Finding, Severity};
pub use ids::{AnalysisId, FindingId, SnapshotId};
pub use repository::{CommitSha, CommitShaError, RepositoryCoordinate, TreeSha};
pub use reproducibility::{CompositionCounter, EvidenceSource, ReproducibilityKey};
pub use rule::{AnalyzerRule, RuleId, RuleInput};
