//! Generates and gates the executable fixtures for every published contract.
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
//! # Two families, one gate
//!
//! `analysis-v1` is the report a reader came for; `admin-v1` is the operational
//! snapshot an operator reads. They are separate directories because they are
//! separate contracts with separate audiences, and one gate because the rule
//! they are held to is identical — a second `--test` target for admin would be
//! the "special admin script" this arrangement exists to avoid, and the
//! regeneration command in the root `AGENTS.md` would stop covering everything.
//!
//! Regenerate after an intentional contract change:
//!
//! ```sh
//! UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures
//! ```

use std::fs;
use std::path::PathBuf;

use repolens_core::ContentDigest;
use repolens_server::contract::admin::{
    AdminOverview, HttpMethodClass, HttpOverview, LatencyPercentile, LatencySummary,
    ProcessOverview, RouteOverview, StatusClassCounts,
};
use repolens_server::contract::analysis::{
    Analysis, AnalysisState, ExecutionMetadata, RepositoryIdentity, RetryPolicy, TriggerStatus,
};
use repolens_server::contract::error::{ApiError, ErrorCode};
use repolens_server::contract::report::{
    AreaLineCount, CodeRole, CompositionExclusion, Confidence, Evidence, EvidenceKind,
    EvidenceProvider, EvidenceSource, Finding, FindingCategory, FindingState, LanguageLineCount,
    LargestSourceFile, LargestSourceFiles, Limitation, LineCountSummary, LineRange,
    OverviewStatement, Report, RoleLineCount, Severity,
};
use serde::Serialize;
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

/// Where the fixtures live. `contracts/` is the handshake between the API and
/// the frontend; both sides read this directory.
fn fixture_dir(family: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/fixtures")
        .join(family)
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
                path: "packages/api-client/src/generated/schema.ts".to_owned(),
                language: "TypeScript".to_owned(),
                code_lines: 1_980,
                // Generated, and the path is one the classification policy
                // genuinely reads that way -- a `generated` segment. An
                // illustrative path the policy would call production, wearing
                // a GENERATED label, would make this fixture contradict the
                // rules it is supposed to demonstrate.
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
            provider: EvidenceProvider::GithubRest,
            api_version: "2026-03-10".to_owned(),
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

/// One row of the published route table.
///
/// A helper rather than eight literals, because the fields that must agree —
/// requests against the status classes below it — are easy to get wrong by hand
/// and impossible to notice afterwards. `admin_fixtures_are_internally_coherent`
/// asserts the agreement; this makes it cheap to keep.
fn route(
    label: &str,
    method: HttpMethodClass,
    responses: StatusClassCounts,
    total_micros: u64,
    percentiles: [LatencyPercentile; 3],
) -> RouteOverview {
    let [p50, p95, p99] = percentiles;
    RouteOverview {
        route: label.to_owned(),
        method,
        requests: responses.informational
            + responses.success
            + responses.redirection
            + responses.client_error
            + responses.server_error
            + responses.other,
        responses,
        latency: LatencySummary {
            total_micros,
            p50,
            p95,
            p99,
        },
    }
}

/// A percentile inside a bucket that has both bounds.
const fn estimate(micros: u64, lower: u64, upper: u64) -> LatencyPercentile {
    LatencyPercentile {
        micros,
        lower_bound_micros: lower,
        upper_bound_micros: Some(upper),
    }
}

/// A percentile in the overflow bucket, where the figure is a floor.
///
/// `micros` equals the lower bound, because past the last bound the histogram
/// recorded that an observation was slower and nothing more. Any upper figure
/// would be invented, which is why the field is null rather than large.
const fn floor(lower: u64) -> LatencyPercentile {
    LatencyPercentile {
        micros: lower,
        lower_bound_micros: lower,
        upper_bound_micros: None,
    }
}

fn counts(success: u64, client_error: u64, server_error: u64) -> StatusClassCounts {
    StatusClassCounts {
        informational: 0,
        success,
        redirection: 0,
        client_error,
        server_error,
        other: 0,
    }
}

/// A plausible snapshot of a process that has been serving for a day.
///
/// **Every value is written here and none is read from the running process.**
/// A fixture built from a live `Metrics::snapshot()`, the real uptime, or
/// `/proc` would be regenerated differently on every run: the gate below would
/// then fail for reasons that have nothing to do with the contract, and the
/// first fix anyone reached for would be deleting the gate.
///
/// The figures are chosen to exercise the wire vocabulary rather than to be
/// impressive — the route labels are the five a caller can actually produce
/// against this router, including the fixed `<unmatched>` label, and the status
/// classes carry a client error and a server error so a consumer that renders
/// only `2xx` fails on the fixture rather than in production.
fn admin_overview(resident_bytes: Option<u64>) -> AdminOverview {
    AdminOverview {
        process: ProcessOverview {
            build_sha: COMMIT_SHA.to_owned(),
            // A day, two hours, three minutes and four seconds: distinct enough
            // in every unit that a renderer dividing by the wrong one is
            // visible rather than plausible.
            uptime_seconds: 93_784,
            resident_bytes,
        },
        http: HttpOverview {
            // One, not zero. The request reading the snapshot is itself in
            // flight while the snapshot is taken, and a fixture showing an idle
            // process would teach a reader to treat one as an anomaly.
            in_flight: 1,
            tracked_routes: 5,
            max_tracked_routes: 64,
            // Sorted by route then method, exactly as the server emits them —
            // a HashMap iterates differently between runs, and a rendered table
            // that reshuffled under a reader would be the server's fault rather
            // than the UI's to fix.
            routes: vec![
                route(
                    "/api/v1/analyses",
                    HttpMethodClass::Post,
                    // The 4xx here is the authentication gate refusing, which is
                    // ordinary traffic for this route rather than a fault.
                    counts(389, 22, 1),
                    64_070_000,
                    [
                        estimate(9_800, 5_000, 10_000),
                        estimate(31_200, 25_000, 50_000),
                        // Past the last bucket bound. A cold instance resuming a
                        // scaled-to-zero database is exactly how a request
                        // exceeds ten seconds, and telling that apart from a slow
                        // handler is what issue #37 exists to do — so the case a
                        // UI must render as a floor gets a fixture.
                        floor(10_000_000),
                    ],
                ),
                route(
                    "/api/v1/analyses/{analysis_id}",
                    HttpMethodClass::Get,
                    counts(7_041, 60, 1),
                    63_918_000,
                    [
                        estimate(7_900, 5_000, 10_000),
                        estimate(22_800, 10_000, 25_000),
                        estimate(41_500, 25_000, 50_000),
                    ],
                ),
                route(
                    "/api/v1/system/probe",
                    HttpMethodClass::Get,
                    counts(1_241, 0, 0),
                    9_928_000,
                    [
                        estimate(6_800, 5_000, 10_000),
                        estimate(17_400, 10_000, 25_000),
                        estimate(33_100, 25_000, 50_000),
                    ],
                ),
                route(
                    "/healthz",
                    HttpMethodClass::Get,
                    counts(6_814, 0, 0),
                    6_132_600,
                    [
                        estimate(720, 500, 1_000),
                        estimate(2_100, 1_000, 2_500),
                        estimate(4_400, 2_500, 5_000),
                    ],
                ),
                // Not a pattern, and it says so. Every request that matched no
                // route shares this one label however many distinct paths were
                // probed, which is what keeps a 404 flood from being an
                // unbounded label set on demand.
                route(
                    "<unmatched>",
                    HttpMethodClass::Get,
                    counts(0, 37, 0),
                    11_100,
                    [
                        estimate(260, 0, 500),
                        estimate(420, 0, 500),
                        estimate(480, 0, 500),
                    ],
                ),
            ],
        },
    }
}

/// The `admin-v1` fixtures.
///
/// Two files differing in exactly one field. That is deliberate: a reader
/// diffing them sees the unknown-memory case and nothing else, and a change
/// that accidentally altered the rest of the payload shows up as noise in a
/// diff that should have one line in it.
///
/// There are no `401` or `403` fixtures. Authorisation semantics are owned by
/// `tests/admin.rs`, which drives the real router, and the wire shape of a
/// refusal is `ApiError` — already published, already rendered. A fixture for
/// them would assert serialization and nothing about behaviour.
fn admin_fixtures() -> Vec<(&'static str, String)> {
    let render = |value: &dyn erased::Erased| value.to_pretty();

    vec![
        ("overview.json", render(&admin_overview(Some(61_849_600)))),
        // Named for the figure that is unknown rather than for the process,
        // which is emphatically available — it answered the request. The
        // precedent is `loc-unavailable.json`: the file is named after the
        // measurement that could not be taken.
        (
            "overview-memory-unavailable.json",
            render(&admin_overview(None)),
        ),
    ]
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

/// Every published family, and the directory each is written to.
///
/// One list, so adding a contract family means adding a line here rather than a
/// second test target. A family that was generated but never gated would be a
/// hand-editable file wearing a generated file's banner.
fn families() -> Vec<(&'static str, Vec<(&'static str, String)>)> {
    vec![("analysis-v1", fixtures()), ("admin-v1", admin_fixtures())]
}

#[test]
fn fixtures_match_the_contract() {
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();

    for (family, fixtures) in families() {
        let dir = fixture_dir(family);

        if update {
            fs::create_dir_all(&dir)
                .unwrap_or_else(|e| panic!("creating contracts/fixtures/{family}: {e}"));
        }

        for (name, generated) in fixtures {
            let path = dir.join(name);

            if update {
                fs::write(&path, &generated)
                    .unwrap_or_else(|e| panic!("writing {family}/{name}: {e}"));
                continue;
            }

            let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "{family}/{name} could not be read ({e}).\n\
                     Generate with: UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures"
                )
            });

            assert_eq!(
                committed.replace("\r\n", "\n"),
                generated.replace("\r\n", "\n"),
                "{family}/{name} is stale.\n\
                 Regenerate with: UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures"
            );
        }
    }
}

#[test]
fn admin_fixtures_are_internally_coherent() {
    // A fixture is evidence, not illustration: the frontend is built against
    // these numbers, so a set that no running process could have produced would
    // teach the UI invariants that do not hold. None of this is checkable by
    // reading the literals — the arithmetic is exactly the part a human eye
    // slides over.
    for (name, body) in admin_fixtures() {
        let overview: AdminOverview =
            serde_json::from_str(&body).unwrap_or_else(|e| panic!("{name} parses: {e}"));

        assert_eq!(
            overview.http.tracked_routes,
            u64::try_from(overview.http.routes.len()).expect("fits"),
            "{name}: the label count disagrees with the table it describes"
        );
        assert!(
            overview.http.tracked_routes <= overview.http.max_tracked_routes,
            "{name}: more labels are held than the ceiling permits"
        );

        let mut previous_label: Option<&str> = None;
        for row in &overview.http.routes {
            let responses = row.responses;
            assert_eq!(
                row.requests,
                responses.informational
                    + responses.success
                    + responses.redirection
                    + responses.client_error
                    + responses.server_error
                    + responses.other,
                "{name}: {} counts requests the status classes do not account for",
                row.route
            );
            assert!(
                row.requests > 0,
                "{name}: {} is a series with no observations, which the server never emits",
                row.route
            );

            // Percentiles cannot decrease, and each estimate has to lie inside
            // the bucket it names. A fixture violating either would be the one
            // shape a reader has no reason to distrust.
            let latency = row.latency;
            assert!(
                latency.p50.micros <= latency.p95.micros
                    && latency.p95.micros <= latency.p99.micros,
                "{name}: {} has percentiles that go backwards",
                row.route
            );
            for (label, estimate) in [
                ("p50", latency.p50),
                ("p95", latency.p95),
                ("p99", latency.p99),
            ] {
                assert!(
                    estimate.micros >= estimate.lower_bound_micros,
                    "{name}: {} {label} is below its own bucket",
                    row.route
                );
                match estimate.upper_bound_micros {
                    Some(upper) => assert!(
                        estimate.micros <= upper && estimate.lower_bound_micros < upper,
                        "{name}: {} {label} is outside its own bucket",
                        row.route
                    ),
                    // The overflow bucket reports a floor rather than an
                    // estimate, so the two must be the same number. Anything
                    // else is an interpolation towards a bound that does not
                    // exist.
                    None => assert_eq!(
                        estimate.micros, estimate.lower_bound_micros,
                        "{name}: {} {label} interpolated past the last bound",
                        row.route
                    ),
                }
            }
            assert!(
                latency.total_micros >= row.requests * latency.p50.lower_bound_micros,
                "{name}: {} spent less time in total than its own median implies",
                row.route
            );

            // The server sorts by label so a rendered table does not reshuffle
            // between reads. A fixture in another order would publish an order
            // the server never produces.
            if let Some(previous) = previous_label {
                assert!(
                    previous < row.route.as_str(),
                    "{name}: {} follows {previous}, which is not the order the server emits",
                    row.route
                );
            }
            previous_label = Some(&row.route);
        }
    }
}

#[test]
fn the_admin_fixtures_differ_only_in_the_figure_that_is_unknown() {
    // The pair exists to publish "unknown renders as unknown rather than zero".
    // If the two files drifted in any other field, a consumer diffing them
    // would learn the wrong lesson about which case it is looking at — and the
    // null would stop being the thing under test.
    let known: AdminOverview = serde_json::from_str(&admin_fixtures()[0].1).expect("parses");
    let unknown: AdminOverview = serde_json::from_str(&admin_fixtures()[1].1).expect("parses");

    assert!(known.process.resident_bytes.is_some());
    assert_eq!(
        unknown.process.resident_bytes, None,
        "the unavailable-memory fixture must carry null, not zero"
    );
    assert_eq!(
        AdminOverview {
            process: ProcessOverview {
                resident_bytes: known.process.resident_bytes,
                ..unknown.process
            },
            ..unknown
        },
        known,
        "the two fixtures differ in more than the memory figure"
    );
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
    //
    // `FORBIDDEN` joins them for the same reason and one of its own: the admin
    // gate refuses before any handler runs, so no analysis can carry it, and it
    // belongs to `admin-v1` rather than to this contract at all. Its behaviour
    // is owned by `tests/admin.rs` against the real router, and its rendering by
    // the unknown-variant gate in the client package, which covers every code.
    const EXEMPT: [ErrorCode; 10] = [
        ErrorCode::InvalidRepositoryUrl,
        ErrorCode::AnalysisNotFound,
        ErrorCode::ReportNotAvailable,
        ErrorCode::Unauthenticated,
        ErrorCode::Forbidden,
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
