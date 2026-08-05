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
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::{AppState, BUILD_SHA};

/// Ceiling on request bodies. Every current and planned endpoint takes either
/// nothing or one repository URL, so this is generous rather than tight.
const REQUEST_BODY_LIMIT_BYTES: usize = 16 * 1024;

/// Ceiling on how long a request may occupy a Cloud Run instance. Analysis is
/// performed by the worker, never inside a request, so no endpoint has a
/// legitimate reason to approach this.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Root OpenAPI document. Paths and schemas are contributed by the router.
#[derive(OpenApi)]
#[openapi(info(
    title = "RepoLens API",
    description = "Deterministic, evidence-backed analysis of a public GitHub repository at an exact commit."
))]
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
    pub schema_version: Option<i64>,
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
    // Errors are logged by shape, never interpolated wholesale: a connection
    // error can carry the URL, and the URL carries a password.
    if let Err(error) = sqlx::query("SELECT 1").execute(pool).await {
        tracing::warn!(error = %error, "system probe could not reach the database");
        return (ProbeStatus::Unavailable, None);
    }

    let applied = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT max(version) FROM _sqlx_migrations WHERE success",
    )
    .fetch_one(pool)
    .await;

    match applied {
        Ok(Some(version)) => (ProbeStatus::Ok, Some(version)),
        // The table exists but is empty: the migrator ran and applied nothing.
        Ok(None) => {
            tracing::warn!("database is reachable but no migrations have been applied");
            (ProbeStatus::Degraded, None)
        }
        // Almost always `undefined_table`: the migrator has never run here.
        // Reachable, so not unavailable — but not ready to serve either.
        Err(error) => {
            tracing::warn!(error = %error, "database is reachable but the schema is not initialised");
            (ProbeStatus::Degraded, None)
        }
    }
}

/// Builds the HTTP application together with the OpenAPI document it generates.
///
/// Returning both from one call is the point: they cannot drift apart, because
/// there is no way to obtain one without the other.
pub fn build(state: AppState) -> (Router, utoipa::openapi::OpenApi) {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(liveness))
        .routes(routes!(system_probe))
        .with_state(state)
        .split_for_parts();

    // CORS is intentionally absent: the allowed origin is the deployed
    // Cloudflare domain, which does not exist yet. A permissive default now
    // would be a security decision made by omission.
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
