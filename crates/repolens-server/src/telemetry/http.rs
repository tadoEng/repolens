//! The middleware that records a request into the registry.
//!
//! One layer, mounted by `api::apply_layers`, and the only shipped caller of
//! [`Metrics::record_request`]. Everything that decides what a label may contain
//! is decided here, in one function, rather than being spread across the
//! handlers that would each have to remember it.

use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use super::metrics::{Metrics, RouteMethod, UNMATCHED_ROUTE};

/// Records one request: its count, latency, status class, and its presence
/// while it is in flight.
///
/// # Where the route label comes from
///
/// [`MatchedPath`] is the pattern the router matched —
/// `/api/v1/analyses/{analysis_id}` — not the path the caller sent. Reading
/// [`Request::uri`] instead would put an analysis id into a map key, and with it
/// anything else a caller can write into a path: a repository name, a token
/// somebody pasted into a URL, an email in a query string. The extractor is what
/// makes that structural. There is no sanitiser here to get wrong, because the
/// identifying half of the path never arrives in the first place.
///
/// A request that matched no route has no pattern, and the URI is exactly the
/// wrong thing to fall back to — it is the one input an anonymous stranger
/// writes in full, so a 404 flood would be an unbounded label set on demand.
/// Those requests share [`UNMATCHED_ROUTE`], which costs one series no matter
/// how many distinct paths are probed.
///
/// # Where it sits in the stack
///
/// Outside the panic layer and inside the tracing layer. Outside the panic
/// layer, because a handler that panics has already become a `500` by the time
/// this sees it, and a `5xx` that was never counted is the single most useful
/// number missing from a dashboard. Inside `MatchedPath`'s reach, because every
/// layer in `api::apply_layers` is applied through `Router::layer` and therefore
/// runs after routing — which is what makes the extractor available to a
/// middleware at all.
pub async fn record(State(metrics): State<Metrics>, request: Request, next: Next) -> Response {
    let method = RouteMethod::classify(request.method());
    // Cloned before the request is consumed, which costs an `Arc` bump rather
    // than an allocation: `MatchedPath` is a handle to a string the router
    // already owns.
    let matched = request.extensions().get::<MatchedPath>().cloned();

    // Raised before the handler and lowered when this guard drops, so an unwind
    // cannot leave the gauge high.
    let _in_flight = metrics.enter_request();

    let started = Instant::now();
    let response = next.run(request).await;
    let latency = started.elapsed();

    metrics.record_request(
        method,
        matched
            .as_ref()
            .map_or(UNMATCHED_ROUTE, MatchedPath::as_str),
        response.status(),
        latency,
    );

    response
}
