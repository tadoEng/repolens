//! HTTP surface.
//!
//! The router is built through `utoipa-axum`'s [`OpenApiRouter`] so that the
//! OpenAPI document is collected from the routes that actually exist. A path
//! cannot be documented without being served, and cannot be served without
//! being documented — which is what makes the generated TypeScript client
//! trustworthy enough to be the frontend's only view of this API.

use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use serde::{Deserialize, Serialize};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub mod analyses;

use crate::config;
use crate::contract;
use crate::state::{AppState, BUILD_SHA};

/// Ceiling on request bodies. Every current and planned endpoint takes either
/// nothing or one repository URL, so this is generous rather than tight.
const REQUEST_BODY_LIMIT_BYTES: usize = 16 * 1024;

/// Ceiling on how long a request may occupy a Cloud Run instance. Analysis is
/// performed by the worker, never inside a request, so no endpoint has a
/// legitimate reason to approach this.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Root OpenAPI document. Paths and schemas are contributed by the router.
/// Root OpenAPI document.
///
/// Paths are contributed by the router, so a route cannot be served without
/// being documented. The `analysis-v1` schemas are registered explicitly
/// instead, because issue #14 fixes the contract while issue #6 builds the
/// endpoints that serve it — the generated client and the executable fixtures
/// therefore exist before any analysis route does, which is what unblocks
/// frontend work without anyone inventing a DTO in Svelte.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "RepoLens API",
        description = "Deterministic, evidence-backed analysis of a public GitHub repository at an exact commit."
    ),
    components(schemas(
        analyses::CreateAnalysisRequest,
        contract::error::ApiError,
        contract::error::ErrorCode,
        contract::analysis::Analysis,
        contract::analysis::AnalysisState,
        contract::analysis::ExecutionMetadata,
        contract::analysis::RepositoryIdentity,
        contract::analysis::RetryPolicy,
        contract::analysis::TriggerStatus,
        contract::report::AreaLineCount,
        contract::report::CompositionExclusion,
        contract::report::Confidence,
        contract::report::Evidence,
        contract::report::EvidenceKind,
        contract::report::Finding,
        contract::report::FindingCategory,
        contract::report::FindingState,
        contract::report::LanguageLineCount,
        contract::report::Limitation,
        contract::report::LineCountSummary,
        contract::report::LineRange,
        contract::report::OverviewStatement,
        contract::report::Report,
        contract::report::Severity,
    ))
)]
struct ApiDoc;

/// Process liveness.
///
/// Deliberately *not* the system probe. `GET /api/v1/system/probe`, which also
/// reports database reachability, build SHA, and schema version, is owned by
/// the walking-skeleton work (issue #11); answering "is the database up?" here
/// before there is a pool would be a claim this binary cannot support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LivenessResponse {
    /// Always `ok` when the process can serve a request at all.
    pub status: String,
}

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    responses((status = 200, description = "The process is serving requests", body = LivenessResponse))
)]
async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse {
        status: "ok".to_owned(),
    })
}

/// Reachability of one dependency.
///
/// Enum values are `SCREAMING_SNAKE_CASE` per the settled contract convention
/// (issue #14). Unlike object fields — where Rust's own naming already matches
/// and no rename is used — enum variants are `PascalCase` in Rust, so a rename
/// is unavoidable here. It is applied once, on the enum, rather than per
/// variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProbeStatus {
    /// Reachable and behaving.
    Ok,
    /// Reachable but not fully healthy.
    Degraded,
    /// Not reachable, or not configured.
    Unavailable,
}

impl ProbeStatus {
    /// Stable label for logs, matching the wire representation.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Degraded => "DEGRADED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

/// Result of the system probe.
///
/// Deliberately the *whole* hosting path in one response: the process answered
/// (`api`), the database answered a real query (`database`), the running code
/// is identifiable (`build_sha`), and the schema is at a known version
/// (`schema_version`). A liveness endpoint proves only the first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SystemProbeResponse {
    /// Always `OK`: reaching this handler means the process is serving.
    pub api: ProbeStatus,
    /// Whether a real query against the configured database succeeded.
    pub database: ProbeStatus,
    /// Commit this binary was built from, or `unknown` for a local build.
    pub build_sha: String,
    /// Highest applied migration version.
    ///
    /// Null rather than zero when the database could not be reached: "no
    /// migrations have been applied" and "we could not find out" are different
    /// facts, and collapsing them into `0` would let a connection failure read
    /// as an empty database. The frontend must render the null case, which is
    /// also the cheapest available exercise of its unknown-value handling.
    ///
    /// `required` is explicit because utoipa treats `Option<T>` as optional by
    /// default, which would generate `schema_version?: number | null` in
    /// TypeScript. The field is always present — its *value* is nullable — and
    /// the two are different contracts: an optional field lets a consumer
    /// forget the null case entirely, which is the case that matters here.
    #[schema(required)]
    pub schema_version: Option<i64>,
}

/// Why a database probe did not succeed.
///
/// Errors are reported by bounded category rather than by rendering the `sqlx`
/// error, because that message is attacker-influenceable in principle and
/// unbounded in practice — it can carry connection parameters, server notices,
/// and query text into the log. A closed set keeps log volume predictable and
/// keeps a credential-bearing URL out of the record entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeFailure {
    /// The work did not complete within the probe's own budget.
    Timeout,
    /// No usable connection: DNS, TLS, authentication, or the pool.
    Connection,
    /// Connected, but the expected relation is absent — migrations never ran.
    SchemaMissing,
    /// Connected, but reading the schema version failed for another reason —
    /// most often a missing `SELECT` grant on `_sqlx_migrations`.
    SchemaUnreadable,
}

impl ProbeFailure {
    /// Stable, low-cardinality label for logs and metrics.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connection => "connection",
            Self::SchemaMissing => "schema_missing",
            Self::SchemaUnreadable => "schema_unreadable",
        }
    }

    /// Whether connectivity had already been proven when this failure occurred.
    ///
    /// A failure after `SELECT 1` succeeds cannot mean "unreachable", so it
    /// maps to `DEGRADED` rather than `UNAVAILABLE`. A timeout is deliberately
    /// *not* in this set: the deadline covers both queries, so which phase was
    /// in flight is unknown, and claiming reachability we did not establish
    /// would be the same overstatement this distinction exists to prevent.
    const fn after_connectivity(self) -> bool {
        matches!(self, Self::SchemaMissing | Self::SchemaUnreadable)
    }

    /// Classifies a `sqlx` error without retaining its message.
    fn classify(error: &sqlx::Error) -> Self {
        match error {
            sqlx::Error::PoolTimedOut => Self::Timeout,
            // 42P01 undefined_table. The probe reads `_sqlx_migrations`, so this
            // means the migrator has never run here rather than that the
            // database is unreachable.
            sqlx::Error::Database(db) if db.code().as_deref() == Some("42P01") => {
                Self::SchemaMissing
            }
            sqlx::Error::Database(_) => Self::SchemaUnreadable,
            // Everything else — I/O, TLS, a closed pool, DNS, authentication —
            // means no usable connection. Enumerating those variants explicitly
            // would duplicate this arm without adding a distinction the caller
            // can act on.
            _ => Self::Connection,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/system/probe",
    tag = "system",
    responses((
        status = 200,
        description = "Reachability of the API and its database",
        body = SystemProbeResponse
    ))
)]
async fn system_probe(State(state): State<AppState>) -> Json<SystemProbeResponse> {
    let (database, schema_version) = match state.pool() {
        Some(pool) => probe_database(pool).await,
        None => (ProbeStatus::Unavailable, None),
    };

    Json(SystemProbeResponse {
        api: ProbeStatus::Ok,
        database,
        build_sha: BUILD_SHA.to_owned(),
        schema_version,
    })
}

/// Establishes reachability first, then schema state.
///
/// Two queries rather than one, because "the database is unreachable" and "the
/// database is fine but migrations have never run" are different faults with
/// different fixes, and a single query against `_sqlx_migrations` cannot tell
/// them apart — a missing table would report an empty database as unreachable.
/// That is the same conflation `schema_version` avoids by being nullable.
///
/// | Connectivity | Migrations   | Reported      |
/// |--------------|--------------|---------------|
/// | fails        | —            | `UNAVAILABLE` |
/// | succeeds     | absent/empty | `DEGRADED`    |
/// | succeeds     | applied      | `OK`          |
///
/// Returns `200` in every case. The probe reports dependency health as data;
/// failing the request would make "the API is up but the database is not"
/// indistinguishable from "the API is down", which is exactly the distinction
/// this endpoint exists to draw.
async fn probe_database(pool: &sqlx::PgPool) -> (ProbeStatus, Option<i64>) {
    match run_database_probe(pool).await {
        Ok(version) => {
            if version.is_some() {
                (ProbeStatus::Ok, version)
            } else {
                // The table exists but is empty: the migrator ran and applied
                // nothing.
                tracing::warn!("database is reachable but no migrations have been applied");
                (ProbeStatus::Degraded, None)
            }
        }
        Err(failure) => {
            // The phase matters more than the error. Once `SELECT 1` has
            // returned, connectivity is *proven*, so a later failure cannot
            // mean "unreachable" — it means reachable but not serving. That
            // distinction becomes load-bearing when the migration and runtime
            // roles hold different privileges: a permission error reading
            // `_sqlx_migrations` is a misconfigured grant, not a network fault,
            // and reporting it as UNAVAILABLE would send a reader looking in
            // entirely the wrong place.
            let status = if failure.after_connectivity() {
                ProbeStatus::Degraded
            } else {
                ProbeStatus::Unavailable
            };

            tracing::warn!(
                failure = failure.as_str(),
                status = status.as_str(),
                "system probe could not read the database"
            );
            (status, None)
        }
    }
}

/// Budget for the probe's own database work.
///
/// Strictly shorter than the router's request timeout. Without an inner budget,
/// a stalled database would hold the handler until the outer layer cut the
/// request and returned `408` — losing the `200` plus `UNAVAILABLE` this
/// endpoint promises, and turning "the database is hanging" into "the API is
/// broken". The distinction is the entire point of the endpoint, so it has to
/// survive the failure mode most likely to obscure it.
const DATABASE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs both queries under one shared deadline.
async fn run_database_probe(pool: &sqlx::PgPool) -> Result<Option<i64>, ProbeFailure> {
    let work = async {
        // Connectivity first. A single query against `_sqlx_migrations` could
        // not separate "unreachable" from "schema never applied": a missing
        // table would report an empty database as unreachable.
        //
        // Failures here are *always* connectivity failures, regardless of what
        // SQLx reports. `SELECT 1` touches no schema, so there is no
        // schema-shaped explanation for it failing, and classifying by error
        // kind could otherwise map it to a schema category and claim a
        // reachability we never established.
        sqlx::query("SELECT 1")
            .execute(pool)
            .await
            .map_err(|_| ProbeFailure::Connection)?;

        // Past this point connectivity is proven, so `classify` may attribute a
        // failure to the schema.
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT max(version) FROM _sqlx_migrations WHERE success",
        )
        .fetch_one(pool)
        .await
        .map_err(|error| ProbeFailure::classify(&error))
    };

    tokio::time::timeout(DATABASE_PROBE_TIMEOUT, work)
        .await
        .unwrap_or(Err(ProbeFailure::Timeout))
}

/// Builds the HTTP application together with the OpenAPI document it generates.
///
/// Returning both from one call is the point: they cannot drift apart, because
/// there is no way to obtain one without the other.
pub fn build(state: AppState) -> (Router, utoipa::openapi::OpenApi) {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(liveness))
        .routes(routes!(system_probe))
        .merge(analyses::routes())
        .with_state(state)
        .split_for_parts();

    // A statically hosted frontend calling this API is cross-origin, so the
    // browser blocks every request without this. Applied only when an exact
    // origin is configured — never a wildcard, which would have to be revisited
    // the moment an endpoint needs credentials, and which is a security
    // decision made by omission rather than by choice.
    let router = if let Some(origin) = config::cors_allowed_origin() {
        if let Ok(value) = origin.parse::<HeaderValue>() {
            tracing::info!(%origin, "CORS enabled for one exact origin");
            router.layer(
                CorsLayer::new()
                    .allow_origin(value)
                    .allow_methods([Method::GET])
                    .allow_headers([header::CONTENT_TYPE]),
            )
        } else {
            // Refusing to start would take the API down over a typo in a
            // variable that only affects browsers; serving without CORS fails
            // visibly in the browser and loudly in the log.
            tracing::error!(
                %origin,
                "CORS_ALLOWED_ORIGIN is not a valid header value; serving without CORS"
            );
            router
        }
    } else {
        tracing::info!("no CORS_ALLOWED_ORIGIN configured; serving without CORS");
        router
    };

    let router = router
        .layer(DefaultBodyLimit::max(REQUEST_BODY_LIMIT_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http());

    (router, openapi)
}

#[cfg(test)]
mod budget_tests {
    use super::{DATABASE_PROBE_TIMEOUT, REQUEST_TIMEOUT};

    #[test]
    fn probe_budget_is_strictly_shorter_than_the_request_budget() {
        // The endpoint promises 200 + UNAVAILABLE when the database stalls. That
        // promise holds only while the probe's own deadline fires first; if the
        // router's timeout won, the caller would get 408 and could not tell "the
        // database is hanging" from "the API is broken".
        //
        // Asserted as an invariant rather than by stalling a real database:
        // there is no pool to stall in a unit test, and a sleep-based test would
        // buy a slower suite without proving anything this does not.
        assert!(
            DATABASE_PROBE_TIMEOUT < REQUEST_TIMEOUT,
            "probe budget {DATABASE_PROBE_TIMEOUT:?} must be shorter than request timeout {REQUEST_TIMEOUT:?}"
        );
    }
}
