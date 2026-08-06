//! Store behaviour against a real PostgreSQL.
//!
//! The first integration suite in this workspace, and it arrives with the CI
//! service that runs it — a suite nothing executes is a suite nobody maintains.
//!
//! # It fails loudly rather than skipping
//!
//! `DATABASE_URL` unset is a **panic**, not a skip. A suite that quietly
//! disappears when its dependency is absent is worse than no suite: every run
//! is green, the coverage claim survives, and the day the variable is dropped
//! from CI nobody finds out. The message says exactly how to supply one.
//!
//! # What it proves
//!
//! That a terminal write carries the canonical repository coordinate. The
//! coordinate is adopted mid-pipeline by a best-effort update whose failure is
//! logged and ignored, so the only durable guarantee is the one made by
//! `store::complete` and `store::fail` in the same statement that sets the
//! terminal state. That guarantee cannot be checked without a database: it is a
//! property of the SQL, not of any Rust value.

use repolens_core::RepositoryCoordinate;
use repolens_server::contract::analysis::AnalysisState;
use repolens_server::contract::analysis::RepositoryIdentity;
use repolens_server::contract::error::{ApiError, ErrorCode};
use repolens_server::contract::report::{Confidence, OverviewStatement, Report};
use repolens_server::store;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

/// Connects, or fails saying what is missing.
async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "DATABASE_URL is not set.\n\
             \n\
             These tests run against a real PostgreSQL and are deliberately not skippable: a \
             suite that vanishes when its dependency is absent reports green while proving \
             nothing. Start one and apply the migrations, for example:\n\
             \n  docker run --rm -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres:17\
             \n  DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres \\\n    \
             DATABASE_DIRECT_URL=postgres://postgres:postgres@localhost:5432/postgres \\\n    \
             cargo run --bin migrate\n"
        )
    });

    PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("DATABASE_URL is set but no connection could be established")
}

/// Inserts a queued analysis and returns its id.
async fn queued(pool: &PgPool, coordinate: &RepositoryCoordinate) -> Uuid {
    store::create_analysis(pool, coordinate)
        .await
        .expect("the analysis is created")
        .id
}

/// Reads owner and name straight from the row, bypassing the DTO mapping.
async fn stored_coordinate(pool: &PgPool, id: Uuid) -> RepositoryCoordinate {
    let row = sqlx::query("SELECT owner, name FROM analyses WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("the analysis row exists");

    RepositoryCoordinate::new(row.get::<String, _>("owner"), row.get::<String, _>("name"))
}

async fn cleanup(pool: &PgPool, id: Uuid) {
    // Reports first: the foreign key points that way.
    let _ = sqlx::query("DELETE FROM reports WHERE analysis_id = $1")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM analyses WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await;
}

fn report_for(id: Uuid, coordinate: &RepositoryCoordinate) -> Report {
    Report {
        analysis_id: id,
        repository: RepositoryIdentity {
            owner: coordinate.owner.clone(),
            name: coordinate.name.clone(),
        },
        commit_sha: "a".repeat(40),
        tree_sha: "b".repeat(40),
        analyzer_version: "0.1.0".to_owned(),
        ruleset_version: "2".to_owned(),
        completed_at: OffsetDateTime::UNIX_EPOCH,
        overview: vec![OverviewStatement {
            statement: "nothing to say".to_owned(),
            supporting_rule_ids: vec![],
            confidence: Confidence::Low,
        }],
        findings: vec![],
        composition: None,
        limitations: vec![],
    }
}

#[tokio::test]
async fn completion_persists_the_canonical_coordinate() {
    let pool = pool().await;
    let submitted = RepositoryCoordinate::new("old-owner", "old-name");
    let canonical = RepositoryCoordinate::new("new-owner", "new-name");

    let id = queued(&pool, &submitted).await;

    // The row starts under the submitted coordinate, because that is all the
    // create handler knows.
    assert_eq!(stored_coordinate(&pool, id).await, submitted);

    // Completion alone — no intervening `adopt_coordinate`. This is the case
    // the review named: the mid-pipeline update is best-effort, so if it is
    // lost, completion must still leave the row and its report agreeing.
    store::complete(&pool, id, &report_for(id, &canonical))
        .await
        .expect("the report is stored");

    assert_eq!(
        stored_coordinate(&pool, id).await,
        canonical,
        "a completed analysis must name the repository its report names; \
         leaving the submitted coordinate gives one row two answers"
    );

    let analysis = store::load_analysis(&pool, id)
        .await
        .expect("the analysis reads back");
    assert_eq!(analysis.state, AnalysisState::Completed);
    assert_eq!(analysis.repository.owner, canonical.owner);
    assert_eq!(analysis.repository.name, canonical.name);
    assert!(analysis.report_available);

    cleanup(&pool, id).await;
}

#[tokio::test]
async fn a_terminal_failure_persists_the_canonical_coordinate() {
    let pool = pool().await;
    let submitted = RepositoryCoordinate::new("old-owner", "archived-old-name");
    let canonical = RepositoryCoordinate::new("new-owner", "archived-new-name");

    let id = queued(&pool, &submitted).await;

    // An archived repository is rejected after resolution, so the canonical
    // coordinate is known by then and the terminal row must carry it.
    store::fail(
        &pool,
        id,
        &ApiError::new(ErrorCode::RepositoryArchived, "archived"),
        Some(&canonical),
    )
    .await
    .expect("the failure is recorded");

    assert_eq!(stored_coordinate(&pool, id).await, canonical);

    let analysis = store::load_analysis(&pool, id)
        .await
        .expect("the analysis reads back");
    assert_eq!(analysis.state, AnalysisState::FailedPermanent);

    cleanup(&pool, id).await;
}

#[tokio::test]
async fn a_failure_before_resolution_keeps_what_was_submitted() {
    let pool = pool().await;
    let submitted = RepositoryCoordinate::new("owner-that", "never-resolved");

    let id = queued(&pool, &submitted).await;

    // Nothing was learned from GitHub, so there is no canonical coordinate to
    // adopt. The `COALESCE` must leave the submitted one alone rather than
    // writing null over it — the row would otherwise lose the only identity it
    // ever had, and the column is NOT NULL.
    store::fail(
        &pool,
        id,
        &ApiError::new(ErrorCode::RepositoryNotFound, "no such repository"),
        None,
    )
    .await
    .expect("the failure is recorded");

    assert_eq!(stored_coordinate(&pool, id).await, submitted);

    cleanup(&pool, id).await;
}

#[tokio::test]
async fn a_report_round_trips_through_jsonb_unchanged() {
    // The report is stored as one JSONB document. PostgreSQL does not preserve
    // key order or duplicate keys in `jsonb`, so this asserts the thing that
    // actually matters: what comes back deserializes to the same report.
    let pool = pool().await;
    let coordinate = RepositoryCoordinate::new("round", "trip");
    let id = queued(&pool, &coordinate).await;

    let original = report_for(id, &coordinate);
    store::complete(&pool, id, &original)
        .await
        .expect("the report is stored");

    let loaded = store::load_report(&pool, id)
        .await
        .expect("the report reads back");

    assert_eq!(loaded, original);
    assert_eq!(
        loaded.analytical_payload().unwrap(),
        original.analytical_payload().unwrap()
    );

    cleanup(&pool, id).await;
}

#[tokio::test]
async fn an_unknown_analysis_is_reported_as_an_analysis_and_not_a_repository() {
    // Driven over HTTP, and only reachable with a real pool: without one the
    // handler answers 503 before it ever consults the store, so no test in
    // `api.rs` can reach this path.
    //
    // The code matters. `REPOSITORY_NOT_FOUND` says a repository is absent or
    // private on GitHub, which sends the reader to check a URL that was never
    // the problem — and the UI offers to correct one.
    use axum::body::Body;
    use axum::http::Request;
    use repolens_github::{GitHubClientConfig, GitHubRestClient};
    use repolens_server::api;
    use repolens_server::state::AppState;
    use tower::ServiceExt as _;

    let url = std::env::var("DATABASE_URL").expect("checked by `pool`");
    let github = GitHubRestClient::new(GitHubClientConfig::new()).expect("constructible");
    let state = AppState::with_pool(
        AppState::connect_lazy(&url).expect("the pool is configured"),
        github,
    );
    let (app, _openapi) = api::build(state, None);

    let unknown = Uuid::now_v7();
    for uri in [
        format!("/api/v1/analyses/{unknown}"),
        format!("/api/v1/analyses/{unknown}/report"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&uri)
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), 404, "at {uri}");
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .expect("body is readable");
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("the envelope");
        assert_eq!(
            parsed["code"], "ANALYSIS_NOT_FOUND",
            "an unknown analysis id is not a statement about a repository; at {uri}"
        );
    }
}

#[tokio::test]
async fn an_unknown_analysis_is_not_found_rather_than_an_error() {
    let pool = pool().await;

    // The distinction the handler maps to 404 versus 503. A query failure and
    // an absent row must not arrive as the same variant.
    let missing = store::load_analysis(&pool, Uuid::now_v7()).await;
    assert!(matches!(missing, Err(store::StoreError::NotFound)));

    let no_report = store::load_report(&pool, Uuid::now_v7()).await;
    assert!(matches!(no_report, Err(store::StoreError::NotFound)));
}
