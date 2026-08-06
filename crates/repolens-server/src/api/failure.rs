//! One error envelope, for every failure the API can produce.
//!
//! A handler that builds an [`ApiError`] is the easy half. The hard half is
//! everything that never reaches a handler: a malformed JSON body, a path
//! parameter that is not a UUID, a body over the limit, a request that ran out
//! of time, a panic. Each of those is answered by `axum` or by a `tower` layer,
//! and each answers in its *own* format — a plain-text line, or nothing at all.
//!
//! That makes the envelope a promise the API only sometimes keeps, which is
//! worse than not promising it. A client written against the contract has to
//! parse JSON on some failures and sniff `content-type` on others, and the
//! generated TypeScript client — which types every error as `ApiError` — is
//! simply wrong for the cases it cannot see. So the conversions live here, in
//! one place, and the router wires all of them.
//!
//! Messages are fixed strings. None of them echoes the request: a rejection
//! message from `serde` or from a panic payload can carry body fragments,
//! internal paths, or retrieved repository content, and this string crosses
//! into a browser.

use std::any::Any;

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use tower_http::catch_panic::ResponseForPanic;

use crate::contract::error::{ApiError, ErrorCode};

/// An error on its way out of a handler, or out of the framework.
///
/// Carries the status alongside the body so the mapping from "what went wrong"
/// to "what HTTP says" lives in one place rather than at every call site.
pub(crate) struct Failure(pub(crate) StatusCode, pub(crate) ApiError);

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

impl Failure {
    pub(crate) fn bad_request(code: ErrorCode, message: &str) -> Self {
        Self(StatusCode::BAD_REQUEST, ApiError::new(code, message))
    }

    /// No analysis — or no report for one — with that identifier.
    ///
    /// `ANALYSIS_NOT_FOUND`, never `REPOSITORY_NOT_FOUND`. The latter says a
    /// repository is absent or private on GitHub, which is a different fact
    /// with a different remedy: it would send the caller to check a URL that
    /// was never the problem, and the UI offers to correct one.
    pub(crate) fn analysis_not_found(message: &str) -> Self {
        Self(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::AnalysisNotFound, message),
        )
    }

    /// The analysis exists; its report does not yet.
    ///
    /// Still `404` — the resource genuinely is not there — but under its own
    /// code, because the remedy is opposite to the one above: keep polling
    /// rather than correct the identifier.
    pub(crate) fn report_not_available(message: &str) -> Self {
        Self(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::ReportNotAvailable, message),
        )
    }

    /// The database is unreachable or misconfigured.
    ///
    /// `503` rather than `500`: the service is fine, its dependency is not, and
    /// the distinction tells a caller whether retrying is worth anything.
    pub(crate) fn unavailable() -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                ErrorCode::WorkerFailedRetriable,
                "The analysis store is unavailable. This is usually temporary.",
            ),
        )
    }
}

impl From<JsonRejection> for Failure {
    /// Maps `axum`'s own JSON rejection onto the envelope.
    ///
    /// The rejection's status is kept rather than flattened to `400`: `415` for
    /// a missing `Content-Type`, `422` for a well-formed body of the wrong
    /// shape, and `413` when the body limit tripped are all more useful to a
    /// caller than one undifferentiated code.
    ///
    /// The body limit surfaces *here* rather than needing its own interception,
    /// because `DefaultBodyLimit` makes reading the body fail and this
    /// extractor is what reads it.
    fn from(rejection: JsonRejection) -> Self {
        let status = rejection.status();

        if status == StatusCode::PAYLOAD_TOO_LARGE {
            return Self(
                status,
                ApiError::new(
                    ErrorCode::RequestTooLarge,
                    "The request body is larger than this endpoint accepts.",
                ),
            );
        }

        Self(
            status,
            ApiError::new(
                ErrorCode::MalformedRequest,
                "The request body could not be read as the JSON this endpoint expects. Check \
                 that it is valid JSON and that Content-Type is application/json.",
            ),
        )
    }
}

impl From<PathRejection> for Failure {
    /// Maps a path-parameter rejection onto the envelope.
    ///
    /// Reached when `{analysis_id}` is not a UUID. `400` rather than `404`:
    /// the caller sent something that could never identify an analysis, which
    /// is a different fact from a well-formed id that does not exist, and
    /// collapsing them would tell a client to stop retrying a typo it could
    /// fix.
    fn from(rejection: PathRejection) -> Self {
        Self(
            rejection.status(),
            ApiError::new(
                ErrorCode::MalformedRequest,
                "That is not a valid analysis identifier.",
            ),
        )
    }
}

/// `axum::Json`, rejecting through [`Failure`].
///
/// A newtype rather than a blanket change, because the substitution has to be
/// visible at the handler signature: an endpoint that goes back to plain `Json`
/// silently stops honouring the envelope, and a reviewer can see this name.
pub(crate) struct TypedJson<T>(pub(crate) T);

impl<S, T> FromRequest<S> for TypedJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = Failure;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state).await?;
        Ok(Self(value))
    }
}

/// `axum::extract::Path`, rejecting through [`Failure`].
pub(crate) struct TypedPath<T>(pub(crate) T);

impl<S, T> FromRequestParts<S> for TypedPath<T>
where
    axum::extract::Path<T>: FromRequestParts<S, Rejection = PathRejection>,
    S: Send + Sync,
{
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(value) =
            axum::extract::Path::<T>::from_request_parts(parts, state).await?;
        Ok(Self(value))
    }
}

/// Rewrites the timeout layer's bare `408` into the envelope.
///
/// `TimeoutLayer` builds its response itself, without passing through any
/// extractor, so it is the one framework failure that cannot be intercepted at
/// the point it is produced. It is caught on the way out instead.
///
/// Keyed on the status alone, which is sound because no handler in this API
/// returns `408` — the only producer of that status in the stack is the layer
/// this function is mounted directly outside of. A route that ever needs to
/// return `408` itself must build the envelope and this note must change.
pub(crate) async fn envelope_timeouts(response: Response) -> Response {
    if response.status() != StatusCode::REQUEST_TIMEOUT {
        return response;
    }

    Failure(
        StatusCode::REQUEST_TIMEOUT,
        ApiError::new(
            ErrorCode::RequestTimedOut,
            "The request took longer than this API allows. This is usually temporary.",
        ),
    )
    .into_response()
}

/// Turns a caught panic into the envelope.
///
/// Replaces `CatchPanicLayer::new()`, whose default response is a plain-text
/// body — the single most surprising thing a client can receive from a JSON
/// API, and the one it is least likely to have a code path for.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PanicEnvelope;

impl ResponseForPanic for PanicEnvelope {
    type ResponseBody = axum::body::Body;

    fn response_for_panic(&mut self, panic: Box<dyn Any + Send + 'static>) -> Response {
        // The payload is dropped without being read, and that is the point.
        //
        // A panic message is built by whatever code panicked, so it can carry
        // an internal path, a fragment of a repository file the handler was
        // holding, or a value read from configuration. Writing it to a log is
        // the same disclosure as returning it, only to a different reader —
        // and logs are the copy that gets shipped to a third party, retained,
        // and searched. `config.rs` already refuses to echo a value it
        // rejected for exactly this reason; this is the same rule.
        //
        // What is left is the fact and the location, which is what a panic in
        // a request actually needs: the tracing span carries the route, and a
        // panic hook installed at startup is where a backtrace belongs.
        drop(panic);
        tracing::error!("a request handler panicked; the payload is deliberately not recorded");

        Failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                ErrorCode::InternalError,
                "Something went wrong in this service. The failure has been recorded.",
            ),
        )
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;

    #[test]
    fn a_timeout_is_retriable_and_a_malformed_request_is_not() {
        // The UI offers a retry control from this. A timeout says the
        // deployment was slow; a malformed request says the caller must change
        // what it sends, and retrying it unchanged will fail identically.
        assert!(ErrorCode::RequestTimedOut.is_retriable());
        assert!(!ErrorCode::MalformedRequest.is_retriable());
        assert!(!ErrorCode::RequestTooLarge.is_retriable());
        assert!(!ErrorCode::InternalError.is_retriable());
    }
}
