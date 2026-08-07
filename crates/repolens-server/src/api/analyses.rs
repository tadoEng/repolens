//! Analysis creation and reading.
//!
//! Three routes, no more: create one, watch it, read its report. Everything the
//! frontend already renders is served by these; nothing here anticipates a
//! screen that does not exist.
//!
//! There is deliberately **no retry endpoint**. Starting work is an
//! authenticated operation, and authentication is issue #13. A route that
//! accepted retries from anyone would be the abuse surface the auth gate exists
//! to close, and the frontend correctly offers no control for it.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use repolens_core::RepositoryCoordinate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::failure::{Failure, TypedJson, TypedPath};
use crate::contract::analysis::Analysis;
use crate::contract::error::{ApiError, ErrorCode};
use crate::contract::report::Report;
use crate::state::AppState;
use crate::{pipeline, store};

/// What a caller submits to start an analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateAnalysisRequest {
    /// A public GitHub repository URL, e.g. `https://github.com/rust-lang/crates.io`.
    ///
    /// A URL rather than separate owner and name fields, because a URL is what a
    /// person has in their clipboard. Parsing it is our job, not theirs.
    pub repository_url: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/analyses",
    tag = "analyses",
    request_body = CreateAnalysisRequest,
    responses(
        (status = 202, description = "Accepted; the analysis is queued", body = Analysis),
        (status = 400, description = "The URL is not a public GitHub repository", body = ApiError),
        (status = 413, description = "The request body is over the limit", body = ApiError),
        (status = 415, description = "Content-Type is not application/json", body = ApiError),
        (status = 422, description = "The body is JSON but not this request", body = ApiError),
        (status = 503, description = "The analysis store is unavailable", body = ApiError),
        (status = 408, description = "The request exceeded the server time budget", body = ApiError),
        (status = 500, description = "An unhandled fault in this service", body = ApiError)
    )
)]
async fn create(
    State(state): State<AppState>,
    // `TypedJson`, not `Json`: a plain `Json` rejection is answered by `axum`
    // in its own format, so a malformed body would bypass the error envelope
    // this API promises. See `super::failure`.
    TypedJson(request): TypedJson<CreateAnalysisRequest>,
) -> Result<(StatusCode, Json<Analysis>), Failure> {
    let coordinate = parse_repository_url(&request.repository_url).ok_or_else(|| {
        Failure::bad_request(
            ErrorCode::InvalidRepositoryUrl,
            "That is not a GitHub repository URL. It should look like \
             https://github.com/owner/repository.",
        )
    })?;

    let pool = state.pool().ok_or_else(Failure::unavailable)?;

    let analysis = store::create_analysis(pool, &coordinate)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "could not create an analysis");
            Failure::unavailable()
        })?;

    // Inline execution, spawned so the response is not held for the length of an
    // analysis. Issue #7 replaces this with a durable claim; until then an
    // analysis does not survive the process, which is recorded in `pipeline`.
    //
    // There is no branch here for missing credentials. The client exists
    // whether or not a token was configured, and an unauthenticated request to
    // a public repository is a request GitHub may well answer. If it declines —
    // rate limit, removal, or anything else — that response is what decides the
    // error code, rather than this process deciding on its behalf.
    let (pool, source, id) = (
        pool.clone(),
        std::sync::Arc::clone(state.github()),
        analysis.id,
    );
    tokio::spawn(async move { pipeline::run(&pool, &*source, id, &coordinate).await });

    // 202, not 201: the analysis exists, but the thing the caller actually wants
    // does not yet.
    //
    // No `Location` header. The body already carries `id`, and the generated
    // client reads the body rather than response headers — a header would be a
    // second copy of the identity that nothing consumes, and one the OpenAPI
    // document would then have to describe. Callers poll
    // `/api/v1/analyses/{id}` at `poll_after_ms`.
    Ok((StatusCode::ACCEPTED, Json(analysis)))
}

#[utoipa::path(
    get,
    path = "/api/v1/analyses/{analysis_id}",
    tag = "analyses",
    params(("analysis_id" = Uuid, Path, description = "Analysis identifier")),
    responses(
        (status = 200, description = "Current state of the analysis", body = Analysis),
        (status = 400, description = "The identifier is not a UUID", body = ApiError),
        (status = 404, description = "No such analysis", body = ApiError),
        (status = 503, description = "The analysis store is unavailable", body = ApiError),
        (status = 408, description = "The request exceeded the server time budget", body = ApiError),
        (status = 500, description = "An unhandled fault in this service", body = ApiError)
    )
)]
async fn read(
    State(state): State<AppState>,
    TypedPath(analysis_id): TypedPath<Uuid>,
) -> Result<Json<Analysis>, Failure> {
    let pool = state.pool().ok_or_else(Failure::unavailable)?;

    store::load_analysis(pool, analysis_id)
        .await
        .map(Json)
        .map_err(|error| match error {
            store::StoreError::NotFound => {
                Failure::analysis_not_found("No analysis with that identifier.")
            }
            // Unreachable from this query, which reads the analysis rather than
            // its report. Matched explicitly rather than with a wildcard so a
            // future variant fails to compile here instead of silently
            // becoming a 503.
            other @ (store::StoreError::Query(_) | store::StoreError::ReportNotReady) => {
                tracing::error!(error = %other, "could not read an analysis");
                Failure::unavailable()
            }
        })
}

#[utoipa::path(
    get,
    path = "/api/v1/analyses/{analysis_id}/report",
    tag = "analyses",
    params(("analysis_id" = Uuid, Path, description = "Analysis identifier")),
    responses(
        (status = 200, description = "The completed report", body = Report),
        (status = 400, description = "The identifier is not a UUID", body = ApiError),
        (status = 404, description = "No report for that analysis", body = ApiError),
        (status = 503, description = "The analysis store is unavailable", body = ApiError),
        (status = 408, description = "The request exceeded the server time budget", body = ApiError),
        (status = 500, description = "An unhandled fault in this service", body = ApiError)
    )
)]
async fn read_report(
    State(state): State<AppState>,
    TypedPath(analysis_id): TypedPath<Uuid>,
) -> Result<Json<Report>, Failure> {
    let pool = state.pool().ok_or_else(Failure::unavailable)?;

    store::load_report(pool, analysis_id)
        .await
        .map(Json)
        .map_err(|error| match error {
            // Two facts, two codes. "There is no such analysis" tells a client
            // its identifier is wrong; "the report is not ready" tells it to
            // keep polling. One code for both was false in the second case and
            // told a caller to give up on work that was still running.
            store::StoreError::NotFound => {
                Failure::analysis_not_found("No analysis with that identifier.")
            }
            store::StoreError::ReportNotReady => Failure::report_not_available(
                "That analysis has not produced a report. It may still be running, or it may \
                 have failed — the analysis itself says which.",
            ),
            other @ store::StoreError::Query(_) => {
                tracing::error!(error = %other, "could not read a report");
                Failure::unavailable()
            }
        })
}

/// Extracts owner and name from a GitHub repository URL.
///
/// Accepts what a person actually pastes: with or without a scheme, with or
/// without `www.`, with a trailing slash, with `.git`, and with extra path
/// segments (`/tree/main`, `/blob/…`) that a browser adds when you copy from a
/// file view.
///
/// Rejects anything not on `github.com`. Phase 0 analyses public GitHub
/// repositories, and silently accepting another host would produce a confusing
/// failure much later instead of an actionable one now.
fn parse_repository_url(input: &str) -> Option<RepositoryCoordinate> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strip the scheme by hand rather than parsing: a bare `github.com/a/b` has
    // no scheme, and prepending one to parse it is the same work with an extra
    // dependency in the path.
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_www = without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme);

    let rest = without_www.strip_prefix("github.com/")?;

    let mut segments = rest.split('/').filter(|segment| !segment.is_empty());
    let owner = segments.next()?;
    let name = segments.next()?.trim_end_matches(".git");

    if owner.is_empty() || name.is_empty() {
        return None;
    }
    // A path segment cannot contain these, so their presence means the input was
    // a query string or fragment rather than a repository.
    if [owner, name]
        .iter()
        .any(|part| part.contains('?') || part.contains('#') || part.contains(' '))
    {
        return None;
    }

    Some(RepositoryCoordinate::new(owner, name))
}

/// The analysis routes, for mounting on the application router.
pub fn routes() -> utoipa_axum::router::OpenApiRouter<AppState> {
    use utoipa_axum::routes;

    utoipa_axum::router::OpenApiRouter::new()
        .routes(routes!(create))
        .routes(routes!(read))
        .routes(routes!(read_report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_what_a_person_actually_pastes() {
        for input in [
            "https://github.com/rust-lang/crates.io",
            "http://github.com/rust-lang/crates.io",
            "https://www.github.com/rust-lang/crates.io",
            "github.com/rust-lang/crates.io",
            "https://github.com/rust-lang/crates.io/",
            "https://github.com/rust-lang/crates.io.git",
            // What a browser gives you when copying from a file view.
            "https://github.com/rust-lang/crates.io/tree/main",
            "https://github.com/rust-lang/crates.io/blob/main/Cargo.toml",
            "  https://github.com/rust-lang/crates.io  ",
        ] {
            let parsed = parse_repository_url(input);
            assert_eq!(
                parsed,
                Some(RepositoryCoordinate::new("rust-lang", "crates.io")),
                "should accept {input:?}"
            );
        }
    }

    #[test]
    fn rejects_anything_that_is_not_a_github_repository() {
        for input in [
            "",
            "   ",
            "not a url",
            "https://github.com",
            "https://github.com/",
            // An owner with no repository is not analysable.
            "https://github.com/rust-lang",
            // Another host: rejected now with an actionable message rather than
            // failing confusingly during ingestion.
            "https://gitlab.com/owner/repo",
            "https://example.com/rust-lang/crates.io",
            "https://github.com/owner/repo?tab=readme",
            "https://github.com/owner/repo#readme",
        ] {
            assert_eq!(parse_repository_url(input), None, "should reject {input:?}");
        }
    }
}
