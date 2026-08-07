//! One analysis run, end to end.
//!
//! Generic over [`GitHubRepositorySource`] rather than taking the concrete
//! client, so the whole pipeline can be exercised against a fake without a
//! network or a token. That is the only reason for the generic; nothing here
//! is meant to be swapped in production.
//!
//! # Execution is inline, and that is temporary
//!
//! [`run`] is spawned as a task from the create handler rather than claimed by
//! a worker through a durable queue. It is the shortest path to a demo that
//! analyses a real repository, and it is deliberately *not* the architecture:
//! issue #7 replaces it with a Cloud Run Job claiming a PostgreSQL lease.
//!
//! What that costs today, stated so it is not discovered later: an analysis
//! dies with the process. A deploy mid-run leaves a row in `ANALYZING` that
//! nothing will ever move, and there is no lease to expire and no recovery to
//! reclaim it. Acceptable while the whole system is one process being demoed;
//! unacceptable the moment it is not.

use repolens_core::{RepositoryCoordinate, ruleset};
use repolens_github::{GitHubRepositorySource, GitHubSourceError};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::contract::analysis::{AnalysisState, RepositoryIdentity};
use crate::contract::error::{ApiError, ErrorCode};
use crate::contract::report::{
    Confidence, Evidence, EvidenceKind, Finding, FindingCategory, FindingState, Limitation,
    LineRange, OverviewStatement, Report, Severity,
};
use crate::store;

/// Version of the analyzer producing these reports.
///
/// Distinct from the ruleset version: the same rules run by a different
/// analyzer can produce a different report, so both are published.
const ANALYZER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runs one analysis to completion, recording every state transition.
///
/// Never returns an error: a failure *is* the outcome, and it is written to the
/// analysis where the frontend can render it. Returning `Err` would leave the
/// row in a running state with the reason only in a log.
pub async fn run<S>(pool: &PgPool, source: &S, id: Uuid, coordinate: &RepositoryCoordinate)
where
    S: GitHubRepositorySource,
{
    // Filled in as resolution establishes each part, and carried out here so a
    // *failure* is recorded against everything already known.
    let mut resolved = Resolved::default();

    if let Err(error) = execute(pool, source, id, coordinate, &mut resolved).await {
        // The store failing here is the one case that cannot be recorded, since
        // recording is what failed. Logged by category, never with the URL.
        if let Err(store_error) = store::fail(pool, id, &error, &resolved).await {
            tracing::error!(
                analysis = %id,
                error = %store_error,
                "could not record an analysis failure"
            );
        }
    }
}

/// What resolution established before the outcome, whatever the outcome was.
///
/// Both fields reach the database through best-effort writes during the run —
/// `adopt_coordinate` and `advance` each log and continue when they fail — so
/// neither is guaranteed to be on the row when a terminal state is written.
/// Carrying them here lets the terminal write set them atomically alongside the
/// state, which is the only way an analysis cannot finish claiming less than
/// was known about it. A failure after the commit was resolved must not be
/// recorded with `commit_sha: null`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// Canonical coordinate, once GitHub has answered.
    pub coordinate: Option<RepositoryCoordinate>,
    /// Exact commit, once a reference has resolved to one.
    pub commit_sha: Option<String>,
}

/// The pipeline proper. Returns the failure to record, if any.
///
/// `resolved` is an out-parameter rather than a return value because it must
/// survive the error path: the caller needs it precisely when this function
/// returns `Err`.
async fn execute<S>(
    pool: &PgPool,
    source: &S,
    id: Uuid,
    submitted: &RepositoryCoordinate,
    resolved: &mut Resolved,
) -> Result<(), ApiError>
where
    S: GitHubRepositorySource,
{
    advance(pool, id, AnalysisState::Resolving, None).await;

    let repository = source
        .resolve_repository(submitted)
        .await
        .map_err(translate)?;

    // Adopted *before* any decision that can reject the repository.
    //
    // The archived check below returns early, and it used to do so before this
    // ran — so an archived repository that had also been renamed reached a
    // terminal state under the submitted coordinate, which is the one case
    // where the reader most needs to be told the real name. A rejection is
    // still a result about a specific repository, and it must name the one
    // GitHub identified.
    //
    // Everything past this point uses the coordinate GitHub answered with,
    // never the one that was submitted. A renamed or transferred repository
    // still resolves under its old address — GitHub redirects — so the
    // submission succeeds while naming something that no longer exists.
    let coordinate = repository.coordinate.clone();
    resolved.coordinate = Some(coordinate.clone());
    if coordinate != *submitted {
        tracing::info!(
            analysis = %id,
            submitted = %submitted,
            canonical = %coordinate,
            "the submission redirected; adopting the canonical coordinate"
        );
        // Best-effort, and deliberately not the only place this is written.
        // The terminal writes in `store::complete` and `store::fail` set
        // owner/name too, in the same transaction that sets the terminal
        // state, so losing this update costs the progress display its
        // corrected name and cannot leave a finished analysis disagreeing with
        // its own report.
        adopt_coordinate(pool, id, &coordinate).await;
    }
    let coordinate = &coordinate;

    if repository.archived {
        return Err(ApiError::new(
            ErrorCode::RepositoryArchived,
            "This repository is archived. It can still be read, but it is not under active \
             development, which is worth knowing before drawing conclusions from it.",
        ));
    }

    // Enforced here rather than at the GitHub boundary, and after the canonical
    // coordinate has been adopted.
    //
    // `resolve_repository` used to refuse an oversized repository before it had
    // parsed `full_name`, so this rejection reached the pipeline with no way to
    // learn the canonical coordinate and a renamed repository terminated under
    // the submitted address. Whether to spend the budget is a decision recorded
    // against a specific repository, so it is made where that repository is
    // known. Nothing has been downloaded at this point either way.
    if repository.size_kilobytes > repolens_github::limits::MAX_REPOSITORY_KILOBYTES {
        tracing::info!(
            analysis = %id,
            observed = repository.size_kilobytes,
            limit = repolens_github::limits::MAX_REPOSITORY_KILOBYTES,
            "the repository exceeds the ingestion ceiling"
        );
        return Err(ApiError::new(
            ErrorCode::RepositoryTooLarge,
            "This repository is larger than the limits this analysis is allowed to spend. The \
             limits are ours, not a judgement about the repository.",
        ));
    }

    let commit = source
        .resolve_commit(coordinate, &repository.default_branch)
        .await
        .map_err(translate)?;

    // Recorded before the transition is attempted. `advance` logs and continues
    // when its write fails, so this is what guarantees a later failure still
    // terminates with the commit it actually analyzed.
    resolved.commit_sha = Some(commit.sha.as_str().to_owned());

    advance(
        pool,
        id,
        AnalysisState::Collecting,
        Some(commit.sha.as_str()),
    )
    .await;

    let tree = source
        .fetch_tree(coordinate, &commit.sha)
        .await
        .map_err(translate)?;

    advance(pool, id, AnalysisState::Analyzing, None).await;

    let paths: Vec<String> = tree
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, repolens_github::TreeEntryKind::Blob))
        .map(|entry| entry.path.clone())
        .collect();

    // Read a bounded set of files, so rules can say what a repository is built
    // with rather than only which files it has.
    //
    // `select_paths` has existed since #4 and nothing called it: every report
    // until now cost one tree request and could claim only presence. The
    // selection is a pure function of the tree, so two runs at one commit read
    // the same files in the same order — which is what keeps a content-backed
    // report as reproducible as a path-backed one.
    //
    // A failure here is **not** fatal. Contents make findings stronger; losing
    // them costs confidence, not the report. The rules are told collection did
    // not happen, and every content rule then answers UNABLE_TO_VERIFY rather
    // than mistaking an unread file for an absent feature.
    let selection = repolens_github::select_paths(&tree);
    let (files, contents_collected) = match source
        .fetch_selected_blobs(coordinate, &commit.sha, &selection.paths)
        .await
    {
        Ok(blobs) => (decode(&blobs), true),
        Err(error) => {
            tracing::warn!(
                analysis = %id,
                error = %error,
                "could not read file contents; content rules will report unverified"
            );
            (Vec::new(), false)
        }
    };

    let input = repolens_core::RuleInput {
        repository: coordinate,
        commit: &commit.sha,
        paths: &paths,
        files: &files,
        tree_truncated: tree.truncated,
        contents_collected,
    };
    let outcomes = ruleset::evaluate(&input);

    advance(pool, id, AnalysisState::BuildingReport, None).await;

    let report = build_report(id, coordinate, &commit, &tree, &outcomes);

    store::complete(pool, id, &report).await.map_err(|error| {
        tracing::error!(analysis = %id, error = %error, "could not store the report");
        // Retriable, not permanent. The analysis itself *succeeded* — the
        // report was built — and what failed was the write. That is a
        // connection, a pool timeout, or a failover, none of which the same
        // commit and ruleset will reproduce. `ANALYZER_FAILED_PERMANENT` told
        // the UI to withhold retry forever over a fault that would very likely
        // clear on the next attempt, and it named the analyzer for a failure
        // the analyzer did not have.
        ApiError::new(
            ErrorCode::WorkerFailedRetriable,
            "The analysis finished but its report could not be stored. This is usually \
             temporary.",
        )
    })
}

/// Turns retrieved blobs into the text form rules read.
///
/// Undecodable bytes are dropped rather than lossily converted. A binary file
/// rendered as replacement characters can match a pattern by accident, and a
/// rule that fired on mojibake would cite an excerpt no reader could recognise
/// — worse than not having read the file, because it looks like evidence.
fn decode(blobs: &[repolens_github::BlobContent]) -> Vec<repolens_core::FileContent> {
    blobs
        .iter()
        .filter_map(|blob| {
            let text = String::from_utf8(blob.bytes.clone()).ok()?;
            Some(repolens_core::FileContent {
                path: blob.path.clone(),
                text,
                digest: blob.content_digest.clone(),
                // The boundary caps each blob, and a rule that finds nothing in
                // a file cut short has established nothing. Carried so
                // `content_verdict` can say so.
                truncated: false,
            })
        })
        .collect()
}

/// Records a transition, logging rather than failing if it cannot.
///
/// A lost transition costs the progress display a step; aborting the analysis
/// over one would cost the user the whole report. The wrong trade in the other
/// direction.
async fn advance(pool: &PgPool, id: Uuid, state: AnalysisState, commit_sha: Option<&str>) {
    if let Err(error) = store::advance(pool, id, state, commit_sha).await {
        tracing::warn!(analysis = %id, ?state, error = %error, "could not record a transition");
    }
}

/// Persists the canonical coordinate, logging rather than failing if it cannot.
///
/// Same trade as [`advance`]: losing this write costs the progress record its
/// corrected name, while aborting over it would cost the whole report. The
/// analysis continues against the canonical coordinate either way, so the
/// report — which is built from it, not from the row — stays correct.
async fn adopt_coordinate(pool: &PgPool, id: Uuid, coordinate: &RepositoryCoordinate) {
    if let Err(error) = store::adopt_coordinate(pool, id, coordinate).await {
        tracing::warn!(
            analysis = %id,
            error = %error,
            "could not record the canonical coordinate"
        );
    }
}

/// Maps an ingestion failure onto the public error contract.
///
/// The mapping is where a private repository stops being distinguishable from a
/// missing one — deliberately, since telling an anonymous caller that a private
/// repository exists leaks its existence.
fn translate(error: GitHubSourceError) -> ApiError {
    match error {
        GitHubSourceError::RepositoryNotFound { .. } => ApiError::new(
            ErrorCode::RepositoryNotFound,
            "No public repository was found at that address. Check the owner and name, and note \
             that private repositories are not supported.",
        ),
        GitHubSourceError::RateLimited {
            retry_after_seconds,
            reset_at,
        } => {
            // GitHub's own `retry-after` wins whenever it was sent.
            //
            // It accompanies the *secondary* rate limit, where there is no
            // window to reset and `x-ratelimit-reset` is typically absent or
            // points at the unrelated primary window. Consulting `reset_at`
            // first — or ignoring `retry-after` outright — publishes a wait
            // GitHub never asked for, and the fallback below then invents 60
            // seconds out of nothing. Retrying earlier than GitHub said is how
            // a secondary limit escalates.
            let seconds = retry_after_seconds
                .and_then(|seconds| u32::try_from(seconds).ok())
                .or_else(|| {
                    reset_at.and_then(|reset| {
                        (reset - OffsetDateTime::now_utc())
                            .whole_seconds()
                            .try_into()
                            .ok()
                    })
                })
                // Neither header arrived. This is a guess and the only one
                // available; it is deliberately the last resort rather than the
                // common path.
                .unwrap_or(60);
            ApiError::rate_limited(
                "The GitHub rate limit is exhausted. This is temporary.",
                seconds,
            )
        }
        GitHubSourceError::LimitExceeded { .. } => ApiError::new(
            ErrorCode::RepositoryTooLarge,
            "This repository is larger than the limits this analysis is allowed to spend. The \
             limits are ours, not a judgement about the repository.",
        ),
        // Everything else — transport, TLS, an unexpected status — may succeed
        // on another attempt, so it is inaccessible rather than absent.
        other => {
            tracing::warn!(error = %other, "ingestion failed");
            ApiError::new(
                ErrorCode::RepositoryInaccessible,
                "The repository could not be read. This is usually temporary.",
            )
        }
    }
}

/// Turns rule outcomes into a report.
fn build_report(
    analysis_id: Uuid,
    coordinate: &RepositoryCoordinate,
    commit: &repolens_github::ResolvedCommit,
    tree: &repolens_github::RepositoryTree,
    outcomes: &[ruleset::RuleOutcome],
) -> Report {
    let findings: Vec<Finding> = outcomes.iter().map(finding).collect();

    // Report-level, so "absence of evidence is not evidence of absence" stays
    // visible in the overview rather than buried in a finding nobody expanded.
    let mut limitations = Vec::new();
    if tree.truncated {
        limitations.push(Limitation {
            code: "TREE_TRUNCATED".to_owned(),
            explanation: "The repository tree exceeded the traversal bound, so files outside the \
                          collected paths were never seen. Every absence in this report is \
                          reported as unverified rather than missing."
                .to_owned(),
        });
    }
    limitations.push(Limitation {
        code: "PATHS_ONLY".to_owned(),
        explanation: "This ruleset reads file paths, not file contents. A finding states that a \
                      file exists at a path, not what it contains."
            .to_owned(),
    });

    let detected: Vec<String> = outcomes
        .iter()
        .filter(|o| o.outcome == ruleset::Outcome::Detected)
        .map(|o| o.rule_id.to_owned())
        .collect();

    let overview = vec![OverviewStatement {
        statement: if detected.is_empty() {
            "No rule in this ruleset matched anything in the collected paths.".to_owned()
        } else {
            format!(
                "{} of {} checks were satisfied by files present at this commit.",
                detected.len(),
                outcomes.len()
            )
        },
        supporting_rule_ids: detected,
        // Low, and honestly so: a path-based ruleset establishes presence, not
        // behaviour. Confidence rises when #5 adds rules that read contents.
        confidence: Confidence::Low,
    }];

    Report {
        analysis_id,
        repository: RepositoryIdentity {
            owner: coordinate.owner.clone(),
            name: coordinate.name.clone(),
        },
        commit_sha: commit.sha.as_str().to_owned(),
        // From the commit object, deliberately not from `tree.sha`.
        //
        // GitHub's tree endpoint echoes back whichever SHA it was asked for. The
        // listing is fetched by commit SHA, which resolves to the right tree but
        // reports that commit SHA as `sha` — so `tree.sha` would silently write
        // `commit_sha` into this field, and the two halves of the identity would
        // be one value repeated. Verified against `rust-lang/crates.io` at
        // 7bef82ce: the tree endpoint answers `7bef82ce…` when asked by commit
        // and `47ce74b3…` when asked by tree, for byte-identical entries.
        // `TreeSha` → `String` happens here and only here: the wire DTO is the
        // one place the typed identity has to be flattened.
        tree_sha: commit.tree_sha.as_str().to_owned(),
        analyzer_version: ANALYZER_VERSION.to_owned(),
        ruleset_version: ruleset::RULESET_VERSION.to_owned(),
        completed_at: OffsetDateTime::now_utc(),
        overview,
        findings,
        // No line counts: composition needs the archive path, which is #12.
        // Null is the designed state for that, not zero.
        composition: None,
        limitations,
    }
}

fn finding(outcome: &ruleset::RuleOutcome) -> Finding {
    let (title, explanation, category) = describe(outcome.rule_id);

    let state = match outcome.outcome {
        ruleset::Outcome::Detected => FindingState::Detected,
        ruleset::Outcome::Missing => FindingState::Missing,
        ruleset::Outcome::UnableToVerify => FindingState::UnableToVerify,
    };

    Finding {
        // Derived from the rule, not random: two runs over the same commit must
        // produce the same report, and a random id would break that for no gain.
        id: Uuid::new_v5(&Uuid::NAMESPACE_URL, outcome.rule_id.as_bytes()),
        rule_id: outcome.rule_id.to_owned(),
        ruleset_version: ruleset::RULESET_VERSION.to_owned(),
        category,
        state,
        // Every seed rule is informational. Severity is about impact if the
        // finding is valid, and "this repository has no architecture document"
        // is worth knowing rather than worth alarming about.
        severity: Severity::Info,
        confidence: match outcome.outcome {
            // Seeing a path proves the file is there.
            ruleset::Outcome::Detected => Confidence::High,
            // Not seeing one, across a complete tree, is a weaker claim.
            ruleset::Outcome::Missing => Confidence::Medium,
            ruleset::Outcome::UnableToVerify => Confidence::Low,
        },
        title: title.to_owned(),
        explanation: explanation.to_owned(),
        evidence: outcome
            .evidence
            .iter()
            .map(|item| Evidence {
                // A quoted line is a file *excerpt*; a bare path is only
                // presence. The kinds are not interchangeable — a reader who
                // sees FILE_EXCERPT expects something to read.
                kind: if item.excerpt.is_some() {
                    EvidenceKind::FileExcerpt
                } else {
                    EvidenceKind::FilePresence
                },
                path: Some(item.path.clone()),
                // Present exactly when the rule read the file. A path-only
                // finding carries neither: nothing was read, so an excerpt or
                // a digest would be the fabrication this pipeline exists to
                // prevent.
                excerpt: item.excerpt.clone(),
                truncated: false,
                digest: item.digest.clone(),
                line_range: item.line_range.map(|(start, end)| LineRange { start, end }),
            })
            .collect(),
        limitations: Vec::new(),
        recommended_action: None,
    }
}

/// Human-readable text for each rule.
///
/// Kept beside the pipeline rather than in `repolens-core`, because it is
/// presentation: the domain decides *what* was concluded, this decides how to
/// say it. A rule with no entry still reports, using its id — a missing string
/// must not silently drop a finding.
fn describe(rule_id: &str) -> (&'static str, &'static str, FindingCategory) {
    match rule_id {
        "rust.workspace" => (
            "Rust workspace detected",
            "A Cargo manifest is present at the repository root.",
            FindingCategory::Technology,
        ),
        "ci.workflows" => (
            "GitHub Actions workflows present",
            "Workflow definitions exist under .github/workflows. This states that CI is \
             configured, not what it runs — reading the steps needs file contents.",
            FindingCategory::CiCd,
        ),
        "docs.architecture" => (
            "Architecture documentation",
            "An architecture document describes the boundaries a newcomer needs. Its absence \
             means they are not written down, not that the architecture is poor.",
            FindingCategory::SourceAndDocumentation,
        ),
        "contract.openapi.committed" => (
            "Committed openapi.json or openapi.yaml",
            "A file whose name is exactly openapi.json or openapi.yaml is committed, which is \
             what allows a generated client rather than a hand-written one. Those two names \
             and no others: openapi.yml, a snapshot of a document generated at runtime, or any \
             other spelling is not detected by this rule and its absence here is not a claim \
             that the repository has no OpenAPI contract.",
            FindingCategory::Architecture,
        ),
        "database.migrations" => (
            "Migration strategy detected",
            "SQL migrations are committed under migrations/, so schema changes are versioned \
             alongside the code that depends on them.",
            FindingCategory::Operations,
        ),
        "tests.present" => (
            "Automated tests present",
            "Test files exist. This states that tests are written, not that they are good or \
             that they run — both need more than a path.",
            FindingCategory::Testing,
        ),
        "framework.axum" => (
            "Built on axum",
            "The Cargo manifest declares axum, so the HTTP surface is built on it. Read from              the dependency line quoted below rather than inferred from a directory name.",
            FindingCategory::Technology,
        ),
        "framework.sveltekit" => (
            "Built on SvelteKit",
            "The npm manifest declares @sveltejs/kit. This states which framework the              frontend uses, not how it is deployed — an adapter choice needs its own rule.",
            FindingCategory::Technology,
        ),
        "database.sqlx" => (
            "Database access through SQLx",
            "The Cargo manifest declares sqlx. Committed migrations say a schema is versioned;              this says what reaches it.",
            FindingCategory::Technology,
        ),
        _ => (
            "Unrecognised rule",
            "This rule produced a result but has no description in this build. It is reported \
             rather than dropped, because a missing string is a gap in presentation, not \
             grounds to hide a finding.",
            FindingCategory::Technology,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path-only rule input, for tests that do not exercise content rules.
    ///
    /// `contents_collected: false` is the honest setting: these tests hand the
    /// analyzer no files, so every content rule answers `UNABLE_TO_VERIFY` —
    /// which is exactly what a run that read nothing should produce.
    fn path_input(paths: &[String]) -> repolens_core::RuleInput<'static> {
        repolens_core::RuleInput {
            repository: Box::leak(Box::new(RepositoryCoordinate::new("owner", "name"))),
            commit: Box::leak(Box::new(
                repolens_core::CommitSha::parse(&"a".repeat(40)).expect("a literal digest"),
            )),
            paths: Box::leak(paths.to_vec().into_boxed_slice()),
            files: &[],
            tree_truncated: false,
            contents_collected: false,
        }
    }

    #[test]
    fn every_seed_rule_has_a_description() {
        // A rule without one still reports, but under a generic title that tells
        // a reader nothing. This makes adding a rule and forgetting the text a
        // test failure rather than a quietly worse report.
        for outcome in ruleset::evaluate(&path_input(&[])) {
            let (title, _, _) = describe(outcome.rule_id);
            assert_ne!(
                title, "Unrecognised rule",
                "{} has no description",
                outcome.rule_id
            );
        }
    }

    #[test]
    fn finding_ids_are_stable_across_runs() {
        // Two analyses of the same commit must produce the same report. A random
        // id would make every rerun differ in a way no reader could explain.
        let outcomes = ruleset::evaluate(&path_input(&["Cargo.toml".to_owned()]));
        let first: Vec<_> = outcomes.iter().map(|o| finding(o).id).collect();
        let second: Vec<_> = outcomes.iter().map(|o| finding(o).id).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn the_tree_sha_comes_from_the_commit_not_from_the_echoed_listing() {
        // GitHub's tree endpoint echoes back whichever SHA it was asked for. The
        // listing is fetched by commit SHA, so `tree.sha` holds that commit SHA
        // for a perfectly correct set of entries — and writing it into the
        // report made both halves of the identity the same value, which reads as
        // correct because it is a well-formed digest.
        //
        // Reproduced with the shape observed against `rust-lang/crates.io` at
        // 7bef82ce: the listing echoes the commit SHA, the commit object carries
        // the real tree, 47ce74b3.
        const COMMIT: &str = "7bef82cebb702b89ec8d3f13facf67a83bc7d090";
        const TREE: &str = "47ce74b3cf6de899392fb2caf1fd6406f2fa47f3";

        let commit = repolens_github::ResolvedCommit {
            sha: repolens_core::CommitSha::parse(COMMIT).expect("a literal digest"),
            tree_sha: repolens_core::TreeSha::parse(TREE).expect("a literal digest"),
            committed_at: OffsetDateTime::from_unix_timestamp(1_785_873_497)
                .expect("a literal timestamp"),
        };
        let tree = repolens_github::RepositoryTree {
            // What GitHub actually returns when asked by commit.
            sha: COMMIT.to_owned(),
            entries: Vec::new(),
            truncated: false,
        };

        let report = build_report(
            Uuid::nil(),
            &RepositoryCoordinate::new("rust-lang", "crates.io"),
            &commit,
            &tree,
            &ruleset::evaluate(&path_input(&[])),
        );

        assert_eq!(report.commit_sha, COMMIT);
        assert_eq!(
            report.tree_sha, TREE,
            "the tree SHA must come from the commit object; taking the listing's \
             echoed SHA repeats the commit SHA and the identity carries no tree"
        );
        assert_ne!(
            report.tree_sha, report.commit_sha,
            "a commit and its root tree are different objects"
        );
    }

    #[test]
    fn the_rate_limit_wait_is_githubs_when_github_supplied_one() {
        // GitHub sends `retry-after` with the *secondary* rate limit, where
        // there is no window to reset. Preferring `reset_at` — or ignoring
        // `retry-after` entirely and falling through to the invented 60 —
        // publishes a wait GitHub never asked for, and retrying a secondary
        // limit early is how it escalates.
        let error = translate(GitHubSourceError::RateLimited {
            retry_after_seconds: Some(37),
            // Deliberately present and deliberately different: if `reset_at`
            // won, this would read 3600 rather than 37.
            reset_at: Some(OffsetDateTime::now_utc() + time::Duration::hours(1)),
        });

        assert_eq!(error.code(), ErrorCode::RateLimited);
        assert_eq!(
            error.retry_after_seconds(),
            Some(37),
            "GitHub's own retry-after must win over a window reset"
        );
    }

    #[test]
    fn the_rate_limit_wait_falls_back_to_the_window_reset() {
        // The primary limit usually sends no `retry-after`, only a reset
        // instant. That is still measured rather than guessed.
        let error = translate(GitHubSourceError::RateLimited {
            retry_after_seconds: None,
            reset_at: Some(OffsetDateTime::now_utc() + time::Duration::seconds(120)),
        });

        let seconds = error.retry_after_seconds().expect("a wait is published");
        assert!(
            (118..=120).contains(&seconds),
            "expected roughly 120 seconds from the reset instant, got {seconds}"
        );
    }

    #[test]
    fn the_invented_wait_is_the_last_resort_only() {
        // Neither header arrived. 60 is a guess, and it must be reachable only
        // when there is genuinely nothing to measure.
        let error = translate(GitHubSourceError::RateLimited {
            retry_after_seconds: None,
            reset_at: None,
        });
        assert_eq!(error.retry_after_seconds(), Some(60));
    }

    /// Records the coordinate every call was made with.
    ///
    /// The whole point of the fake: the finding is not about what the pipeline
    /// returns, it is about *which repository it asks GitHub for* after a
    /// redirect.
    struct RecordingSource {
        canonical: RepositoryCoordinate,
        archived: bool,
        size_kilobytes: u64,
        seen: std::sync::Mutex<Vec<RepositoryCoordinate>>,
    }

    impl GitHubRepositorySource for RecordingSource {
        async fn resolve_repository(
            &self,
            coordinate: &RepositoryCoordinate,
        ) -> Result<repolens_github::ResolvedRepository, GitHubSourceError> {
            self.seen.lock().unwrap().push(coordinate.clone());
            // What GitHub does for a renamed or transferred repository: the old
            // address resolves, and the answer names the new one.
            Ok(repolens_github::ResolvedRepository {
                coordinate: self.canonical.clone(),
                default_branch: "main".to_owned(),
                archived: self.archived,
                size_kilobytes: self.size_kilobytes,
            })
        }

        async fn resolve_commit(
            &self,
            coordinate: &RepositoryCoordinate,
            _reference: &str,
        ) -> Result<repolens_github::ResolvedCommit, GitHubSourceError> {
            self.seen.lock().unwrap().push(coordinate.clone());
            Ok(repolens_github::ResolvedCommit {
                sha: repolens_core::CommitSha::parse(&"a".repeat(40)).expect("a literal digest"),
                tree_sha: repolens_core::TreeSha::parse(&"b".repeat(40)).expect("a literal digest"),
                committed_at: OffsetDateTime::UNIX_EPOCH,
            })
        }

        async fn fetch_tree(
            &self,
            coordinate: &RepositoryCoordinate,
            _commit: &repolens_core::CommitSha,
        ) -> Result<repolens_github::RepositoryTree, GitHubSourceError> {
            self.seen.lock().unwrap().push(coordinate.clone());
            Ok(repolens_github::RepositoryTree {
                sha: "a".repeat(40),
                entries: Vec::new(),
                truncated: false,
            })
        }

        async fn fetch_selected_blobs(
            &self,
            coordinate: &RepositoryCoordinate,
            _commit: &repolens_core::CommitSha,
            _paths: &[String],
        ) -> Result<Vec<repolens_github::BlobContent>, GitHubSourceError> {
            // Recorded like the others: content collection is a request against
            // a specific repository, so it has to use the canonical coordinate
            // too. Returns nothing, which these tests treat as a run that read
            // no files — content rules then report unverified.
            self.seen.lock().unwrap().push(coordinate.clone());
            Ok(Vec::new())
        }

        async fn download_archive(
            &self,
            _coordinate: &RepositoryCoordinate,
            _commit: &repolens_core::CommitSha,
            _max_compressed_bytes: u64,
            _destination: &std::path::Path,
        ) -> Result<repolens_github::ArchiveDownload, GitHubSourceError> {
            unreachable!("composition is #12")
        }
    }

    #[tokio::test]
    async fn a_redirected_submission_is_analyzed_under_its_canonical_coordinate() {
        // A pool that can never connect, with an acquire timeout short enough
        // that each store call fails immediately. Every write this pipeline
        // makes is either log-and-continue or the final `complete`, so the
        // traversal under test runs to the end regardless — and what is under
        // test is which coordinate GitHub was asked for, not what was stored.
        let pool = unreachable_pool();

        let submitted = RepositoryCoordinate::new("old-owner", "old-name");
        let canonical = RepositoryCoordinate::new("new-owner", "new-name");
        let source = RecordingSource {
            canonical: canonical.clone(),
            archived: false,
            size_kilobytes: 1,
            seen: std::sync::Mutex::new(Vec::new()),
        };

        let mut reported = Resolved::default();
        let _ = execute(&pool, &source, Uuid::nil(), &submitted, &mut reported).await;

        assert_eq!(
            reported.coordinate.as_ref(),
            Some(&canonical),
            "the canonical coordinate must reach the caller, which is what lets a \
             terminal failure be recorded under it"
        );

        let seen = source.seen.lock().unwrap().clone();
        assert_eq!(
            seen.first(),
            Some(&submitted),
            "the first call is the submission itself; that is how the redirect is discovered"
        );
        assert!(
            seen.len() > 1,
            "the pipeline must get past resolution for this test to mean anything"
        );
        assert!(
            seen[1..].iter().all(|coordinate| coordinate == &canonical),
            "every call after resolution must use the coordinate GitHub answered \
             with, not the submitted one; saw {seen:?}"
        );
    }

    /// A pool that can never connect, failing fast enough to use in a test.
    fn unreachable_pool() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(1))
            // Port 1 on loopback refuses immediately, so each store call fails
            // fast rather than waiting out a DNS or connect timeout. The
            // credentials are named rather than plausible: a fixture that
            // merely looks real still trips credential scanners and still makes
            // a reviewer stop and check.
            .connect_lazy("postgres://EXAMPLE_USER:EXAMPLE_PASSWORD@127.0.0.1:1/none")
            .expect("a lazy pool never connects at construction")
    }

    #[tokio::test]
    async fn an_archived_repository_still_reports_its_canonical_coordinate() {
        // The archived check returns early. It used to do so *before* the
        // coordinate was adopted, so an archived repository that had also been
        // renamed reached a terminal state under the address the submitter
        // typed — the one case where the reader most needs the real name,
        // because they are being told to go and look at the repository.
        let submitted = RepositoryCoordinate::new("old-owner", "old-name");
        let canonical = RepositoryCoordinate::new("new-owner", "new-name");
        let source = RecordingSource {
            canonical: canonical.clone(),
            archived: true,
            size_kilobytes: 1,
            seen: std::sync::Mutex::new(Vec::new()),
        };

        let mut reported = Resolved::default();
        let outcome = execute(
            &unreachable_pool(),
            &source,
            Uuid::nil(),
            &submitted,
            &mut reported,
        )
        .await;

        assert_eq!(
            outcome
                .expect_err("an archived repository is rejected")
                .code(),
            ErrorCode::RepositoryArchived
        );
        assert_eq!(
            reported.coordinate.as_ref(),
            Some(&canonical),
            "the rejection must still be recorded against the coordinate GitHub \
             identified, not the one that was submitted"
        );
        assert!(
            reported.commit_sha.is_none(),
            "an archived repository is rejected before any commit is resolved"
        );
    }

    #[tokio::test]
    async fn an_oversized_repository_still_reports_its_canonical_coordinate() {
        // The size ceiling used to be enforced inside `resolve_repository`,
        // above the point where `full_name` was parsed, so this rejection
        // reached the pipeline with no canonical coordinate at all and a
        // renamed repository terminated under the submitted address.
        let submitted = RepositoryCoordinate::new("old-owner", "old-name");
        let canonical = RepositoryCoordinate::new("new-owner", "new-name");
        let source = RecordingSource {
            canonical: canonical.clone(),
            archived: false,
            size_kilobytes: repolens_github::limits::MAX_REPOSITORY_KILOBYTES + 1,
            seen: std::sync::Mutex::new(Vec::new()),
        };

        let mut resolved = Resolved::default();
        let outcome = execute(
            &unreachable_pool(),
            &source,
            Uuid::nil(),
            &submitted,
            &mut resolved,
        )
        .await;

        assert_eq!(
            outcome.expect_err("the ceiling is enforced").code(),
            ErrorCode::RepositoryTooLarge
        );
        assert_eq!(
            resolved.coordinate.as_ref(),
            Some(&canonical),
            "a rejection is still a result about a specific repository and must \
             name the one GitHub identified"
        );
        assert!(
            resolved.commit_sha.is_none(),
            "nothing was resolved to a commit, so claiming one would be a fabrication"
        );
    }

    #[tokio::test]
    async fn a_failure_after_resolution_still_knows_the_commit() {
        // `advance` writes the commit best-effort and logs on failure, so the
        // terminal write is the only durable carrier. Here every store call
        // fails, which is exactly the case that used to terminate with
        // `commit_sha: null` for a commit that had been resolved.
        let coordinate = RepositoryCoordinate::new("owner", "name");
        let source = RecordingSource {
            canonical: coordinate.clone(),
            archived: false,
            size_kilobytes: 1,
            seen: std::sync::Mutex::new(Vec::new()),
        };

        let mut resolved = Resolved::default();
        let error = execute(
            &unreachable_pool(),
            &source,
            Uuid::nil(),
            &coordinate,
            &mut resolved,
        )
        .await
        .expect_err("the report cannot be stored");

        assert_eq!(error.code(), ErrorCode::WorkerFailedRetriable);
        assert_eq!(
            resolved.commit_sha.as_deref(),
            Some("a".repeat(40).as_str()),
            "the commit was resolved and analyzed; a terminal state that forgot \
             it would report less than was known"
        );
        assert_eq!(resolved.coordinate.as_ref(), Some(&coordinate));
    }

    #[tokio::test]
    async fn a_report_that_cannot_be_stored_is_retriable() {
        // The analysis itself succeeded — the report was built — and what failed
        // was the write. A connection, a pool timeout, or a failover is not
        // something the same commit and ruleset reproduce, so classifying it as
        // ANALYZER_FAILED_PERMANENT told the UI to withhold retry forever over a
        // fault that would very likely clear, and blamed the analyzer for a
        // failure it did not have.
        let coordinate = RepositoryCoordinate::new("owner", "name");
        let source = RecordingSource {
            canonical: coordinate.clone(),
            archived: false,
            size_kilobytes: 1,
            seen: std::sync::Mutex::new(Vec::new()),
        };

        let mut reported = Resolved::default();
        let error = execute(
            &unreachable_pool(),
            &source,
            Uuid::nil(),
            &coordinate,
            &mut reported,
        )
        .await
        .expect_err("the report cannot be stored against a pool that cannot connect");

        assert_eq!(error.code(), ErrorCode::WorkerFailedRetriable);
        assert!(
            error.code().is_retriable(),
            "a transient write failure must leave retry available"
        );
    }

    #[test]
    fn the_report_names_the_repository_it_was_built_for() {
        // The other half of the same finding: a report that cited the submitted
        // coordinate would point a reader at an address that no longer
        // identifies what was analyzed.
        let canonical = RepositoryCoordinate::new("new-owner", "new-name");
        let commit = repolens_github::ResolvedCommit {
            sha: repolens_core::CommitSha::parse(&"a".repeat(40)).expect("a literal digest"),
            tree_sha: repolens_core::TreeSha::parse(&"b".repeat(40)).expect("a literal digest"),
            committed_at: OffsetDateTime::UNIX_EPOCH,
        };
        let tree = repolens_github::RepositoryTree {
            sha: "a".repeat(40),
            entries: Vec::new(),
            truncated: false,
        };

        let report = build_report(
            Uuid::nil(),
            &canonical,
            &commit,
            &tree,
            &ruleset::evaluate(&path_input(&[])),
        );

        assert_eq!(report.repository.owner, "new-owner");
        assert_eq!(report.repository.name, "new-name");
    }

    #[test]
    fn a_path_only_finding_carries_no_invented_evidence() {
        // Nothing was read, so an excerpt or a digest here would be fabricated.
        let outcomes = ruleset::evaluate(&path_input(&["Cargo.toml".to_owned()]));
        let detected = outcomes
            .iter()
            .find(|o| o.outcome == ruleset::Outcome::Detected)
            .unwrap();

        for evidence in finding(detected).evidence {
            assert!(evidence.excerpt.is_none());
            assert!(evidence.digest.is_none());
            assert!(!evidence.truncated);
        }
    }
}
