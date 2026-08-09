//! The operational snapshot, as an administrator sees it.
//!
//! Every figure here describes **the process that answered the request**, and
//! nothing else. There is no aggregation across instances, no history, and no
//! persistence: the numbers are counters held in memory since this process
//! started, so a restart resets them and a second instance would answer with its
//! own. A reader who forgets that will mistake a rollout for a traffic collapse,
//! which is why the sentence is part of the published contract rather than a
//! note on a dashboard someone may not have written yet.
//!
//! # What is deliberately absent
//!
//! No environment value, connection string, token, project id, or allow-listed
//! uid appears in any type here — not filtered out on the way past, but absent,
//! so there is no field for one to arrive in. That is asserted against the
//! serialized response bytes in `tests/admin.rs` rather than by reading this
//! paragraph.
//!
//! Nothing here reports the database, the analyzer, or per-phase timings either.
//! Those are named in issue #37's Stage A list and are **not instrumented yet**;
//! publishing a section assembled from whatever happened to be readable would be
//! the same over-claim the report contract refuses when it declines to turn an
//! abandoned line count into a zero.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The request method, folded into the closed set this service records.
///
/// A wire enum of its own rather than the internal `RouteMethod`, translated by
/// an exhaustive match in `api::admin`. HTTP methods are an *extensible* token —
/// `hyper` will deliver `WHATEVER /healthz` — so publishing the method as a free
/// string would tell a consumer the set is open when the entire cardinality
/// guarantee is that it is closed. Everything outside this set arrives as
/// [`Other`](Self::Other).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HttpMethodClass {
    /// `GET`.
    Get,
    /// `HEAD`.
    Head,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `PATCH`.
    Patch,
    /// `DELETE`.
    Delete,
    /// `OPTIONS`, which is also what a CORS preflight arrives as.
    Options,
    /// `TRACE`.
    Trace,
    /// `CONNECT`.
    Connect,
    /// Any other token a client sent, folded together.
    Other,
}

/// Responses counted by status class.
///
/// Named fields rather than an array, because an array's meaning is its index
/// order and no consumer can check that it still holds. A field named
/// `server_error` cannot be silently reordered into the position `client_error`
/// used to occupy.
///
/// A class rather than the code: `404` and `410` answer the same operational
/// question, and a counter per code would be several hundred figures at a
/// resolution nothing reads at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StatusClassCounts {
    /// `1xx`.
    pub informational: u64,
    /// `2xx`.
    pub success: u64,
    /// `3xx`.
    pub redirection: u64,
    /// `4xx` — the caller's fault.
    pub client_error: u64,
    /// `5xx` — ours.
    pub server_error: u64,
    /// Any other status the HTTP crate admits. No handler here produces one.
    pub other: u64,
}

/// One percentile, carrying the resolution it was read at.
///
/// `micros` is a linear interpolation *inside one bucket*, which assumes the
/// observations in that bucket are spread evenly across it. They are not, and
/// nothing measured how they actually are. The bounds are what the histogram
/// genuinely knows — the answer lies between them — and they travel with the
/// estimate rather than being recoverable only by consulting a bucket table
/// elsewhere, so a consumer cannot take the interpolation for a measurement
/// without first stepping over the two fields that say it is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LatencyPercentile {
    /// Interpolated estimate, in microseconds.
    pub micros: u64,
    /// Lower bound of the bucket the percentile fell in.
    pub lower_bound_micros: u64,
    /// Upper bound of that bucket.
    ///
    /// **Null for the overflow bucket**, where the histogram recorded that an
    /// observation was slower than the last bound and nothing further. Any
    /// figure here would be invented, and `micros` is then the last bound itself
    /// — a floor rather than an estimate, which a UI has to render differently.
    ///
    /// Required-but-nullable rather than optional, so a consumer cannot skip the
    /// case: the field is always present, its value is not.
    #[schema(required)]
    pub upper_bound_micros: Option<u64>,
}

/// The latency distribution for one route and method.
///
/// No mean is published. `total_micros` and the request count it belongs to are
/// what a mean is computed *from*, and they are strictly more useful: a
/// consumer that wants the average can divide, and one that wants to know how
/// much time a route has cost in total cannot recover it from an average.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LatencySummary {
    /// Total observed latency across every recorded request, in microseconds.
    pub total_micros: u64,
    /// Median.
    pub p50: LatencyPercentile,
    /// 95th percentile.
    pub p95: LatencyPercentile,
    /// 99th percentile.
    ///
    /// Of a hundred observations this is the ninety-ninth, so exactly one is
    /// slower than the figure reported. The slowest observation is reached at
    /// p100 and nowhere earlier — expecting p99 to follow the tail is the
    /// common misreading, and the bucket bounds on each estimate are what let a
    /// reader check rather than assume.
    pub p99: LatencyPercentile,
}

/// Counters for one route label and one method class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RouteOverview {
    /// The normalised route label.
    ///
    /// Always the *matched* router pattern — `/api/v1/analyses/{analysis_id}` —
    /// never the path a caller sent, so no analysis id, repository name, or
    /// pasted token can arrive here. Two labels are not patterns and say so:
    /// `<unmatched>` for a request that matched no route, and `<overflow>` for
    /// requests folded together once the registry stopped distinguishing.
    pub route: String,
    /// The method class.
    pub method: HttpMethodClass,
    /// Requests recorded for this route and method.
    pub requests: u64,
    /// How those requests were answered.
    pub responses: StatusClassCounts,
    /// How long they took.
    pub latency: LatencySummary,
}

/// What the process knows about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProcessOverview {
    /// Commit this binary was built from, or `unknown` for a local build.
    pub build_sha: String,
    /// Seconds since the process started, on a monotonic clock.
    ///
    /// Unaffected by the wall clock being corrected underneath it, and anchored
    /// at the top of `main` rather than at the first read — a process up for
    /// days must not report an uptime of seconds.
    pub uptime_seconds: u64,
    /// Resident set size in bytes, where the platform can answer.
    ///
    /// **Null off Linux**, which has no `/proc/self/status` to read it from and
    /// is where development happens. A plausible zero would turn "we cannot
    /// measure this here" into "the process uses no memory" — unknown is not
    /// zero, the same rule the report contract keeps for a truncated tree.
    ///
    /// Required-but-nullable, so a consumer must render the unknown case rather
    /// than being permitted to forget it exists.
    #[schema(required)]
    pub resident_bytes: Option<u64>,
}

/// What the process has observed about the requests it served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HttpOverview {
    /// Requests being served at the moment the snapshot was taken.
    pub in_flight: u64,
    /// Distinct route labels currently held.
    pub tracked_routes: u64,
    /// The ceiling on that number.
    ///
    /// Published so a reader can tell a quiet registry from a full one without
    /// knowing the constant. Once `tracked_routes` reaches this, further labels
    /// are folded into the `<overflow>` row rather than allocated — which is
    /// what keeps the memory bound arithmetic rather than a hope about how the
    /// router behaves.
    pub max_tracked_routes: u64,
    /// One row per route and method class that has served a request.
    ///
    /// Sorted by route then method, and combinations with no requests are
    /// omitted entirely — so the labels present are exactly the labels this
    /// process recorded, rather than a grid of zeroes that would hide which
    /// ones were real.
    pub routes: Vec<RouteOverview>,
}

/// The operational snapshot of one process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdminOverview {
    /// Facts about the process itself.
    pub process: ProcessOverview,
    /// Facts about the requests it has served.
    pub http: HttpOverview,
}

#[cfg(test)]
mod tests {
    use super::{
        AdminOverview, HttpMethodClass, HttpOverview, LatencyPercentile, LatencySummary,
        ProcessOverview, RouteOverview, StatusClassCounts,
    };

    fn overview(resident_bytes: Option<u64>, upper_bound_micros: Option<u64>) -> AdminOverview {
        AdminOverview {
            process: ProcessOverview {
                build_sha: "unknown".to_owned(),
                uptime_seconds: 1,
                resident_bytes,
            },
            http: HttpOverview {
                in_flight: 0,
                tracked_routes: 1,
                max_tracked_routes: 64,
                routes: vec![RouteOverview {
                    route: "/healthz".to_owned(),
                    method: HttpMethodClass::Get,
                    requests: 1,
                    responses: StatusClassCounts {
                        informational: 0,
                        success: 1,
                        redirection: 0,
                        client_error: 0,
                        server_error: 0,
                        other: 0,
                    },
                    latency: LatencySummary {
                        total_micros: 400,
                        p50: LatencyPercentile {
                            micros: 400,
                            lower_bound_micros: 0,
                            upper_bound_micros,
                        },
                        p95: LatencyPercentile {
                            micros: 400,
                            lower_bound_micros: 0,
                            upper_bound_micros,
                        },
                        p99: LatencyPercentile {
                            micros: 400,
                            lower_bound_micros: 0,
                            upper_bound_micros,
                        },
                    },
                }],
            },
        }
    }

    #[test]
    fn method_classes_serialize_as_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&HttpMethodClass::Get).unwrap(),
            "\"GET\""
        );
        // The one variant whose Rust name and wire name could plausibly drift:
        // the internal registry labels it `<other>`, which is not a contract
        // vocabulary a client can switch on.
        assert_eq!(
            serde_json::to_string(&HttpMethodClass::Other).unwrap(),
            "\"OTHER\""
        );
    }

    #[test]
    fn unmeasurable_memory_is_serialized_as_null_rather_than_omitted() {
        // The field is required-but-nullable. Omitting it would let a consumer
        // treat "this platform cannot report memory" as a field it may ignore,
        // and the whole reason it is nullable is that it must be rendered.
        let json = serde_json::to_value(overview(None, Some(500))).unwrap();
        let resident = &json["process"]["resident_bytes"];

        assert!(
            json["process"].get("resident_bytes").is_some(),
            "resident_bytes must be present even when unknown"
        );
        assert!(resident.is_null(), "and null rather than zero");
    }

    #[test]
    fn an_overflow_percentile_is_serialized_as_a_null_upper_bound() {
        // Past the last bucket bound the histogram knows a floor and nothing
        // else. An omitted field here would read as "no bound applies" instead
        // of "this figure is a floor", which is the difference between rendering
        // `>10s` and rendering `10ms`.
        let json = serde_json::to_value(overview(Some(1), None)).unwrap();
        let bound = &json["http"]["routes"][0]["latency"]["p99"]["upper_bound_micros"];

        assert!(
            json["http"]["routes"][0]["latency"]["p99"]
                .get("upper_bound_micros")
                .is_some(),
            "upper_bound_micros must be present"
        );
        assert!(bound.is_null());
    }

    #[test]
    fn the_snapshot_round_trips() {
        let original = overview(Some(12_345), Some(500));
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<AdminOverview>(&json).unwrap(),
            original
        );
    }
}
