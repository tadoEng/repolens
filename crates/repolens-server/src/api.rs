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
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

/// Builds the HTTP application together with the OpenAPI document it generates.
///
/// Returning both from one call is the point: they cannot drift apart, because
/// there is no way to obtain one without the other.
pub fn build() -> (Router, utoipa::openapi::OpenApi) {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(liveness))
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
