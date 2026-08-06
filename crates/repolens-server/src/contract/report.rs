//! The report, as the browser receives it.
//!
//! Two rules shape every type here, and both exist to stop the report
//! overclaiming:
//!
//! * **Severity and confidence are separate.** Severity is impact if the
//!   finding is valid; confidence is how strong the evidence is. Merging them
//!   into one badge would let a guess look like a certainty.
//! * **Absence is not proof of absence.** `MISSING` and `UNABLE_TO_VERIFY` are
//!   different findings, and `limitations` is first-class at both the report and
//!   the finding level.
//!
//! There is deliberately no overall score. A single number would be the one
//! thing every reader quoted, and no honest number exists.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use repolens_core::ContentDigest;

use super::analysis::RepositoryIdentity;

/// What the analyzer concluded about one checked property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingState {
    /// Observed directly in the repository.
    Detected,
    /// Described in documentation, but not observed in the repository itself.
    /// Distinct from `DETECTED` because a claim is not an implementation.
    Documented,
    /// Looked for and genuinely not present. The rule must explain why that
    /// matters; absence is not automatically a defect.
    Missing,
    /// The rule does not apply to this repository — no Rust code, so no Rust
    /// toolchain check. Not a pass and not a failure.
    NotApplicable,
    /// Could not be established from the evidence collected: a truncated tree,
    /// a skipped oversized file, an abandoned line count. **Never rendered as
    /// `MISSING`** — "we did not see it" is not "it is not there".
    UnableToVerify,
}

/// Impact if the finding is valid. Independent of [`Confidence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    /// Worth knowing; no action implied.
    Info,
    /// Worth a look during ordinary maintenance.
    Low,
    /// Worth scheduling.
    Medium,
    /// Worth attention before the next significant change.
    High,
}

/// Strength of the evidence. Independent of [`Severity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    /// Inferred from indirect signals; a reader should check.
    Low,
    /// Consistent with several signals.
    Medium,
    /// Directly observed in the repository at the analyzed commit.
    High,
}

/// Grouping for the engineering-system section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingCategory {
    /// Languages, frameworks, and libraries in use.
    Technology,
    /// Module boundaries and structure.
    Architecture,
    /// Source layout and documentation.
    SourceAndDocumentation,
    /// Build and dependency management.
    BuildAndDependencies,
    /// Automated tests.
    Testing,
    /// Continuous integration and delivery.
    CiCd,
    /// Runtime operability: health checks, migrations, deployment.
    Operations,
    /// Security posture and maintenance signals.
    SecurityAndMaintenance,
}

/// What kind of thing a piece of evidence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceKind {
    /// A file exists at a path.
    FilePresence,
    /// An excerpt from a file's contents.
    FileExcerpt,
    /// An entry in a dependency manifest.
    DependencyEntry,
    /// A CI workflow or one of its steps.
    WorkflowDefinition,
    /// A counted or measured fact.
    Statistic,
    /// Repository metadata from the provider.
    RepositoryMetadata,
}

/// A line span within a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LineRange {
    /// First line, 1-indexed.
    pub start: u32,
    /// Last line, inclusive.
    pub end: u32,
}

/// One checkable fact supporting a finding.
///
/// Every excerpt is truncated **server-side**. The frontend must never be the
/// thing that prevents a five-megabyte payload: by the time the browser could
/// decide, the bytes have already crossed the network and been parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Evidence {
    /// What sort of evidence this is.
    pub kind: EvidenceKind,
    /// Repository-relative path, when the evidence has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Short excerpt, already truncated to the server's cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Whether `excerpt` was cut short. Required so the UI can say "truncated"
    /// rather than implying the file ends there.
    pub truncated: bool,
    /// Digest of the **full** source content, not the excerpt — which is what
    /// makes the evidence checkable against the commit.
    ///
    /// Typed rather than a bare string so the format is owned in one place. The
    /// ingestion boundary produces it and this contract publishes it; two
    /// independent spellings would not surface until integration, and would
    /// surface as evidence that silently fails to match the commit it pins.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, pattern = "^sha256:[0-9a-f]{64}$")]
    pub digest: Option<ContentDigest>,
    /// Which lines the excerpt came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_range: Option<LineRange>,
}

/// Something the analyzer could not establish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Limitation {
    /// Stable code, so the UI can group and explain limitations consistently.
    pub code: String,
    /// What could not be established, and why.
    pub explanation: String,
}

/// One conclusion, with everything needed to check it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Finding {
    /// Stable identifier within this report.
    pub id: Uuid,
    /// Which rule produced this, e.g. `rust.workspace.detected`.
    pub rule_id: String,
    /// Ruleset that produced it. Carried per-finding as well as per-report so a
    /// stored finding stays interpretable after the report is regenerated.
    pub ruleset_version: String,
    /// Section this belongs to.
    pub category: FindingCategory,
    /// What the analyzer concluded.
    pub state: FindingState,
    /// Impact if valid. **Never merged with `confidence`.**
    pub severity: Severity,
    /// Evidence strength. **Never merged with `severity`.**
    pub confidence: Confidence,
    /// One-line summary.
    pub title: String,
    /// Prose explanation, including why it matters.
    pub explanation: String,
    /// Facts supporting the conclusion. May be empty for `UNABLE_TO_VERIFY`,
    /// which is precisely the case where there is nothing to show.
    pub evidence: Vec<Evidence>,
    /// What this finding does not establish.
    pub limitations: Vec<Limitation>,
    /// Suggested next step. Absent when the honest answer is "nothing to do".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_action: Option<String>,
}

/// Line counts for one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LanguageLineCount {
    /// Language name as the counter reports it.
    pub language: String,
    /// Files attributed to this language.
    pub files: u64,
    /// Lines of code, excluding comments and blanks. The headline number.
    pub code_lines: u64,
    /// Comment lines.
    pub comment_lines: u64,
    /// Blank lines.
    pub blank_lines: u64,
}

/// Line counts for one top-level area of the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AreaLineCount {
    /// Top-level path, e.g. `crates/` or `web/`.
    pub area: String,
    /// Lines of code in it.
    pub code_lines: u64,
}

/// Why some files were left out of the counts.
///
/// Structured rather than prose so the UI can make the ledger expandable. LOC
/// misleads exactly when nobody can see what was excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompositionExclusion {
    /// Path or glob that was excluded.
    pub path_or_rule: String,
    /// Why, in words.
    pub reason: String,
    /// Which policy rule matched, so the decision is traceable.
    pub matched_rule: String,
    /// How many files it covered.
    pub file_count: u64,
    /// How many bytes it covered.
    pub bytes: u64,
}

/// Ceiling on [`LineCountSummary::largest_files`].
///
/// Ten is a review aid, not a dataset: the section exists to point a reader at
/// where to look first, and a list long enough to scroll stops doing that.
pub const MAX_LARGEST_FILES: usize = 10;

/// Rejects an over-long `largest_files` array rather than silently truncating.
///
/// Truncating would make the server and the client disagree about what the
/// report says while both appeared to succeed. A producer that exceeds the bound
/// has a bug, and the contract should say so.
fn deserialize_largest_files<'de, D>(deserializer: D) -> Result<Vec<LargestSourceFile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;

    let files = Vec::<LargestSourceFile>::deserialize(deserializer)?;
    if files.len() > MAX_LARGEST_FILES {
        return Err(D::Error::custom(format!(
            "largest_files carries {} entries, more than the {MAX_LARGEST_FILES} the contract permits",
            files.len()
        )));
    }
    Ok(files)
}

/// How a counted file was classified by role.
///
/// Structural evidence only. This is **not** a test-quality score: a repository
/// with little test code may be thoroughly tested elsewhere, and a repository
/// with a great deal of it may test the wrong things.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodeRole {
    /// Ordinary implementation code.
    Production,
    /// Tests, fixtures, and test helpers.
    Test,
    /// Machine-produced and committed — a generated client, a schema snapshot.
    /// Separated because counting it as hand-written work overstates effort and
    /// understates how much of the repository is derived.
    Generated,
    /// Examples, benchmarks, and developer tooling.
    Tooling,
}

/// Line counts for one role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RoleLineCount {
    /// Which role.
    pub role: CodeRole,
    /// Files attributed to it.
    pub files: u64,
    /// Lines of code in it.
    pub code_lines: u64,
}

/// One of the largest files by line count.
///
/// Size alone is not a defect. It is a **review-priority signal**: a large file
/// that also concentrates several responsibilities is where a reader's
/// attention is best spent first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LargestSourceFile {
    /// Repository-relative path.
    pub path: String,
    /// Language the counter attributed it to.
    pub language: String,
    /// Lines of code, excluding comments and blanks.
    pub code_lines: u64,
    /// Role, so a large generated file is not mistaken for a large hand-written
    /// one — which is the most common way this list misleads.
    pub role: CodeRole,
}

/// Repository composition and line counts.
///
/// Measures composition, **not** productivity or quality. The report says so
/// visibly, because this is the easiest section to misread as a score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LineCountSummary {
    /// Counting tool, e.g. `tokei`.
    pub counter: String,
    /// Exact counter version — different versions count differently, so this is
    /// part of what makes a count reproducible.
    pub counter_version: String,
    /// Version of the exclusion policy applied.
    pub exclusion_policy_version: String,
    /// Files counted.
    pub total_files: u64,
    /// All physical lines.
    pub total_lines: u64,
    /// Lines of code.
    pub code_lines: u64,
    /// Comment lines.
    pub comment_lines: u64,
    /// Blank lines.
    pub blank_lines: u64,
    /// Per-language breakdown, server-ordered.
    pub languages: Vec<LanguageLineCount>,
    /// Per-area breakdown, server-ordered.
    pub areas: Vec<AreaLineCount>,
    /// What was left out, and why.
    pub exclusions: Vec<CompositionExclusion>,
    /// Breakdown by role, server-ordered.
    ///
    /// Present so the report can show what proportion of the repository is
    /// production code without implying a judgement about it.
    pub roles: Vec<RoleLineCount>,
    /// Largest files by line count, server-ordered, descending.
    ///
    /// Bounded at [`MAX_LARGEST_FILES`], published as `maxItems` and enforced on
    /// deserialization. A repository with 40,000 files must not send 40,000 rows
    /// to a browser to render a top-ten list, and a bound that is only described
    /// is a bound the first careless producer ignores.
    #[schema(max_items = 10)]
    #[serde(deserialize_with = "deserialize_largest_files")]
    pub largest_files: Vec<LargestSourceFile>,
    /// Files the policy could not classify. Reported rather than silently
    /// folded into a bucket.
    pub unclassified_files: u64,
}

/// An evidence-backed statement for the executive overview.
///
/// The overview carries the entire summarization load, because there is no
/// score to skim. Each statement therefore points at the findings that support
/// it rather than asserting on its own authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OverviewStatement {
    /// The statement itself.
    pub statement: String,
    /// Findings that support it, by `rule_id`.
    pub supporting_rule_ids: Vec<String>,
    /// Confidence in the statement as a whole.
    pub confidence: Confidence,
}

/// A complete report for one repository at one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Report {
    /// Analysis that produced it.
    pub analysis_id: Uuid,
    /// Repository analyzed.
    pub repository: RepositoryIdentity,
    /// Exact commit. Non-null here, unlike on the analysis: a report cannot
    /// exist without a resolved commit.
    pub commit_sha: String,
    /// Root tree the collectors walked. Part of the reproducibility key, since
    /// two commits sharing a tree yield identical evidence.
    pub tree_sha: String,
    /// Analyzer version that produced this report. First-class, not a footnote.
    pub analyzer_version: String,
    /// Ruleset version evaluated. First-class, not a footnote.
    pub ruleset_version: String,
    /// When the analysis completed.
    #[serde(with = "time::serde::rfc3339")]
    pub completed_at: OffsetDateTime,
    /// Evidence-backed summary statements.
    pub overview: Vec<OverviewStatement>,
    /// All findings, in a **server-decided order**.
    ///
    /// Ordering is part of the contract. A report that listed findings
    /// differently on each load would contradict the determinism it claims,
    /// and no client-side sort can restore an order the server never fixed.
    pub findings: Vec<Finding>,
    /// Composition and line counts.
    ///
    /// **Null when counting did not happen** — most often because extraction
    /// exceeded its configured limit. That is a designed `UNABLE_TO_VERIFY`
    /// outcome with a matching entry in `limitations`, not an error and not a
    /// zero. Required-but-nullable so a consumer must handle it.
    #[schema(required)]
    pub composition: Option<LineCountSummary>,
    /// What this report as a whole does not establish.
    ///
    /// Report-level, not merely per-finding, so "absence of evidence is not
    /// evidence of absence" stays visible in the overview rather than buried
    /// inside an expanded finding nobody opened.
    pub limitations: Vec<Limitation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_and_confidence_are_distinct_types() {
        // They are deliberately not interchangeable. If one enum served both,
        // "high impact" and "high certainty" would be the same value and the
        // separation the report promises would exist only in prose.
        assert_eq!(serde_json::to_string(&Severity::High).unwrap(), "\"HIGH\"");
        assert_eq!(serde_json::to_string(&Confidence::Low).unwrap(), "\"LOW\"");
    }

    #[test]
    fn finding_states_cover_the_documented_set() {
        for (state, expected) in [
            (FindingState::Detected, "\"DETECTED\""),
            (FindingState::Documented, "\"DOCUMENTED\""),
            (FindingState::Missing, "\"MISSING\""),
            (FindingState::NotApplicable, "\"NOT_APPLICABLE\""),
            (FindingState::UnableToVerify, "\"UNABLE_TO_VERIFY\""),
        ] {
            assert_eq!(serde_json::to_string(&state).unwrap(), expected);
        }
    }

    #[test]
    fn an_overlong_largest_files_array_is_rejected() {
        // The bound is published as maxItems and enforced here, so a producer
        // cannot quietly exceed it and a consumer cannot quietly accept it.
        let row = serde_json::json!({
            "path": "a.rs", "language": "Rust", "code_lines": 1, "role": "PRODUCTION"
        });
        let rows: Vec<_> = std::iter::repeat_n(row, MAX_LARGEST_FILES + 1).collect();
        let summary = serde_json::json!({
            "counter": "tokei", "counter_version": "14.0.0",
            "exclusion_policy_version": "1",
            "total_files": 1, "total_lines": 1, "code_lines": 1,
            "comment_lines": 0, "blank_lines": 0,
            "languages": [], "areas": [], "exclusions": [],
            "roles": [], "largest_files": rows, "unclassified_files": 0
        });

        let parsed = serde_json::from_value::<LineCountSummary>(summary);
        assert!(parsed.is_err(), "an over-long list must be rejected");
    }

    #[test]
    fn a_null_composition_is_serialized_rather_than_omitted() {
        // `UNABLE_TO_VERIFY` composition must be visible to the client as an
        // explicit null, not an absent key it can overlook.
        let report = Report {
            analysis_id: Uuid::nil(),
            repository: RepositoryIdentity {
                owner: "o".into(),
                name: "n".into(),
            },
            commit_sha: "0".repeat(40),
            tree_sha: "1".repeat(40),
            analyzer_version: "0.1.0".into(),
            ruleset_version: "1".into(),
            completed_at: OffsetDateTime::UNIX_EPOCH,
            overview: vec![],
            findings: vec![],
            composition: None,
            limitations: vec![],
        };

        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("composition").is_some());
        assert!(json["composition"].is_null());
    }
}
