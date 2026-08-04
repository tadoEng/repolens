//! Repository composition — the normalized result of counting lines of code.
//!
//! Counting is not GitHub-specific, so it does not live in `repolens-github`.
//! Binding the two would make a future local-folder or uploaded-archive
//! analyzer conceptually depend on the GitHub adapter. The trait lives here;
//! the hardened extraction and the Tokei adapter that implement it live in
//! `repolens_server::infrastructure::composition`. Analyzer rules depend on
//! *counts*, never on Tokei.
//!
//! GitHub's own language endpoint reports **bytes, not lines**, which is why it
//! cannot answer this question at all.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Counts for one detected language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageComposition {
    /// Language name as reported by the counter.
    pub language: String,
    /// Files attributed to this language.
    pub files: u64,
    /// Lines of code.
    pub code: u64,
    /// Comment lines.
    pub comments: u64,
    /// Blank lines.
    pub blanks: u64,
}

/// The normalized composition of a repository at one commit.
///
/// **PROVISIONAL** in shape; the exclusion ledger is not. LOC is the easiest
/// number in a report to misread, and it is usually wrong because of what was
/// silently left out — so what was excluded, and under which rule, is
/// first-class data rather than a footnote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryComposition {
    /// Per-language counts over the counted set only.
    pub languages: Vec<LanguageComposition>,
    /// Files that were counted.
    pub counted_files: u64,
    /// Paths deliberately left out, each with the rule that matched.
    pub exclusions: Vec<CompositionExclusion>,
}

/// One entry in the exclusion ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionExclusion {
    /// The path, or the glob/rule expression, that was excluded.
    pub path_or_rule: String,
    /// Human-readable justification.
    pub reason: String,
    /// Identifier of the rule that matched, for auditability.
    pub matched_rule: String,
    /// How many files this exclusion removed.
    pub file_count: u64,
    /// How many bytes this exclusion removed.
    pub bytes: u64,
}

/// A limit that stopped counting before it could complete.
///
/// Recorded with both the configured limit and the value actually observed,
/// so a report can say *why* it cannot answer rather than merely that it
/// cannot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionLimitBreach {
    /// Which control tripped, for example the extraction storage limit.
    pub limit_name: String,
    /// The configured ceiling.
    pub limit_value: u64,
    /// The value that exceeded it.
    pub observed_value: u64,
}

/// Composition may legitimately be absent.
///
/// An archive that exceeds a configured extraction limit yields
/// [`CompositionOutcome::UnableToVerify`] — a designed state that the report
/// renders honestly, not an error and not a zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositionOutcome {
    /// Counting completed within every limit.
    Counted(RepositoryComposition),
    /// Counting was abandoned; the report says so and explains which ceiling
    /// was hit.
    UnableToVerify(CompositionLimitBreach),
}

/// Counts the composition of an already-extracted working tree.
///
/// Synchronous on purpose: counting is CPU-bound, and the async boundary
/// belongs to whoever schedules it (`spawn_blocking` in the worker), not to
/// the domain contract.
pub trait RepositoryCompositionCounter {
    /// Failure mode of the concrete counter.
    type Error;

    /// Counts everything under `root` that the exclusion policy admits.
    fn count_composition(&self, root: &Path) -> Result<CompositionOutcome, Self::Error>;
}
