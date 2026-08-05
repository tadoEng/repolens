//! Findings and the evidence that backs them.
//!
//! RepoLens publishes no universal score. Severity and confidence are kept as
//! two independent axes, because collapsing them is exactly how a "9.2/10
//! architecture" number gets invented: a low-confidence guess and a
//! high-confidence certainty must never average into one figure.

use serde::{Deserialize, Serialize};

use crate::ids::FindingId;
use crate::rule::RuleId;

/// How much this finding matters **if it is true**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    /// Worth knowing; implies no action.
    Info,
    /// Minor.
    Low,
    /// Meaningful.
    Medium,
    /// Serious.
    High,
}

/// How sure the analyzer is that the finding **is** true.
///
/// Never folded into [`Severity`], and never inferred from the amount of
/// evidence alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    /// Consistent with the evidence, but other explanations fit too.
    Low,
    /// Well supported, with a plausible alternative reading.
    Medium,
    /// Directly demonstrated by the cited evidence.
    High,
}

/// A bounded excerpt of something actually observed in the repository.
///
/// **PROVISIONAL.** The wire shape is fixed by the `report-v1` fixtures
/// (issue #14). What is already settled: the *server* enforces the excerpt cap
/// and reports it — the frontend must never be the thing that prevents a
/// multi-megabyte payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Repository-relative path the excerpt came from.
    pub path: String,
    /// The excerpt itself, already truncated to the server's cap.
    pub excerpt: String,
    /// Whether [`Evidence::excerpt`] is shorter than the source region.
    pub truncated: bool,
    /// Digest of the full untruncated source region, so a truncated excerpt
    /// remains verifiable against the repository.
    pub digest: String,
}

/// One claim, its severity, its confidence, and the evidence supporting it.
///
/// **PROVISIONAL** — see [`Evidence`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable identifier for deep-linking into a report.
    pub id: FindingId,
    /// Rule that produced this finding.
    pub rule: RuleId,
    /// One-line statement of what was found.
    pub title: String,
    /// Impact if true.
    pub severity: Severity,
    /// Certainty that it is true.
    pub confidence: Confidence,
    /// Supporting observations. A finding with no evidence is a defect.
    pub evidence: Vec<Evidence>,
}

#[cfg(test)]
mod tests {
    use super::{Confidence, Severity};

    #[test]
    fn severity_orders_from_info_to_high() {
        assert!(Severity::Info < Severity::Low);
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
    }

    #[test]
    fn severity_and_confidence_are_separate_axes() {
        // A high-severity, low-confidence finding must remain expressible;
        // if these ever collapse into one scale, this stops compiling.
        let (severity, confidence) = (Severity::High, Confidence::Low);
        assert_eq!(severity, Severity::High);
        assert_eq!(confidence, Confidence::Low);
    }

    #[test]
    fn enums_serialize_as_stable_screaming_snake_case() {
        let json = serde_json::to_string(&Severity::High).expect("serializes");
        assert_eq!(json, "\"HIGH\"");
    }
}
