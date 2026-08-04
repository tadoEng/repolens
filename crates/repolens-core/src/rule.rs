//! The analyzer rule contract.
//!
//! A rule reads evidence and returns findings. It has no I/O, no clock, and no
//! configuration beyond what it is handed, which is what makes a report
//! reproducible from the key in [`crate::reproducibility`]. Concretely: a rule
//! may not depend on `axum` or `sqlx`, and this crate's dependency list is what
//! enforces that.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finding::Finding;
use crate::repository::{CommitSha, RepositoryCoordinate};

/// Stable identifier for a rule, carried on every finding it produces.
///
/// Stable across releases: renaming one is a ruleset-version change, because
/// it changes what a report means.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    /// Names a rule.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Everything a rule is allowed to look at.
///
/// **PROVISIONAL.** Today it carries only repository identity. The tree, the
/// selected blobs, and the composition result join it as issues #4, #5, and
/// #12 produce them — deliberately grown from real rules rather than guessed
/// at now.
#[derive(Debug, Clone, Copy)]
pub struct RuleInput<'a> {
    /// Which repository is being analyzed.
    pub repository: &'a RepositoryCoordinate,
    /// The exact commit the evidence was read from.
    pub commit: &'a CommitSha,
}

/// A single analyzer rule.
pub trait AnalyzerRule {
    /// Identifier reported on every finding this rule emits.
    fn id(&self) -> RuleId;

    /// Evaluates the rule.
    ///
    /// Returning an empty vector means "this rule found nothing", which is not
    /// the same as "this rule could not run" — a rule that cannot run reports a
    /// limitation instead of silently returning nothing.
    fn evaluate(&self, input: &RuleInput<'_>) -> Vec<Finding>;
}
