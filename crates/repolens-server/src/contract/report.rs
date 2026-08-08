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

/// Ceiling on [`LargestSourceFiles`].
///
/// Ten is a review aid, not a dataset: the section exists to point a reader at
/// where to look first, and a list long enough to scroll stops doing that.
pub const MAX_LARGEST_FILES: usize = 10;

/// Too many rows were offered for the bounded list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "largest_files carries {offered} entries, more than the {MAX_LARGEST_FILES} the contract permits"
)]
pub struct TooManyLargestFiles {
    /// How many were offered.
    pub offered: usize,
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
    /// The classifier recognised nothing about this path.
    ///
    /// Not a category of code — a statement about the analyzer. Roles are
    /// decided from paths alone, and a repository laid out in a way the policy
    /// has no rule for is one this product cannot classify rather than one that
    /// is full of ordinary implementation code.
    ///
    /// Folding these into [`CodeRole::Production`] is what an earlier revision
    /// did, and it makes the production share read as far more certain than the
    /// classifier is. That is the same conflation as reporting `MISSING` for a
    /// file nobody opened, one level up.
    Unclassified,
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

/// A bounded list of the largest source files.
///
/// A newtype rather than a plain `Vec` because the bound has to hold in the
/// direction that actually matters. RepoLens *produces* this DTO far more often
/// than it consumes one, and a `Vec` field with a `maxItems` annotation lets
/// server code build forty thousand rows and serialize a response that violates
/// its own published contract — while every deserialization test still passes.
///
/// The inner value is private, so the only ways in are validated.
/// The published `maxItems` lives on the field in [`LineCountSummary`], because
/// utoipa accepts that attribute on fields rather than on newtype structs. The
/// two values are tied by `published_max_items_matches_the_enforced_bound`, so
/// they cannot drift into disagreeing about the same limit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<LargestSourceFile>")]
pub struct LargestSourceFiles(Vec<LargestSourceFile>);

impl LargestSourceFiles {
    /// Accepts a list that already respects the bound.
    ///
    /// # Errors
    ///
    /// Returns [`TooManyLargestFiles`] when the list is too long. Rejecting
    /// rather than silently truncating: a producer that exceeds the bound has a
    /// bug, and quietly shortening its output would make the server and the
    /// client disagree about what the report says while both appeared to
    /// succeed.
    pub fn new(files: Vec<LargestSourceFile>) -> Result<Self, TooManyLargestFiles> {
        if files.len() > MAX_LARGEST_FILES {
            return Err(TooManyLargestFiles {
                offered: files.len(),
            });
        }
        Ok(Self(files))
    }

    /// Takes the first [`MAX_LARGEST_FILES`] entries of an already-sorted list.
    ///
    /// The supported path for an analyzer that has ranked every file and wants
    /// the head of that ranking. Truncation is explicit here — the caller asked
    /// for it — which is what distinguishes it from the silent shortening
    /// [`new`](Self::new) refuses to do.
    #[must_use]
    pub fn truncated_from(mut files: Vec<LargestSourceFile>) -> Self {
        files.truncate(MAX_LARGEST_FILES);
        Self(files)
    }

    /// Borrows the rows.
    #[must_use]
    pub fn as_slice(&self) -> &[LargestSourceFile] {
        &self.0
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many rows there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl TryFrom<Vec<LargestSourceFile>> for LargestSourceFiles {
    type Error = TooManyLargestFiles;

    fn try_from(files: Vec<LargestSourceFile>) -> Result<Self, Self::Error> {
        Self::new(files)
    }
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
    /// Version of the policy that decided each file's role and area.
    ///
    /// Separate from the exclusion policy because the two answer different
    /// questions — what was left out, and what the rest *is* — and either can
    /// change without the other. Both are needed before two composition
    /// results may be compared: a changed classifier moves the production
    /// share without any file changing.
    pub classification_policy_version: String,
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
    /// Bounded in both directions by [`LargestSourceFiles`]: a producer cannot
    /// construct an over-long list, and an over-long one cannot be parsed.
    #[schema(value_type = Vec<LargestSourceFile>, max_items = 10)]
    pub largest_files: LargestSourceFiles,
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

/// Fields that record *this execution* rather than what was analyzed.
///
/// Named once, here, so that the determinism claim and the test that enforces
/// it cannot drift apart by editing one and not the other.
const EXECUTION_METADATA_FIELDS: [&str; 2] = ["analysis_id", "completed_at"];

/// Limitation codes whose occurrence is decided by the repository, under a
/// fixed set of versioned policies.
///
/// An allowlist, and the direction matters. Eligibility is granted only to
/// codes named here; anything else — including a code added tomorrow — makes a
/// report ineligible. The opposite default is fail-open: a new transient
/// condition would silently be treated as reproducible, and the first evidence
/// of the mistake would be two reports that disagree with nothing to explain
/// why.
///
/// Every entry is a claim that the same commit produces the same outcome. A
/// tree that exceeds the traversal bound exceeds it every time; a file over the
/// per-blob cap is over it every time; an archive holding more than the entry
/// limit holds more than it every time.
///
/// The claim has to survive one further question: *is the quantity measured
/// against the repository, or against the representation it arrived in?* Both
/// archive stream ceilings failed that question and sit in
/// [`RUNTIME_DEPENDENT_LIMITATIONS`] instead.
const REPRODUCIBLE_LIMITATIONS: [&str; 13] = [
    // Bounds applied to the repository, decided by its own shape.
    "TREE_TRUNCATED",
    "BOUNDED_FILE_SELECTION",
    // Selection outcomes. Each is a property of the tree under a fixed
    // selection policy, which is why that policy is versioned in the key.
    "FILE_SKIPPED_NOT_IN_TREE",
    "FILE_SKIPPED_NOT_A_FILE",
    "FILE_SKIPPED_TOO_LARGE",
    "FILE_SKIPPED_BINARY",
    "FILE_SKIPPED_UNDECODABLE",
    "FILE_SKIPPED_BUDGET_SPENT",
    "FILE_SKIPPED_SELECTION_FULL",
    // Content outcomes that are properties of the bytes, not of the fetch.
    "FILE_NOT_DECODABLE",
    "FILE_TRUNCATED",
    // One archive entry per path in the tree, so this counts the repository
    // rather than the packaging: a commit with more paths than the walk accepts
    // has more than it every time.
    "ARCHIVE_ENTRY_COUNT_LIMIT",
    // The entry's own declared size — the file's bytes, not the archive's.
    // Ends the run rather than skipping the file, deliberately: a count that
    // quietly means "counted, minus whatever we choked on" is the failure LOC
    // reporting is most prone to.
    "ARCHIVE_FILE_SIZE_LIMIT",
];

/// Codes this build knows to be decided by something other than the
/// repository: the machine, the network, or the shape of the archive GitHub
/// delivered.
///
/// Not consulted to decide eligibility — [`REPRODUCIBLE_LIMITATIONS`] does
/// that on its own — but to tell a reader *why* a report is ineligible. An
/// unrecognised code is also ineligible, and saying so honestly ("this build
/// has not classified it") is different from claiming to know it is transient.
const RUNTIME_DEPENDENT_LIMITATIONS: [&str; 6] = [
    // A retrieval that failed this time and may succeed next time. This one
    // predates composition: the pipeline deliberately continues without
    // contents rather than failing the analysis, so a completed report already
    // had a transient path before any wall-clock ceiling existed.
    "CONTENTS_NOT_COLLECTED",
    "FILE_NOT_RETRIEVED",
    // Host conditions.
    "ARCHIVE_DURATION_LIMIT",
    "EXTRACTION_STORAGE_LIMIT",
    // Both archive stream ceilings, and for one reason: each is measured
    // against GitHub's archive *representation* rather than against repository
    // content, and GitHub guarantees neither is byte-stable for a fixed commit.
    //
    // The compressed one is whatever gzip emitted. The decompressed one is
    // counted between the decoder and the tar parser, so tar headers and block
    // padding are inside the figure — it is the right control for a
    // decompression bomb, which is about what the machine must hold, and it is
    // not a count of the repository's own bytes. An archive near either ceiling
    // can legitimately fall on both sides of it across two runs of one commit.
    "ARCHIVE_COMPRESSED_LIMIT",
    "ARCHIVE_DECOMPRESSED_LIMIT",
];

/// Why a report may not be compared byte for byte with another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IneligibilityReason {
    /// A condition this build knows to depend on something other than the
    /// repository: the machine, the network, or the archive representation the
    /// evidence arrived in.
    RuntimeDependent,
    /// A limitation code this build does not classify. Ineligible by default:
    /// an unknown condition is not evidence of reproducibility.
    Unclassified,
}

/// Whether two reports sharing a reproducibility key may be compared byte for
/// byte.
///
/// Explicit rather than implied by a `None`, and deliberately *not* implemented
/// by removing the offending evidence from the payload: deleting the fields
/// that record a timeout would make a timed-out run and a successful one look
/// comparable by erasing the difference between them. The payload keeps
/// everything; this says whether comparing it means anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonEligibility {
    /// Every limitation is a property of the repository under fixed versioned
    /// limits, so another run with the same key must produce the same payload.
    Eligible,
    /// Something in this report was not decided by the repository alone.
    Ineligible {
        /// The limitation code that made it so.
        code: String,
        /// Whether this build recognises that code as transient, or simply does
        /// not classify it.
        reason: IneligibilityReason,
    },
}

impl ComparisonEligibility {
    /// Whether a determinism comparison over this report proves anything.
    #[must_use]
    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible)
    }
}

impl Report {
    /// Whether this report may be compared byte for byte with another sharing
    /// its reproducibility key.
    ///
    /// Decided from the limitations the report actually carries, not from
    /// whether `composition` is `null`. A tree that exceeds the traversal bound
    /// exceeds it every time, so that outcome is as reproducible as a complete
    /// walk. Treating every `UNABLE_TO_VERIFY` as non-reproducible would
    /// discard the deterministic majority of them.
    ///
    /// **Fails closed.** Only codes on the allowlist keep a report eligible;
    /// anything unrecognised makes it ineligible and says so. A limitation
    /// added later is therefore conservative until somebody classifies it,
    /// which is the safe direction for a claim of determinism.
    ///
    /// Finding-level limitations count too. A single blob that failed to
    /// retrieve turns one finding into `UNABLE_TO_VERIFY` without any
    /// report-level limitation, and two runs then differ for a reason the
    /// repository did not decide.
    #[must_use]
    pub fn comparison_eligibility(&self) -> ComparisonEligibility {
        let finding_limitations = self
            .findings
            .iter()
            .flat_map(|finding| finding.limitations.iter());

        self.limitations
            .iter()
            .chain(finding_limitations)
            .find(|limitation| !REPRODUCIBLE_LIMITATIONS.contains(&limitation.code.as_str()))
            .map_or(ComparisonEligibility::Eligible, |limitation| {
                let reason = if RUNTIME_DEPENDENT_LIMITATIONS.contains(&limitation.code.as_str()) {
                    IneligibilityReason::RuntimeDependent
                } else {
                    IneligibilityReason::Unclassified
                };
                ComparisonEligibility::Ineligible {
                    code: limitation.code.clone(),
                    reason,
                }
            })
    }

    /// The part of this report that two runs over the same inputs must agree
    /// on, byte for byte.
    ///
    /// Determinism is the property this product rests on, but it does not hold
    /// for the *whole* wire report and never could: every run receives a fresh
    /// `analysis_id`, and `completed_at` is read from the clock. Claiming
    /// byte-identical reports would therefore be false as stated, which is
    /// exactly the kind of overstatement the evidence contract exists to
    /// prevent — so the claim is bounded by executable code rather than by
    /// prose that the next reader has to trust.
    ///
    /// What remains is a function of the reproducibility key **for runs that
    /// completed within their resource limits**, and must not vary between
    /// those runs.
    ///
    /// That qualifier is load-bearing and was missing while it was harmless:
    /// composition runs under a wall-clock ceiling, so two runs with identical
    /// semantic inputs can legitimately produce different payloads when one
    /// crosses it on a loaded host. Ask [`Report::comparison_eligibility`]
    /// before treating a mismatch as a defect.
    ///
    /// Ineligible reports are **not** stripped of the evidence that made them
    /// ineligible. Removing the timeout limitation to make the payloads match
    /// would delete exactly the field recording that the two runs differed —
    /// manufacturing agreement rather than establishing it.
    ///
    /// Built by removing fields from the serialized report rather than by
    /// listing the ones to keep: a field added to [`Report`] then enters this
    /// payload automatically and is held to determinism by default. The
    /// opposite default would let a new nondeterministic field pass unnoticed.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json` error if the report cannot be
    /// serialized.
    pub fn analytical_payload(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        if let Some(object) = value.as_object_mut() {
            for field in EXECUTION_METADATA_FIELDS {
                object.remove(field);
            }
        }
        Ok(value)
    }
}

/// A report with no findings, for tests that care about one field.
///
/// Module-level rather than inside `mod tests` so `infrastructure` can use it:
/// its limits test compares its own classification against
/// [`Report::comparison_eligibility`], and a second copy of this literal is
/// exactly the kind of duplicate definition that drifts.
#[cfg(test)]
pub(crate) fn minimal_report() -> Report {
    Report {
        analysis_id: Uuid::nil(),
        repository: RepositoryIdentity {
            owner: "rust-lang".into(),
            name: "crates.io".into(),
        },
        commit_sha: "0".repeat(40),
        tree_sha: "1".repeat(40),
        analyzer_version: "1".into(),
        ruleset_version: "1".into(),
        completed_at: OffsetDateTime::UNIX_EPOCH,
        overview: vec![],
        findings: vec![],
        composition: None,
        limitations: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limited(codes: &[&str]) -> Report {
        let mut report = minimal_report();
        report.limitations = codes
            .iter()
            .map(|code| Limitation {
                code: (*code).to_owned(),
                explanation: "fixture".to_owned(),
            })
            .collect();
        report
    }

    #[test]
    fn a_transient_content_failure_is_not_reproducible() {
        /*
         * The path that existed before composition did. `collect_selected_blobs`
         * failing does not fail the analysis — the pipeline continues without
         * contents, content rules report UNABLE_TO_VERIFY, and the report says
         * CONTENTS_NOT_COLLECTED.
         *
         * So two runs over the same commit, same tree and same versions can
         * differ: one has content-backed findings, the other does not. Calling
         * both eligible would make a correct outcome look like a defect, and
         * would have done so before any wall-clock ceiling existed.
         */
        let eligibility = limited(&["CONTENTS_NOT_COLLECTED"]).comparison_eligibility();

        assert!(!eligibility.is_eligible());
        assert_eq!(
            eligibility,
            ComparisonEligibility::Ineligible {
                code: "CONTENTS_NOT_COLLECTED".to_owned(),
                reason: IneligibilityReason::RuntimeDependent,
            }
        );
    }

    #[test]
    fn an_unclassified_limitation_makes_a_report_ineligible() {
        // Fail closed. A code this build does not know is not evidence of
        // reproducibility, and the honest answer is "unclassified" rather than
        // a claim that it is transient.
        let eligibility = limited(&["SOMETHING_ADDED_LATER"]).comparison_eligibility();

        assert_eq!(
            eligibility,
            ComparisonEligibility::Ineligible {
                code: "SOMETHING_ADDED_LATER".to_owned(),
                reason: IneligibilityReason::Unclassified,
            },
            "an unrecognised limitation must not silently mean reproducible"
        );
    }

    #[test]
    fn ordinary_bounded_limitations_stay_eligible() {
        // The common case, and the reason this is an allowlist rather than a
        // blanket "any limitation means non-reproducible": a truncated tree and
        // a skipped oversized file are properties of the repository, and a
        // report carrying them is as comparable as one carrying none.
        assert!(
            limited(&[
                "TREE_TRUNCATED",
                "BOUNDED_FILE_SELECTION",
                "FILE_SKIPPED_TOO_LARGE",
                "ARCHIVE_ENTRY_COUNT_LIMIT",
            ])
            .comparison_eligibility()
            .is_eligible()
        );
    }

    #[test]
    fn an_archive_stream_ceiling_is_not_a_measurement_of_the_repository() {
        /*
         * Both stream ceilings were once treated as properties of the archive's
         * content, and the decompressed one survived a round of review that way
         * because "512 MiB of decoded bytes" reads like a fact about the
         * repository. It is not. The counter sits between the gzip decoder and
         * the tar parser, so what it measures is GitHub's tar framing —
         * headers, block padding, entry order — for which there is no
         * byte-stability guarantee, exactly as there is none for the compressed
         * length.
         *
         * Fail-closed classification decided *which set* a code belongs to. It
         * cannot tell whether the quantity being classified is canonical, and
         * that question has to be asked at the point the measurement is taken.
         */
        for code in ["ARCHIVE_COMPRESSED_LIMIT", "ARCHIVE_DECOMPRESSED_LIMIT"] {
            assert_eq!(
                limited(&[code]).comparison_eligibility(),
                ComparisonEligibility::Ineligible {
                    code: code.to_owned(),
                    reason: IneligibilityReason::RuntimeDependent,
                },
                "{code} is measured against the archive representation, not the repository"
            );
        }
    }

    #[test]
    fn a_finding_level_transient_limitation_also_makes_a_report_ineligible() {
        // A single blob that failed to retrieve turns one finding into
        // UNABLE_TO_VERIFY without any report-level limitation at all. Scanning
        // only `Report::limitations` would call that pair of runs comparable.
        let mut report = minimal_report();
        report.findings = vec![Finding {
            id: Uuid::nil(),
            rule_id: "rule".to_owned(),
            ruleset_version: "1".to_owned(),
            category: FindingCategory::Technology,
            state: FindingState::UnableToVerify,
            severity: Severity::Info,
            confidence: Confidence::Low,
            title: "t".to_owned(),
            explanation: "e".to_owned(),
            evidence: vec![],
            limitations: vec![Limitation {
                code: "FILE_NOT_RETRIEVED".to_owned(),
                explanation: "fixture".to_owned(),
            }],
            recommended_action: None,
        }];

        assert!(
            !report.comparison_eligibility().is_eligible(),
            "a transient failure inside a finding is still a transient failure"
        );
    }

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

    fn sample_row() -> LargestSourceFile {
        LargestSourceFile {
            path: "a.rs".to_owned(),
            language: "Rust".to_owned(),
            code_lines: 1,
            role: CodeRole::Production,
        }
    }

    #[test]
    fn a_producer_cannot_construct_an_overlong_list() {
        // The direction that actually matters: RepoLens builds this DTO far more
        // often than it parses one. A plain Vec with a maxItems annotation would
        // let server code emit forty thousand rows while every deserialization
        // test still passed.
        let too_many = vec![sample_row(); MAX_LARGEST_FILES + 1];
        assert!(LargestSourceFiles::new(too_many).is_err());

        let exact = vec![sample_row(); MAX_LARGEST_FILES];
        assert!(LargestSourceFiles::new(exact).is_ok());
    }

    #[test]
    fn truncation_is_available_but_must_be_asked_for() {
        // An analyzer that ranked every file wants the head of that ranking.
        // Explicit truncation is fine; silent truncation inside `new` would make
        // server and client disagree while both appeared to succeed.
        let ranked = vec![sample_row(); 40_000];
        let bounded = LargestSourceFiles::truncated_from(ranked);
        assert_eq!(bounded.len(), MAX_LARGEST_FILES);
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

    /// A report with every analytical field fixed, so only the execution
    /// metadata is left free to vary.
    fn report_with_execution(analysis_id: Uuid, completed_at: OffsetDateTime) -> Report {
        Report {
            analysis_id,
            repository: RepositoryIdentity {
                owner: "rust-lang".into(),
                name: "crates.io".into(),
            },
            commit_sha: "0".repeat(40),
            tree_sha: "1".repeat(40),
            analyzer_version: "0.1.0".into(),
            ruleset_version: "1".into(),
            completed_at,
            overview: vec![],
            findings: vec![],
            composition: None,
            limitations: vec![],
        }
    }

    #[test]
    fn execution_metadata_is_what_stops_whole_reports_from_being_identical() {
        // Establishes the premise of the next test rather than asserting a
        // behaviour worth having: if these two ever serialized identically, the
        // payload below would be proving nothing.
        let first = report_with_execution(Uuid::nil(), OffsetDateTime::UNIX_EPOCH);
        let second = report_with_execution(
            Uuid::from_u128(1),
            OffsetDateTime::from_unix_timestamp(1_785_873_497).unwrap(),
        );

        assert_ne!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&second).unwrap()
        );
    }

    #[test]
    fn two_runs_over_the_same_inputs_agree_on_the_analytical_payload() {
        // The determinism claim, stated exactly as far as it holds. The whole
        // wire report cannot be byte-identical across runs — a fresh
        // `analysis_id` and a clock read guarantee it differs — so what is
        // promised is that everything derived from the reproducibility key
        // agrees.
        let first = report_with_execution(Uuid::nil(), OffsetDateTime::UNIX_EPOCH);
        let second = report_with_execution(
            Uuid::from_u128(1),
            OffsetDateTime::from_unix_timestamp(1_785_873_497).unwrap(),
        );

        assert_eq!(
            first.analytical_payload().unwrap(),
            second.analytical_payload().unwrap(),
            "two runs differing only in execution metadata must agree"
        );
    }

    #[test]
    fn the_payload_drops_execution_metadata_and_keeps_everything_else() {
        let payload = report_with_execution(Uuid::nil(), OffsetDateTime::UNIX_EPOCH)
            .analytical_payload()
            .unwrap();
        let object = payload.as_object().expect("a report is a JSON object");

        for field in EXECUTION_METADATA_FIELDS {
            assert!(!object.contains_key(field), "{field} must not be compared");
        }
        // The reproducibility key itself must survive. A payload that dropped
        // these would compare equal for two genuinely different analyses, which
        // is a worse failure than comparing unequal for the same one.
        for field in [
            "repository",
            "commit_sha",
            "tree_sha",
            "analyzer_version",
            "ruleset_version",
        ] {
            assert!(object.contains_key(field), "{field} must be compared");
        }
    }

    #[test]
    fn a_changed_analytical_field_changes_the_payload() {
        // The direction that catches a payload which silently drops too much.
        let baseline = report_with_execution(Uuid::nil(), OffsetDateTime::UNIX_EPOCH);
        let mut different = report_with_execution(Uuid::nil(), OffsetDateTime::UNIX_EPOCH);
        different.ruleset_version = "2".into();

        assert_ne!(
            baseline.analytical_payload().unwrap(),
            different.analytical_payload().unwrap()
        );
    }
}
