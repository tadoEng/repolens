//! Generates and gates the `analysis-v1` executable fixtures.
//!
//! The fixtures are written **from the Rust types**, not by hand. A hand-written
//! fixture is a second definition of the contract that drifts the moment a DTO
//! changes — and drifts silently, because nothing compiles it. Generating them
//! means a field rename either updates every fixture or fails this test.
//!
//! The other half of the gate lives in TypeScript: the fixtures are type-checked
//! against the generated `schema.ts`, so a shape the frontend cannot consume
//! fails the build rather than the browser.
//!
//! Regenerate after an intentional contract change:
//!
//! ```sh
//! UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures
//! ```

use std::fs;
use std::path::PathBuf;

use repolens_core::ContentDigest;
use repolens_server::contract::analysis::{
    Analysis, AnalysisState, ExecutionMetadata, RepositoryIdentity, RetryPolicy, TriggerStatus,
};
use repolens_server::contract::error::{ApiError, ErrorCode};
use repolens_server::contract::report::{
    AreaLineCount, CodeRole, CompositionExclusion, Confidence, Evidence, EvidenceKind,
    EvidenceSource, Finding, FindingCategory, FindingState, LanguageLineCount, LargestSourceFile,
    LargestSourceFiles, Limitation, LineCountSummary, LineRange, OverviewStatement, Report,
    RoleLineCount, Severity,
};
use serde::Serialize;
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

/// Where the fixtures live. `contracts/` is the handshake between the API and
/// the frontend; both sides read this directory.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/fixtures/analysis-v1")
}

/// Fixed identifiers and timestamps.
///
/// Nothing here may be random or clock-derived: a fixture that changed on every
/// run would make the gate below fail for reasons unrelated to the contract, and
/// the first fix anyone reached for would be deleting the gate.
const ANALYSIS_ID: Uuid = Uuid::from_u128(0x0193_a5c0_0000_7000_8000_0000_0000_0001);
const CREATED_AT: OffsetDateTime = datetime!(2026-08-06 09:00:00 UTC);
const UPDATED_AT: OffsetDateTime = datetime!(2026-08-06 09:00:04 UTC);
const COMMIT_SHA: &str = "0584a2df65968a4e9e6859ef46bbed430408a3f1";
const TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn repository() -> RepositoryIdentity {
    RepositoryIdentity {
        owner: "rust-lang".to_owned(),
        name: "crates.io".to_owned(),
    }
}

/// An analysis in a non-terminal state, with no commit resolved yet.
fn pending(state: AnalysisState, trigger: TriggerStatus, commit: Option<&str>) -> Analysis {
    Analysis {
        id: ANALYSIS_ID,
        repository: repository(),
        commit_sha: commit.map(ToOwned::to_owned),
        state,
        execution: ExecutionMetadata {
            trigger_status: trigger,
            execution_id: matches!(trigger, TriggerStatus::Succeeded)
                .then(|| "exec-0193a5c0".to_owned()),
            triggered_at: matches!(trigger, TriggerStatus::Succeeded).then_some(CREATED_AT),
        },
        retry: RetryPolicy {
            allowed: false,
            reason: Some("the analysis has not failed".to_owned()),
        },
        error: None,
        created_at: CREATED_AT,
        updated_at: UPDATED_AT,
        // Widens as the analysis ages; the client never hardcodes an interval.
        poll_after_ms: Some(2_000),
        report_available: false,
    }
}

/// An analysis that failed, carrying the server's retry decision.
///
/// `commit` is a parameter rather than a constant because **not every failure
/// has one**. A repository that is absent, archived, or over the metadata size
/// ceiling is rejected during resolution, before `resolve_commit` is ever
/// called, so those fixtures must carry `null`. Hardcoding a SHA for all of
/// them made the executable contract describe states the pipeline cannot
/// produce — the fixtures proved serialization and nothing about behaviour, and
/// `null` is precisely the honest representation the contract already defines
/// for "resolution has not produced a commit".
fn failed(
    state: AnalysisState,
    error: ApiError,
    retry: RetryPolicy,
    commit: Option<&str>,
) -> Analysis {
    Analysis {
        id: ANALYSIS_ID,
        repository: repository(),
        commit_sha: commit.map(ToOwned::to_owned),
        state,
        execution: ExecutionMetadata {
            trigger_status: TriggerStatus::Succeeded,
            execution_id: Some("exec-0193a5c0".to_owned()),
            triggered_at: Some(CREATED_AT),
        },
        retry,
        error: Some(error),
        created_at: CREATED_AT,
        updated_at: UPDATED_AT,
        // Terminal: nothing left to poll for.
        poll_after_ms: None,
        report_available: false,
    }
}

fn evidence_excerpt() -> Evidence {
    Evidence {
        kind: EvidenceKind::FileExcerpt,
        path: Some("Cargo.toml".to_owned()),
        excerpt: Some("[workspace]\nmembers = [\"crates/*\"]".to_owned()),
        // Truncated server-side. The frontend is never what stops a large payload.
        truncated: true,
        // Built from raw bytes rather than written as a string, so the fixture
        // cannot contain a digest the contract would reject.
        digest: Some(ContentDigest::from_sha256([0x6b; 32])),
        line_range: Some(LineRange { start: 1, end: 2 }),
    }
}

fn findings() -> Vec<Finding> {
    vec![
        Finding {
            id: Uuid::from_u128(0x0193_a5c0_0000_7000_8000_0000_0000_0010),
            rule_id: "rust.workspace.detected".to_owned(),
            ruleset_version: "1".to_owned(),
            category: FindingCategory::Technology,
            state: FindingState::Detected,
            severity: Severity::Info,
            confidence: Confidence::High,
            title: "Rust workspace detected".to_owned(),
            explanation: "The repository root declares a Cargo workspace.".to_owned(),
            evidence: vec![evidence_excerpt()],
            limitations: vec![],
            recommended_action: None,
        },
        Finding {
            id: Uuid::from_u128(0x0193_a5c0_0000_7000_8000_0000_0000_0011),
            rule_id: "docs.architecture.missing".to_owned(),
            ruleset_version: "1".to_owned(),
            category: FindingCategory::SourceAndDocumentation,
            state: FindingState::Missing,
            // MISSING is not automatically severe; the rule explains why it matters.
            severity: Severity::Low,
            confidence: Confidence::Medium,
            title: "No architecture document found".to_owned(),
            explanation:
                "No docs/ARCHITECTURE file was found at the analyzed commit. Absence here \
                 means the boundaries a reader would need are not written down, not that \
                 the architecture is poor."
                    .to_owned(),
            evidence: vec![Evidence {
                kind: EvidenceKind::FilePresence,
                path: Some("docs/".to_owned()),
                excerpt: None,
                truncated: false,
                digest: None,
                line_range: None,
            }],
            limitations: vec![Limitation {
                code: "TREE_TRUNCATED".to_owned(),
                explanation: "The repository tree exceeded the traversal bound, so a document \
                     outside the collected paths would not have been seen."
                    .to_owned(),
            }],
            recommended_action: Some(
                "Confirm by hand whether an architecture document exists outside the \
                 collected paths."
                    .to_owned(),
            ),
        },
        Finding {
            id: Uuid::from_u128(0x0193_a5c0_0000_7000_8000_0000_0000_0012),
            rule_id: "ci.tests.unverifiable".to_owned(),
            ruleset_version: "1".to_owned(),
            category: FindingCategory::CiCd,
            // The state that must never be rendered as MISSING.
            state: FindingState::UnableToVerify,
            severity: Severity::Info,
            confidence: Confidence::Low,
            title: "Could not determine whether CI runs tests".to_owned(),
            explanation:
                "Workflow files were present but exceeded the per-file size bound, so their \
                 steps were not read."
                    .to_owned(),
            // Deliberately empty: this is the case where there is nothing to show.
            evidence: vec![],
            limitations: vec![Limitation {
                code: "FILE_TOO_LARGE".to_owned(),
                explanation: "One or more workflow files exceeded the per-file byte cap."
                    .to_owned(),
            }],
            recommended_action: None,
        },
    ]
}

fn composition() -> LineCountSummary {
    LineCountSummary {
        counter: "tokei".to_owned(),
        counter_version: "14.0.0".to_owned(),
        exclusion_policy_version: "1".to_owned(),
        classification_policy_version: "1".to_owned(),
        total_files: 842,
        total_lines: 91_204,
        code_lines: 78_310,
        comment_lines: 8_120,
        blank_lines: 4_774,
        languages: vec![
            LanguageLineCount {
                language: "Rust".to_owned(),
                files: 512,
                code_lines: 48_210,
                comment_lines: 6_420,
                blank_lines: 3_180,
            },
            LanguageLineCount {
                language: "TypeScript".to_owned(),
                files: 214,
                code_lines: 19_430,
                comment_lines: 1_060,
                blank_lines: 1_010,
            },
        ],
        areas: vec![
            AreaLineCount {
                area: "crates/".to_owned(),
                code_lines: 51_800,
            },
            AreaLineCount {
                area: "web/".to_owned(),
                code_lines: 26_510,
            },
        ],
        exclusions: vec![CompositionExclusion {
            path_or_rule: "**/node_modules/**".to_owned(),
            reason: "Vendored dependencies are not this repository's code.".to_owned(),
            matched_rule: "vendor.node_modules".to_owned(),
            file_count: 126,
            bytes: 4_182_004,
        }],
        roles: vec![
            RoleLineCount {
                role: CodeRole::Production,
                files: 604,
                code_lines: 63_400,
            },
            RoleLineCount {
                role: CodeRole::Test,
                files: 178,
                code_lines: 11_710,
            },
            RoleLineCount {
                role: CodeRole::Generated,
                files: 34,
                code_lines: 3_200,
            },
        ],
        // Through the validated constructor, so the fixture cannot carry a list
        // the contract would reject.
        largest_files: LargestSourceFiles::new(vec![
            LargestSourceFile {
                path: "src/publication.rs".to_owned(),
                language: "Rust".to_owned(),
                code_lines: 2_410,
                role: CodeRole::Production,
            },
            LargestSourceFile {
                path: "packages/api-client/src/schema.ts".to_owned(),
                language: "TypeScript".to_owned(),
                code_lines: 1_980,
                // Generated: without the role, this would read as the second
                // largest hand-written file in the repository.
                role: CodeRole::Generated,
            },
        ])
        .expect("fixture respects the bound"),
        unclassified_files: 7,
    }
}

fn report(composition: Option<LineCountSummary>, limitations: Vec<Limitation>) -> Report {
    Report {
        analysis_id: ANALYSIS_ID,
        repository: repository(),
        commit_sha: COMMIT_SHA.to_owned(),
        tree_sha: TREE_SHA.to_owned(),
        evidence_source: Some(EvidenceSource {
            api: "github-rest".to_owned(),
            version: "2026-03-10".to_owned(),
        }),
        analyzer_version: "0.1.0".to_owned(),
        ruleset_version: "1".to_owned(),
        completed_at: UPDATED_AT,
        overview: vec![OverviewStatement {
            statement: "Rust workspace with an Axum backend and a static SvelteKit frontend."
                .to_owned(),
            supporting_rule_ids: vec!["rust.workspace.detected".to_owned()],
            confidence: Confidence::High,
        }],
        findings: findings(),
        composition,
        limitations,
    }
}

/// The completed analysis that accompanies a report.
fn completed_analysis() -> Analysis {
    Analysis {
        id: ANALYSIS_ID,
        repository: repository(),
        commit_sha: Some(COMMIT_SHA.to_owned()),
        state: AnalysisState::Completed,
        execution: ExecutionMetadata {
            trigger_status: TriggerStatus::Succeeded,
            execution_id: Some("exec-0193a5c0".to_owned()),
            triggered_at: Some(CREATED_AT),
        },
        retry: RetryPolicy {
            allowed: false,
            reason: Some("the analysis succeeded".to_owned()),
        },
        error: None,
        created_at: CREATED_AT,
        updated_at: UPDATED_AT,
        poll_after_ms: None,
        report_available: true,
    }
}

/// A fixture pairs the analysis with its report, because every screen that
/// renders one needs the other for its header.
#[derive(Serialize)]
struct Fixture<'a> {
    analysis: &'a Analysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<&'a Report>,
}

/// The four failure fixtures, split out to keep `fixtures` within the workspace
/// function-length lint rather than suppressing it.
fn failure_fixtures() -> Vec<(&'static str, Analysis)> {
    let retriable = failed(
        AnalysisState::FailedRetriable,
        ApiError::rate_limited(
            "The GitHub rate limit is exhausted. The analysis will resume automatically.",
            900,
        ),
        RetryPolicy {
            allowed: true,
            reason: None,
        },
        // The rate limit is most often hit fetching the tree, after resolution.
        Some(COMMIT_SHA),
    );

    let permanent = failed(
        AnalysisState::FailedPermanent,
        ApiError::new(
            ErrorCode::AnalyzerFailedPermanent,
            "The analyzer failed deterministically at this commit. Retrying would fail identically.",
        ),
        RetryPolicy {
            allowed: false,
            reason: Some(
                "This failure is deterministic: the same commit and ruleset will fail again."
                    .to_owned(),
            ),
        },
        // The analyzer runs on a resolved commit by definition.
        Some(COMMIT_SHA),
    );

    let worker_retriable = failed(
        AnalysisState::FailedRetriable,
        ApiError::new(
            ErrorCode::WorkerFailedRetriable,
            "The worker stopped before finishing. The analysis can be retried.",
        ),
        RetryPolicy {
            allowed: true,
            reason: None,
        },
        // The worker stops after the report was built, so a commit exists.
        Some(COMMIT_SHA),
    );

    let inaccessible = failed(
        AnalysisState::FailedRetriable,
        ApiError::new(
            ErrorCode::RepositoryInaccessible,
            "The repository could not be read. This is usually temporary.",
        ),
        RetryPolicy {
            allowed: true,
            reason: None,
        },
        // Reachable both before and after resolution; the fixture shows the
        // case where a commit had already been established.
        Some(COMMIT_SHA),
    );

    vec![
        ("failed-retriable.json", retriable),
        ("failed-permanent.json", permanent),
        ("failed-worker-retriable.json", worker_retriable),
        ("failed-inaccessible.json", inaccessible),
    ]
    .into_iter()
    .chain(repository_failure_fixtures())
    .collect()
}

/// Failures decided about the *repository* rather than about the analysis.
///
/// Split from `failure_fixtures` for the same reason that one was split from
/// `fixtures`: the workspace function-length lint, not taste.
fn repository_failure_fixtures() -> Vec<(&'static str, Analysis)> {
    /// Every one of these is permanent, and the reason is the same each time.
    fn permanent() -> RetryPolicy {
        RetryPolicy {
            allowed: false,
            reason: Some(
                "This failure is deterministic: the same commit and ruleset will fail again."
                    .to_owned(),
            ),
        }
    }

    // The three below are reached during an analysis and written to the row by
    // `store::fail`, so each is a terminal state a user can land on. All are
    // permanent: waiting will not create a repository, un-archive one, or
    // shrink one past an ingestion bound.
    let not_found = failed(
        AnalysisState::FailedPermanent,
        ApiError::new(
            ErrorCode::RepositoryNotFound,
            "No public repository was found at that address. Check the owner and name, and note \
             that private repositories are not supported.",
        ),
        permanent(),
        // Rejected during resolution: `resolve_commit` is never reached, so
        // there is no commit and `null` is the honest value.
        None,
    );

    let archived = failed(
        AnalysisState::FailedPermanent,
        ApiError::new(
            ErrorCode::RepositoryArchived,
            "This repository is archived. It can still be read, but it is not under active \
             development, which is worth knowing before drawing conclusions from it.",
        ),
        permanent(),
        // Rejected during resolution: `resolve_commit` is never reached, so
        // there is no commit and `null` is the honest value.
        None,
    );

    let too_large = failed(
        AnalysisState::FailedPermanent,
        ApiError::new(
            ErrorCode::RepositoryTooLarge,
            "This repository is larger than the limits this analysis is allowed to spend. The \
             limits are ours, not a judgement about the repository.",
        ),
        permanent(),
        // Rejected during resolution: `resolve_commit` is never reached, so
        // there is no commit and `null` is the honest value.
        None,
    );

    vec![
        ("failed-repository-not-found.json", not_found),
        ("failed-repository-archived.json", archived),
        ("failed-repository-too-large.json", too_large),
    ]
}

fn fixtures() -> Vec<(&'static str, String)> {
    let queued = pending(AnalysisState::Queued, TriggerStatus::Succeeded, None);
    let resolving = pending(AnalysisState::Resolving, TriggerStatus::Succeeded, None);
    let completed = completed_analysis();
    let full_report = report(Some(composition()), vec![]);
    let no_composition_report = report(
        None,
        vec![Limitation {
            code: "EXTRACTION_STORAGE_LIMIT".to_owned(),
            explanation:
                "Archive extraction exceeded the configured storage limit, so no line counts \
                 were produced. This is not a claim that the repository has no code."
                    .to_owned(),
        }],
    );
    // A report written before the analyzer published where its evidence came
    // from. Nothing produces one now, and the store still holds them — so the
    // frontend has to render the absence rather than assume the key is there,
    // and a fixture is how that stays true.
    let unsourced_report = Report {
        evidence_source: None,
        ..report(Some(composition()), vec![])
    };

    let render = |value: &dyn erased::Erased| value.to_pretty();

    vec![
        (
            "queued.json",
            render(&Fixture {
                analysis: &queued,
                report: None,
            }),
        ),
        (
            "resolving.json",
            render(&Fixture {
                analysis: &resolving,
                report: None,
            }),
        ),
        (
            "completed-report.json",
            render(&Fixture {
                analysis: &completed,
                report: Some(&full_report),
            }),
        ),
        (
            "loc-unavailable.json",
            render(&Fixture {
                analysis: &completed,
                report: Some(&no_composition_report),
            }),
        ),
        (
            "evidence-source-absent.json",
            render(&Fixture {
                analysis: &completed,
                report: Some(&unsourced_report),
            }),
        ),
    ]
    .into_iter()
    .chain(
        failure_fixtures()
            .into_iter()
            .map(|(name, analysis)| {
                let body = render(&Fixture {
                    analysis: &analysis,
                    report: None,
                });
                (name, body)
            })
            .collect::<Vec<_>>(),
    )
    .collect()
}

/// Minimal erasure so `fixtures()` can render heterogeneous values uniformly
/// without a generic function per shape.
mod erased {
    pub(super) trait Erased {
        fn to_pretty(&self) -> String;
    }

    impl<T: serde::Serialize> Erased for T {
        fn to_pretty(&self) -> String {
            let mut json = serde_json::to_string_pretty(self).expect("fixtures always serialize");
            json.push('\n');
            json
        }
    }
}

#[test]
fn fixtures_match_the_contract() {
    let dir = fixture_dir();
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();

    if update {
        fs::create_dir_all(&dir).expect("creating contracts/fixtures/analysis-v1");
    }

    for (name, generated) in fixtures() {
        let path = dir.join(name);

        if update {
            fs::write(&path, &generated).unwrap_or_else(|e| panic!("writing {name}: {e}"));
            continue;
        }

        let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{name} could not be read ({e}).\n\
                 Generate with: UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures"
            )
        });

        assert_eq!(
            committed.replace("\r\n", "\n"),
            generated.replace("\r\n", "\n"),
            "{name} is stale.\n\
             Regenerate with: UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures"
        );
    }
}

#[test]
fn finding_order_is_stable_across_serializations() {
    // The report claims determinism. If findings were emitted in a different
    // order on each call, two loads of the same report would read differently
    // and no client-side sort could recover an order the server never fixed.
    let first = serde_json::to_string(&report(Some(composition()), vec![])).unwrap();
    let second = serde_json::to_string(&report(Some(composition()), vec![])).unwrap();
    assert_eq!(first, second);
}

#[test]
fn every_error_code_is_either_in_a_fixture_or_explicitly_exempt() {
    // The previous version of this test named two codes and called itself a
    // closed-set gate. Adding a ninth code left it green while proving nothing.
    // It now iterates every variant, so a new code must be either rendered in a
    // fixture or listed here as a deliberate omission.
    // Exempt only where the code can never be *persisted on an analysis*.
    //
    // `INVALID_REPOSITORY_URL` is decided before the row is inserted, so no
    // analysis can carry it. The other five are produced by the HTTP layer — a
    // body that is not JSON, a body over the limit, a request that ran out of
    // time, an unknown analysis id, a report that is not ready, a panic — and
    // `pipeline` cannot construct any of them, so no analysis can reach a terminal state carrying one. A
    // fixture asserting otherwise would describe a state the system cannot
    // produce. All of them are still rendered: the unknown-variant gate in the
    // client package covers every code.
    //
    // `REPOSITORY_NOT_FOUND`, `REPOSITORY_ARCHIVED` and `REPOSITORY_TOO_LARGE`
    // were previously in this list and should never have been. Each is reached
    // *during* an analysis — the first two from resolution, the third from an
    // exceeded ingestion bound — and each is written to the row by
    // `store::fail`. Exempting them meant three terminal states a user can
    // actually hit had no proof that the frontend renders them.
    const EXEMPT: [ErrorCode; 9] = [
        ErrorCode::InvalidRepositoryUrl,
        ErrorCode::AnalysisNotFound,
        ErrorCode::ReportNotAvailable,
        ErrorCode::Unauthenticated,
        ErrorCode::AuthenticationUnavailable,
        ErrorCode::MalformedRequest,
        ErrorCode::RequestTooLarge,
        ErrorCode::RequestTimedOut,
        ErrorCode::InternalError,
    ];

    let rendered: String = fixtures().into_iter().map(|(_, body)| body).collect();
    let mut missing = Vec::new();
    for code in ErrorCode::ALL {
        if EXEMPT.contains(&code) {
            continue;
        }
        let name = serde_json::to_string(&code).unwrap();
        let bare = name.trim_matches('"').to_owned();
        if !rendered.contains(&bare) {
            missing.push(bare);
        }
    }

    assert!(
        missing.is_empty(),
        "these error codes have no fixture, so no frontend rendering is proven: {missing:?}"
    );
}
