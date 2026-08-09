//! The operational snapshot endpoint.
//!
//! One route, and the reason it is on this service rather than beside it: the
//! figures it publishes are counters held in *this* process's memory, so
//! anything that had to reach them from outside would first have to make them
//! leave — a scrape endpoint, an exporter, a time-series database. None of that
//! is warranted until these numbers show it is, and issue #37 is explicit that
//! adopting the answer first would be adopting it before we have the question.
//!
//! The gate is the `Admin` extractor, and it is the whole of the access
//! control — it is crate-private, so this link is deliberately not one. `/admin`
//! being hard to find is not access control; the frontend only presents what
//! this endpoint has already decided to hand over.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;

use crate::api::authenticated::Admin;
use crate::contract::admin::{
    AdminOverview, HttpMethodClass, HttpOverview, LatencyPercentile, LatencySummary,
    ProcessOverview, RouteOverview, StatusClassCounts,
};
use crate::contract::error::ApiError;
use crate::state::{AppState, BUILD_SHA};
use crate::telemetry::metrics::{
    HistogramSnapshot, LatencyEstimate, MAX_TRACKED_ROUTES, MetricsSnapshot, RouteMethod,
    RouteSample, StatusClass,
};
use crate::telemetry::process;

#[utoipa::path(
    get,
    path = "/api/v1/admin/overview",
    tag = "admin",
    responses(
        (status = 200, description = "Operational snapshot of the process that answered", body = AdminOverview),
        (status = 401, description = "No valid Firebase ID token was presented", body = ApiError),
        (status = 403, description = "The caller is signed in and is not an administrator", body = ApiError),
        (status = 503, description = "Sign-in could not be checked, so the request was refused", body = ApiError),
        (status = 408, description = "The request exceeded the server time budget", body = ApiError),
        (status = 500, description = "An unhandled fault in this service", body = ApiError)
    )
)]
async fn overview(
    State(state): State<AppState>,
    // The gate. A handler that takes `Admin` cannot be reached without a
    // verified Firebase ID token whose uid is on the configured allowlist, and
    // that fact is visible here rather than in a layer that names paths.
    _admin: Admin,
) -> Json<AdminOverview> {
    // Read once, and only from the registry and the process. Nothing here
    // consults configuration, the environment, or the database — not because
    // those are filtered on the way out, but because they are never read, which
    // is the only version of that guarantee a reviewer can check by looking.
    Json(AdminOverview {
        process: ProcessOverview {
            build_sha: BUILD_SHA.to_owned(),
            uptime_seconds: process::uptime().as_secs(),
            resident_bytes: process::resident_bytes(),
        },
        http: http_overview(&state.metrics().snapshot()),
    })
}

/// Widens a count to the wire type.
///
/// Saturating rather than panicking. These are figures on a dashboard, and a
/// route that took a request down over the arithmetic in its own metrics would
/// be a worse outcome than a number pinned at its ceiling — which on any
/// platform this runs on is unreachable anyway, `usize` being at most 64 bits.
fn widen(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Translates the registry's view of HTTP into the published one.
fn http_overview(snapshot: &MetricsSnapshot) -> HttpOverview {
    HttpOverview {
        in_flight: snapshot.in_flight,
        tracked_routes: widen(snapshot.tracked_routes),
        max_tracked_routes: widen(MAX_TRACKED_ROUTES),
        // `filter_map` rather than `map`, because building a row needs
        // percentiles and a histogram with no observations has none. Two
        // separate things keep it from dropping anything: a series exists only
        // once something was recorded into it, and `Metrics::snapshot` reads the
        // whole registry under a barrier, so `count` can never exceed the
        // observations the buckets hold. Without that second property this
        // branch was reachable — a request completing mid-read could leave a
        // rank the bucket walk could not satisfy, and the route would vanish
        // from the table under load.
        //
        // Publishing zeroes instead is not the alternative: zero is
        // indistinguishable from a genuinely instant response, which is the one
        // thing a percentile here must never be able to mean. So a dropped row
        // stays dropped — and says so, because a table quietly missing a route
        // is the failure a reader cannot see.
        routes: snapshot
            .routes
            .iter()
            .filter_map(|sample| {
                let row = route_overview(sample);
                if row.is_none() {
                    // The label is safe to log: it is a matched pattern or a
                    // fixed string, never a caller-written path.
                    tracing::error!(
                        route = %sample.route,
                        requests = sample.count,
                        "a route was dropped from the operational snapshot: its histogram \
                         reported no percentile for requests it had counted"
                    );
                }
                row
            })
            .collect(),
    }
}

/// One published row, or nothing when the sample carries no measurement.
fn route_overview(sample: &RouteSample) -> Option<RouteOverview> {
    Some(RouteOverview {
        route: sample.route.clone(),
        method: method_class(sample.method),
        requests: sample.count,
        responses: status_class_counts(sample),
        latency: latency_summary(&sample.latency)?,
    })
}

/// Translates the internal method class into the published one.
///
/// An exhaustive match rather than a `Display` or a string field, and that is
/// the point: a variant added to [`RouteMethod`] fails to compile here instead
/// of reaching a client as a value the contract never declared. The two enums
/// are separate for the same reason a database row is never a DTO — the
/// registry's vocabulary is its own, and `<other>` is a log label rather than
/// something a consumer could switch on.
const fn method_class(method: RouteMethod) -> HttpMethodClass {
    match method {
        RouteMethod::Get => HttpMethodClass::Get,
        RouteMethod::Head => HttpMethodClass::Head,
        RouteMethod::Post => HttpMethodClass::Post,
        RouteMethod::Put => HttpMethodClass::Put,
        RouteMethod::Patch => HttpMethodClass::Patch,
        RouteMethod::Delete => HttpMethodClass::Delete,
        RouteMethod::Options => HttpMethodClass::Options,
        RouteMethod::Trace => HttpMethodClass::Trace,
        RouteMethod::Connect => HttpMethodClass::Connect,
        RouteMethod::Other => HttpMethodClass::Other,
    }
}

/// Reads the per-class counters by name rather than by position.
///
/// The internal counters are an array indexed by [`StatusClass::index`]. Going
/// through the named accessor is what stops a reordering of that enum from
/// silently relabelling every figure on the dashboard — the kind of change that
/// compiles, passes, and reports client errors as server errors.
fn status_class_counts(sample: &RouteSample) -> StatusClassCounts {
    StatusClassCounts {
        informational: sample.in_status_class(StatusClass::Informational),
        success: sample.in_status_class(StatusClass::Success),
        redirection: sample.in_status_class(StatusClass::Redirection),
        client_error: sample.in_status_class(StatusClass::ClientError),
        server_error: sample.in_status_class(StatusClass::ServerError),
        other: sample.in_status_class(StatusClass::Other),
    }
}

/// The three percentiles, read from one histogram snapshot.
///
/// All from the same snapshot, which is what makes them describe the same set
/// of observations. Reading the live counters once per percentile would let a
/// p50 taken after a burst exceed a p99 taken before it — a shape no reader
/// would think to distrust, because percentiles are not supposed to be able to
/// do that.
fn latency_summary(histogram: &HistogramSnapshot) -> Option<LatencySummary> {
    Some(LatencySummary {
        total_micros: histogram.sum_micros(),
        p50: percentile(histogram.percentile(50)?),
        p95: percentile(histogram.percentile(95)?),
        p99: percentile(histogram.percentile(99)?),
    })
}

/// Publishes an estimate together with the bucket it was read from.
const fn percentile(estimate: LatencyEstimate) -> LatencyPercentile {
    LatencyPercentile {
        micros: estimate.micros,
        lower_bound_micros: estimate.lower_bound_micros,
        upper_bound_micros: estimate.upper_bound_micros,
    }
}

/// The admin routes, for mounting on the application router.
pub fn routes() -> OpenApiRouter<AppState> {
    use utoipa_axum::routes;

    OpenApiRouter::new().routes(routes!(overview))
}

#[cfg(test)]
mod tests {
    use super::{http_overview, method_class};
    use crate::contract::admin::HttpMethodClass;
    use crate::telemetry::metrics::{Metrics, RouteMethod};
    use axum::http::StatusCode;
    use std::time::Duration;

    #[test]
    fn every_recorded_method_reaches_the_wire_under_its_own_name() {
        // The translation is the seam where a registry label could quietly
        // become a different published value. Asserted over `RouteMethod::ALL`
        // rather than a list written here, so a variant added to the registry
        // cannot be left untested by being forgotten in two places at once.
        for method in RouteMethod::ALL {
            let published = method_class(method);
            let wire = serde_json::to_string(&published).expect("serializes");
            let wire = wire.trim_matches('"');

            if method == RouteMethod::Other {
                // The one variant whose names deliberately differ: the registry
                // writes `<other>` for a log, and the contract publishes a token
                // a client can switch on.
                assert_eq!(method.as_str(), "<other>");
                assert_eq!(wire, "OTHER");
            } else {
                assert_eq!(
                    wire,
                    method.as_str(),
                    "{method:?} is published as something other than the method it names"
                );
            }
        }
    }

    #[test]
    fn distinct_methods_stay_distinct_after_translation() {
        // Two methods folding into one published value would merge two rows of
        // the route table into one, and the merged row would look like a single
        // busy endpoint rather than two. Compared pairwise rather than by
        // `dedup`, which only removes *adjacent* repeats and would miss a
        // collision between the first arm and the last.
        let published: Vec<HttpMethodClass> =
            RouteMethod::ALL.into_iter().map(method_class).collect();

        for (left, first) in published.iter().enumerate() {
            for second in &published[left + 1..] {
                assert_ne!(
                    first, second,
                    "two registry methods are published as the same value"
                );
            }
        }
    }

    #[test]
    fn the_published_table_carries_what_was_recorded() {
        let metrics = Metrics::new();
        for status in [StatusCode::OK, StatusCode::NOT_FOUND] {
            metrics.record_request(
                RouteMethod::Get,
                "/api/v1/analyses/{analysis_id}",
                status,
                Duration::from_micros(1_200),
            );
        }

        let overview = http_overview(&metrics.snapshot());

        assert_eq!(overview.tracked_routes, 1);
        assert_eq!(overview.routes.len(), 1);

        let row = &overview.routes[0];
        assert_eq!(row.route, "/api/v1/analyses/{analysis_id}");
        assert_eq!(row.method, HttpMethodClass::Get);
        assert_eq!(row.requests, 2);
        assert_eq!(row.responses.success, 1);
        assert_eq!(row.responses.client_error, 1);
        assert_eq!(row.responses.server_error, 0);
        assert_eq!(row.latency.total_micros, 2_400);
        // Both observations are in the 1 ms – 2.5 ms bucket, so every percentile
        // reports that bucket and an estimate inside it. Asserting the bounds
        // rather than only the estimate is what proves the resolution travelled
        // with the figure instead of being dropped in translation.
        assert_eq!(row.latency.p50.lower_bound_micros, 1_000);
        assert_eq!(row.latency.p99.upper_bound_micros, Some(2_500));
    }

    #[test]
    fn an_untouched_process_publishes_an_empty_table_rather_than_zeroes() {
        // "Nothing has been measured" is not a measurement of zero. A row of
        // zeroes for a route nobody called would report a p99 for an endpoint
        // that has never been reached.
        let overview = http_overview(&Metrics::new().snapshot());

        assert!(overview.routes.is_empty());
        assert_eq!(overview.tracked_routes, 0);
        assert_eq!(overview.in_flight, 0);
        assert_eq!(overview.max_tracked_routes, 64);
    }
}
