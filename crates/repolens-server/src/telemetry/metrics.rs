//! In-process request metrics, bounded by construction.
//!
//! Everything here answers one question — how much of a request is this
//! service? — without a metrics dependency, a scrape endpoint, or a time-series
//! database. What that buys is one process-local aggregate whose memory ceiling
//! can be stated in a sentence. What it costs is history, which nothing needs
//! yet: adopting Prometheus before these numbers exist would be adopting an
//! answer before we have the question.
//!
//! # Cardinality is a memory bound, not a style preference
//!
//! A label is a map key, and a map keyed by anything a caller writes is a map a
//! caller can grow. `/api/v1/analyses/019fdb48-…` is a distinct key per
//! analysis, so labelling by concrete path would let a few thousand anonymous
//! requests pin an unbounded amount of memory in a process that never restarts.
//!
//! Three separate things keep that closed, and the third is the one that
//! matters most:
//!
//! 1. the route label is the *matched* pattern, supplied by [`super::http`]
//!    from axum's `MatchedPath` rather than from the URI;
//! 2. the method is folded into the closed set [`RouteMethod`], because HTTP
//!    method names are extensible and `EVIL1 /healthz`, `EVIL2 /healthz`, …
//!    would otherwise be an unbounded label arriving through the verb;
//! 3. the map itself refuses to grow past [`MAX_TRACKED_ROUTES`], which makes
//!    the bound a property of this module rather than an argument about how the
//!    router above it happens to behave today.
//!
//! Nothing else identifying is recorded at all. There is no field here for a
//! repository, a Firebase uid, an email, a token, or an analysis id — not
//! filtered out, but absent, so there is no path by which one could arrive.
//!
//! # The ceiling, in numbers
//!
//! [`MAX_TRACKED_ROUTES`] routes × [`RouteMethod::COUNT`] methods ×
//! ([`BUCKET_COUNT`] + [`StatusClass::COUNT`] + 2) counters of eight bytes is
//! under 200 KiB, reached only if every method is exercised against every
//! route. The current router has five routes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use axum::http::{Method, StatusCode};

/// Ceiling on distinct route labels held at once.
///
/// The router has a handful of routes and `MatchedPath` cannot invent more, so
/// this is never reached in practice. It exists so that "memory is bounded" is a
/// property of this map rather than a claim about the layer above it: a route
/// mounted under a wildcard, or a middleware reordering that left `MatchedPath`
/// unavailable, would otherwise turn a routing change into an unbounded
/// allocation with nothing in this file to catch it.
pub const MAX_TRACKED_ROUTES: usize = 64;

/// Label for a request that matched no route.
///
/// One fixed string rather than the request URI. The URI is the input to this
/// process that an anonymous stranger writes in full, so recording it would be
/// recording arbitrary attacker-chosen text as a map key — the exact failure
/// this module exists to prevent, arriving through the one request that has no
/// matched pattern to fall back on.
pub const UNMATCHED_ROUTE: &str = "<unmatched>";

/// Label for requests folded together once [`MAX_TRACKED_ROUTES`] is reached.
///
/// Distinct from [`UNMATCHED_ROUTE`] because the two mean different things: one
/// says the request matched nothing, the other says the registry is full and
/// stopped distinguishing. Collapsing them would hide the second, which is the
/// one that means something is wrong here.
pub const OVERFLOW_ROUTE: &str = "<overflow>";

/// Upper bounds of every latency bucket but the last, in microseconds.
///
/// Fixed and hard-coded. A histogram that allocated a bucket per observed value
/// would be an unbounded map keyed by latency — the same failure as an
/// unbounded label, arriving through the value instead of the key.
///
/// Microseconds rather than milliseconds because the number under test is
/// expected to be small: the hypothesis is that an axum handler costs
/// single-digit milliseconds, and a histogram whose finest bucket is one
/// millisecond cannot tell "fast" from "immeasurable" — it would answer every
/// percentile with its own floor and look like a confirmation.
///
/// The top bound is ten seconds, comfortably inside the router's thirty-second
/// request budget, so a request that timed out lands in the overflow bucket
/// rather than pretending to a figure.
pub const LATENCY_BUCKET_BOUNDS_MICROS: [u64; 14] = [
    500,        // 0.5 ms
    1_000,      // 1 ms
    2_500,      // 2.5 ms
    5_000,      // 5 ms
    10_000,     // 10 ms
    25_000,     // 25 ms
    50_000,     // 50 ms
    100_000,    // 100 ms
    250_000,    // 250 ms
    500_000,    // 500 ms
    1_000_000,  // 1 s
    2_500_000,  // 2.5 s
    5_000_000,  // 5 s
    10_000_000, // 10 s
];

/// One bucket per bound, plus one for everything above the last.
pub const BUCKET_COUNT: usize = LATENCY_BUCKET_BOUNDS_MICROS.len() + 1;

/// The class of a response status, which is all that is recorded.
///
/// A class rather than the code: `404` and `410` answer the same operational
/// question, and a counter per code would be several hundred series for a
/// resolution no dashboard reads at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusClass {
    /// `1xx`.
    Informational,
    /// `2xx`.
    Success,
    /// `3xx`.
    Redirection,
    /// `4xx` — the caller's fault.
    ClientError,
    /// `5xx` — ours.
    ServerError,
    /// Anything else `http` permits.
    ///
    /// No handler in this service produces one, but [`StatusCode`] admits
    /// values up to `999`, so a total classification needs somewhere to put
    /// them. Folding them into `ServerError` would report a status we never
    /// sent as an error we never had.
    Other,
}

impl StatusClass {
    /// Every class, in index order.
    pub const ALL: [Self; 6] = [
        Self::Informational,
        Self::Success,
        Self::Redirection,
        Self::ClientError,
        Self::ServerError,
        Self::Other,
    ];

    /// How many classes exist. The width of every per-class array.
    pub const COUNT: usize = Self::ALL.len();

    /// Position in a per-class array.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Stable, low-cardinality label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Informational => "1xx",
            Self::Success => "2xx",
            Self::Redirection => "3xx",
            Self::ClientError => "4xx",
            Self::ServerError => "5xx",
            Self::Other => "other",
        }
    }

    /// Classifies a response status.
    #[must_use]
    pub fn classify(status: StatusCode) -> Self {
        match status.as_u16() / 100 {
            1 => Self::Informational,
            2 => Self::Success,
            3 => Self::Redirection,
            4 => Self::ClientError,
            5 => Self::ServerError,
            _ => Self::Other,
        }
    }
}

/// The request method, folded into a closed set.
///
/// HTTP methods are an extensible token, not an enumeration: `hyper` will parse
/// and deliver `WHATEVER /healthz` quite happily, and axum answers it with `405`
/// from the *matched* route — so it reaches this layer with a perfectly ordinary
/// route label attached. Recording the method as sent would therefore be an
/// unbounded label that a stranger can grow one request at a time, which is why
/// everything outside this set becomes [`RouteMethod::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMethod {
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
    /// Any other token a client sent.
    Other,
}

impl RouteMethod {
    /// Every method class, in index order.
    pub const ALL: [Self; 10] = [
        Self::Get,
        Self::Head,
        Self::Post,
        Self::Put,
        Self::Patch,
        Self::Delete,
        Self::Options,
        Self::Trace,
        Self::Connect,
        Self::Other,
    ];

    /// How many method classes exist. The width of every per-method array.
    pub const COUNT: usize = Self::ALL.len();

    /// Position in a per-method array.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Stable, low-cardinality label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Trace => "TRACE",
            Self::Connect => "CONNECT",
            Self::Other => "<other>",
        }
    }

    /// Folds a request method into the closed set.
    ///
    /// Matched on the string rather than against [`Method`]'s associated
    /// constants, which are not usable as patterns. The `_` arm is what closes
    /// the set, and it is the arm that carries the whole cardinality guarantee.
    #[must_use]
    pub fn classify(method: &Method) -> Self {
        match method.as_str() {
            "GET" => Self::Get,
            "HEAD" => Self::Head,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "PATCH" => Self::Patch,
            "DELETE" => Self::Delete,
            "OPTIONS" => Self::Options,
            "TRACE" => Self::Trace,
            "CONNECT" => Self::Connect,
            _ => Self::Other,
        }
    }
}

/// A percentile read off a bucketed histogram, carrying the resolution it was
/// read at.
///
/// [`Self::micros`] is a linear interpolation inside one bucket, which assumes
/// the observations in that bucket are spread evenly across it. They are not,
/// and nothing here knows how they are actually spread. The bounds are what the
/// histogram genuinely knows — the answer is somewhere between them — and they
/// travel with the estimate rather than being recoverable only by consulting
/// the bucket table, so a reader cannot take the interpolation for a
/// measurement without first stepping over the thing that says it is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyEstimate {
    /// Interpolated estimate, in microseconds.
    pub micros: u64,
    /// Lower bound of the bucket the percentile fell in.
    pub lower_bound_micros: u64,
    /// Upper bound of that bucket.
    ///
    /// Absent for the overflow bucket, where the histogram recorded that an
    /// observation was slower than the last bound and nothing more. Any figure
    /// here would be invented, and [`Self::micros`] is then the last bound
    /// itself — a floor, not an estimate.
    pub upper_bound_micros: Option<u64>,
}

/// One consistent read of a histogram.
///
/// Percentiles are computed from a snapshot rather than from live counters so
/// that p50, p95 and p99 describe the same set of observations. Reading the
/// atomics once per percentile would let a p50 taken after a burst exceed a p99
/// taken before it — a shape no reader would think to distrust, because
/// percentiles are not supposed to be able to do that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistogramSnapshot {
    buckets: [u64; BUCKET_COUNT],
    count: u64,
    sum_micros: u64,
}

impl HistogramSnapshot {
    /// How many observations were recorded.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Total observed latency, in microseconds.
    #[must_use]
    pub const fn sum_micros(&self) -> u64 {
        self.sum_micros
    }

    /// Observations per bucket, in bound order, with the overflow bucket last.
    #[must_use]
    pub const fn buckets(&self) -> &[u64; BUCKET_COUNT] {
        &self.buckets
    }

    /// The `percentile`th latency, estimated by interpolation.
    ///
    /// `percentile` is an integer in `0..=100`, and integers are the point: the
    /// rank being selected is computed exactly, so the only approximation in the
    /// answer is the bucketing itself, which [`LatencyEstimate`] then states.
    /// Values above 100 are clamped.
    ///
    /// Returns `None` for an empty histogram. A zero would be indistinguishable
    /// from a genuinely instant response, and "nothing has been measured" is not
    /// a measurement.
    #[must_use]
    pub fn percentile(&self, percentile: u32) -> Option<LatencyEstimate> {
        if self.count == 0 {
            return None;
        }

        // Nearest rank: the observation selected is the one at 1-based rank
        // `ceil(percentile × count / 100)`, floored at 1.
        //
        // The formula is written down because the intuition it contradicts is a
        // common one, and only the arithmetic settles it: of a hundred
        // observations p99 is the ninety-ninth, so one observation is slower
        // than the figure reported — the slowest is reached at p100 and nowhere
        // earlier. The floor is what makes p0 the fastest observation rather
        // than rank zero, which no observation occupies.
        let rank = (u64::from(percentile.min(100)) * self.count)
            .div_ceil(100)
            .max(1);

        let mut below = 0u64;
        for (index, &in_bucket) in self.buckets.iter().enumerate() {
            if in_bucket == 0 {
                continue;
            }
            if below + in_bucket >= rank {
                return Some(Self::interpolate(index, rank - below, in_bucket));
            }
            below += in_bucket;
        }

        // Unreachable: `rank` is at most `count`, which is the sum of the
        // buckets. Answering `None` rather than panicking, because a metric that
        // takes a request down over its own arithmetic is worse than a metric
        // that declines to answer.
        None
    }

    /// Places `position` of `in_bucket` observations inside bucket `index`.
    fn interpolate(index: usize, position: u64, in_bucket: u64) -> LatencyEstimate {
        let lower = match index.checked_sub(1) {
            Some(previous) => LATENCY_BUCKET_BOUNDS_MICROS[previous],
            None => 0,
        };
        let Some(&upper) = LATENCY_BUCKET_BOUNDS_MICROS.get(index) else {
            // The overflow bucket. The last bound is a floor and is reported as
            // one; there is no upper bound to interpolate towards.
            return LatencyEstimate {
                micros: lower,
                lower_bound_micros: lower,
                upper_bound_micros: None,
            };
        };

        let span = upper - lower;
        LatencyEstimate {
            micros: lower + span.saturating_mul(position) / in_bucket,
            lower_bound_micros: lower,
            upper_bound_micros: Some(upper),
        }
    }
}

/// A fixed-bucket latency histogram.
#[derive(Debug)]
struct Histogram {
    buckets: [AtomicU64; BUCKET_COUNT],
    count: AtomicU64,
    sum_micros: AtomicU64,
}

impl Histogram {
    const fn new() -> Self {
        Self {
            buckets: [const { AtomicU64::new(0) }; BUCKET_COUNT],
            count: AtomicU64::new(0),
            sum_micros: AtomicU64::new(0),
        }
    }

    fn record(&self, latency: Duration) {
        // A latency that does not fit a `u64` of microseconds is 584,000 years
        // long, so saturating is a formality rather than a policy.
        let micros = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        let index = LATENCY_BUCKET_BOUNDS_MICROS
            .iter()
            .position(|bound| micros <= *bound)
            .unwrap_or(BUCKET_COUNT - 1);

        // Relaxed throughout: these counters order nothing, and a reader that
        // saw a bucket increment before the total would still be reading a
        // number that was true a moment ago. Paying for stronger ordering on a
        // path every request takes would buy an accuracy nobody can observe.
        self.buckets[index].fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_micros.fetch_add(micros, Ordering::Relaxed);
    }

    fn snapshot(&self) -> HistogramSnapshot {
        let mut buckets = [0u64; BUCKET_COUNT];
        for (slot, counter) in buckets.iter_mut().zip(self.buckets.iter()) {
            *slot = counter.load(Ordering::Relaxed);
        }
        HistogramSnapshot {
            buckets,
            count: self.count.load(Ordering::Relaxed),
            sum_micros: self.sum_micros.load(Ordering::Relaxed),
        }
    }
}

/// Counters for one route and one method class.
#[derive(Debug)]
struct MethodMetrics {
    by_status_class: [AtomicU64; StatusClass::COUNT],
    latency: Histogram,
}

impl MethodMetrics {
    const fn new() -> Self {
        Self {
            by_status_class: [const { AtomicU64::new(0) }; StatusClass::COUNT],
            latency: Histogram::new(),
        }
    }
}

/// Counters for one route label, one slot per method class.
///
/// A fixed array rather than a nested map, so a route seen for the first time
/// allocates exactly once and every request after it touches only atomics. It
/// is also what makes the memory ceiling arithmetic rather than empirical:
/// every tracked route costs the same, whether one method is used or ten.
#[derive(Debug)]
struct RouteMetrics {
    by_method: [MethodMetrics; RouteMethod::COUNT],
}

impl RouteMetrics {
    const fn new() -> Self {
        Self {
            by_method: [const { MethodMetrics::new() }; RouteMethod::COUNT],
        }
    }

    fn record(&self, method: RouteMethod, status: StatusCode, latency: Duration) {
        let slot = &self.by_method[method.index()];
        slot.by_status_class[StatusClass::classify(status).index()].fetch_add(1, Ordering::Relaxed);
        slot.latency.record(latency);
    }
}

/// Shared state behind every [`Metrics`] handle.
#[derive(Debug)]
struct Inner {
    /// Keyed by route label. Never grows past [`MAX_TRACKED_ROUTES`].
    ///
    /// An `RwLock` rather than a `Mutex` because the write path is taken once
    /// per route for the life of the process — after the first request to each,
    /// every recording is a read lock and some atomic adds.
    routes: RwLock<HashMap<Box<str>, RouteMetrics>>,
    /// Where requests go once the map is full.
    overflow: RouteMetrics,
    in_flight: AtomicU64,
}

/// The registry. Cloning shares it.
///
/// One per process, built at the composition root and reachable from
/// [`crate::state::AppState`] so the layer that records into it and the handler
/// that will eventually read it cannot end up looking at two different
/// registries.
#[derive(Debug, Clone)]
pub struct Metrics {
    inner: Arc<Inner>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                routes: RwLock::new(HashMap::new()),
                overflow: RouteMetrics::new(),
                in_flight: AtomicU64::new(0),
            }),
        }
    }

    /// Records one completed request.
    ///
    /// `route` must already be a normalised label — a matched pattern,
    /// [`UNMATCHED_ROUTE`], or a fixed string. Nothing here inspects it, so
    /// passing a concrete URI would defeat every guarantee this module makes;
    /// [`super::http::record`] is the only caller that ships, and it is where
    /// that normalisation is enforced.
    pub fn record_request(
        &self,
        method: RouteMethod,
        route: &str,
        status: StatusCode,
        latency: Duration,
    ) {
        // The common path: the route has been seen, so a read lock and a few
        // relaxed adds are the whole cost.
        //
        // The block is deliberate rather than stylistic. `RwLock` is not
        // reentrant, so the read guard has to be gone before the write below is
        // attempted; leaving that to temporary-scope rules would make a
        // deadlock on every request the consequence of an edition detail.
        {
            let routes = self.routes();
            if let Some(metrics) = routes.get(route) {
                metrics.record(method, status, latency);
                return;
            }
        }

        let mut routes = self
            .inner
            .routes
            .write()
            .unwrap_or_else(PoisonError::into_inner);

        // Re-checked under the write lock: another thread may have inserted this
        // route between the read above and this line, and `len()` has to be
        // compared against the map as it is now rather than as it was.
        if !routes.contains_key(route) && routes.len() >= MAX_TRACKED_ROUTES {
            self.inner.overflow.record(method, status, latency);
            return;
        }

        routes
            .entry(route.into())
            .or_insert_with(RouteMetrics::new)
            .record(method, status, latency);
    }

    /// Raises the in-flight count until the returned guard is dropped.
    #[must_use]
    pub fn enter_request(&self) -> InFlightGuard {
        self.inner.in_flight.fetch_add(1, Ordering::Relaxed);
        InFlightGuard {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Requests currently being served by this process.
    #[must_use]
    pub fn in_flight(&self) -> u64 {
        self.inner.in_flight.load(Ordering::Relaxed)
    }

    /// How many distinct route labels are held.
    ///
    /// The number [`MAX_TRACKED_ROUTES`] bounds, exposed so boundedness can be
    /// asserted directly rather than inferred from a snapshot that only lists
    /// what was exercised.
    #[must_use]
    pub fn tracked_routes(&self) -> usize {
        self.routes().len()
    }

    /// Reads every counter that has been touched.
    ///
    /// Route and method combinations with no requests are omitted, so the
    /// labels in the result are exactly the labels this process has recorded —
    /// which is what makes "no identifier ever became a label" a property a test
    /// can assert rather than a comment it has to trust.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        let routes = self.routes();
        let mut samples: Vec<RouteSample> = routes
            .iter()
            .map(|(label, metrics)| (label.as_ref(), metrics))
            .chain(std::iter::once((OVERFLOW_ROUTE, &self.inner.overflow)))
            .flat_map(|(label, metrics)| sample_route(label, metrics))
            .collect();

        // A HashMap iterates in an order that changes between runs; a stable one
        // keeps a rendered table from reshuffling itself under a reader.
        samples.sort_by(|left, right| {
            left.route
                .cmp(&right.route)
                .then(left.method.index().cmp(&right.method.index()))
        });

        // Read from the same guard as the samples above, not through
        // `tracked_routes()`. Two separate reads could disagree — a route
        // recorded between them would be counted and not listed — and a
        // snapshot whose own fields contradict each other is the shape a reader
        // has no reason to distrust.
        let tracked_routes = routes.len();

        MetricsSnapshot {
            in_flight: self.in_flight(),
            tracked_routes,
            routes: samples,
        }
    }

    /// Read access to the route map, surviving a poisoned lock.
    ///
    /// Nothing is called while this lock is held, so a panic cannot leave the
    /// map half-written and the poison flag carries no information here. Failing
    /// closed would silence the metrics from the first panic onward — removing
    /// the numbers at exactly the moment somebody wants them.
    fn routes(&self) -> std::sync::RwLockReadGuard<'_, HashMap<Box<str>, RouteMetrics>> {
        self.inner
            .routes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Builds one sample per method class that has seen a request.
fn sample_route<'a>(
    label: &'a str,
    metrics: &'a RouteMetrics,
) -> impl Iterator<Item = RouteSample> + 'a {
    RouteMethod::ALL.into_iter().filter_map(move |method| {
        let slot = &metrics.by_method[method.index()];
        let latency = slot.latency.snapshot();
        if latency.count() == 0 {
            return None;
        }

        let mut by_status_class = [0u64; StatusClass::COUNT];
        for (target, counter) in by_status_class.iter_mut().zip(slot.by_status_class.iter()) {
            *target = counter.load(Ordering::Relaxed);
        }

        Some(RouteSample {
            route: label.to_owned(),
            method,
            count: latency.count(),
            by_status_class,
            latency,
        })
    })
}

/// Holds the in-flight count up for as long as it lives.
///
/// A guard rather than a matched pair of increments, because the decrement has
/// to happen on every exit — including the unwind out of a panicking handler,
/// where the line after the `await` never runs. A gauge that leaks on panic
/// climbs forever and is worse than no gauge at all, because it reads as
/// saturation.
#[derive(Debug)]
pub struct InFlightGuard {
    inner: Arc<Inner>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.inner.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Everything the registry holds, read at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsSnapshot {
    /// Requests being served when the snapshot was taken.
    pub in_flight: u64,
    /// Distinct route labels held, which is the number [`MAX_TRACKED_ROUTES`]
    /// bounds.
    ///
    /// Not derivable from `routes` below: that field lists the `<overflow>`
    /// series alongside real labels once the registry is full, so counting it
    /// would report the ceiling plus one and hide the fact that the map stopped
    /// growing exactly where it promised to.
    pub tracked_routes: usize,
    /// One entry per route and method class that has seen a request, sorted.
    pub routes: Vec<RouteSample>,
}

impl MetricsSnapshot {
    /// Every distinct route label present, in sorted order and without repeats.
    ///
    /// The label set, which is the thing that must stay bounded and free of
    /// identifiers.
    #[must_use]
    pub fn route_labels(&self) -> Vec<&str> {
        let mut labels: Vec<&str> = self
            .routes
            .iter()
            .map(|route| route.route.as_str())
            .collect();
        labels.dedup();
        labels
    }
}

/// Counters for one route label and one method class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSample {
    /// The normalised route label.
    pub route: String,
    /// The method class.
    pub method: RouteMethod,
    /// Requests recorded.
    pub count: u64,
    /// Responses by class, indexed by [`StatusClass::index`].
    pub by_status_class: [u64; StatusClass::COUNT],
    /// Latency distribution.
    pub latency: HistogramSnapshot,
}

impl RouteSample {
    /// Responses in one class.
    #[must_use]
    pub const fn in_status_class(&self, class: StatusClass) -> u64 {
        self.by_status_class[class.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BUCKET_COUNT, Histogram, LATENCY_BUCKET_BOUNDS_MICROS, MAX_TRACKED_ROUTES, Metrics,
        OVERFLOW_ROUTE, RouteMethod, StatusClass,
    };
    use axum::http::{Method, StatusCode};
    use std::time::Duration;

    fn record(metrics: &Metrics, route: &str, micros: u64) {
        metrics.record_request(
            RouteMethod::Get,
            route,
            StatusCode::OK,
            Duration::from_micros(micros),
        );
    }

    #[test]
    fn an_untouched_registry_reports_no_series() {
        let metrics = Metrics::new();
        let snapshot = metrics.snapshot();
        assert!(
            snapshot.routes.is_empty(),
            "nothing recorded means no series at all, not a series full of zeroes"
        );
        assert_eq!(snapshot.tracked_routes, 0);
        assert_eq!(metrics.tracked_routes(), 0);
    }

    #[test]
    fn an_empty_histogram_has_no_percentile() {
        // Not reachable through `snapshot`, which omits empty series — so it is
        // asserted here instead. A zero would be indistinguishable from a
        // genuinely instant response, and inventing one is how a dashboard ends
        // up reporting a p99 for an endpoint nobody has called.
        let empty = Histogram::new().snapshot();
        assert_eq!(empty.count(), 0);
        assert_eq!(empty.percentile(50), None);
        assert_eq!(empty.percentile(99), None);
    }

    #[test]
    fn a_percentile_lands_in_the_bucket_holding_that_rank() {
        let metrics = Metrics::new();
        // Ninety-nine fast requests and one slow one, arranged so the rank
        // arithmetic is the only thing that can produce the answers below. At
        // rank 99 of 100, p99 is still one of the fast observations; the single
        // slow one is reached at p100 and not before. Expecting p99 to follow
        // the tail is the misreading this case exists to fail on.
        for _ in 0..99 {
            record(&metrics, "/fast", 400);
        }
        record(&metrics, "/fast", 3_000_000);

        let snapshot = metrics.snapshot();
        let sample = &snapshot.routes[0];
        let p50 = sample.latency.percentile(50).expect("100 observations");
        let p99 = sample.latency.percentile(99).expect("100 observations");

        assert_eq!(p50.lower_bound_micros, 0);
        assert_eq!(p50.upper_bound_micros, Some(500));
        assert_eq!(p99.lower_bound_micros, 0);
        assert_eq!(p99.upper_bound_micros, Some(500));

        let p100 = sample.latency.percentile(100).expect("100 observations");
        assert_eq!(
            p100.lower_bound_micros, 2_500_000,
            "the slowest observation is the one at 3 s"
        );
        assert_eq!(p100.upper_bound_micros, Some(5_000_000));
    }

    #[test]
    fn interpolation_stays_inside_the_bucket_it_reports() {
        let metrics = Metrics::new();
        // Four observations spread across one bucket, 1 ms to 2.5 ms.
        for micros in [1_100, 1_600, 2_000, 2_400] {
            record(&metrics, "/spread", micros);
        }

        let snapshot = metrics.snapshot();
        let latency = &snapshot.routes[0].latency;
        for percentile in [0, 25, 50, 75, 99, 100] {
            let estimate = latency.percentile(percentile).expect("four observations");
            assert_eq!(estimate.lower_bound_micros, 1_000);
            assert_eq!(estimate.upper_bound_micros, Some(2_500));
            assert!(
                (estimate.lower_bound_micros..=2_500).contains(&estimate.micros),
                "p{percentile} estimated {} outside its own bucket",
                estimate.micros
            );
        }
    }

    #[test]
    fn percentiles_do_not_decrease_as_the_percentile_rises() {
        let metrics = Metrics::new();
        for micros in [100, 900, 4_000, 40_000, 400_000, 4_000_000, 40_000_000] {
            record(&metrics, "/spread", micros);
        }

        let snapshot = metrics.snapshot();
        let latency = &snapshot.routes[0].latency;
        let mut previous = 0;
        for percentile in 0..=100 {
            let estimate = latency.percentile(percentile).expect("seven observations");
            assert!(
                estimate.micros >= previous,
                "p{percentile} went backwards: {} after {previous}",
                estimate.micros
            );
            previous = estimate.micros;
        }
    }

    #[test]
    fn the_overflow_bucket_reports_a_floor_rather_than_an_estimate() {
        let metrics = Metrics::new();
        record(&metrics, "/slow", 60_000_000);

        let snapshot = metrics.snapshot();
        let estimate = snapshot.routes[0]
            .latency
            .percentile(99)
            .expect("one observation");

        let last_bound = LATENCY_BUCKET_BOUNDS_MICROS[LATENCY_BUCKET_BOUNDS_MICROS.len() - 1];
        assert_eq!(estimate.lower_bound_micros, last_bound);
        assert_eq!(
            estimate.upper_bound_micros, None,
            "beyond the last bound the histogram knows a floor and nothing else; \
             an upper figure would be invented"
        );
        assert_eq!(estimate.micros, last_bound);
    }

    #[test]
    fn every_observation_lands_in_exactly_one_bucket() {
        let metrics = Metrics::new();
        // One observation on each boundary, one below the first, one past the
        // last. A boundary belongs to the bucket it bounds.
        let mut expected = 0;
        record(&metrics, "/bounds", 0);
        expected += 1;
        for bound in LATENCY_BUCKET_BOUNDS_MICROS {
            record(&metrics, "/bounds", bound);
            expected += 1;
        }
        record(&metrics, "/bounds", u64::MAX);
        expected += 1;

        let snapshot = metrics.snapshot();
        let latency = &snapshot.routes[0].latency;
        assert_eq!(latency.count(), expected);
        assert_eq!(
            latency.buckets().iter().sum::<u64>(),
            expected,
            "an observation that fell through the buckets would make every \
             percentile read from a distribution missing it"
        );
        assert_eq!(latency.buckets().len(), BUCKET_COUNT);
        assert_eq!(
            latency.buckets()[0],
            2,
            "0 and the first bound both belong to the first bucket"
        );
        assert_eq!(latency.buckets()[BUCKET_COUNT - 1], 1);
    }

    #[test]
    fn the_route_map_stops_growing_at_the_ceiling() {
        let metrics = Metrics::new();
        for index in 0..(MAX_TRACKED_ROUTES * 20) {
            record(&metrics, &format!("/route-{index}"), 1_000);
        }

        assert_eq!(
            metrics.tracked_routes(),
            MAX_TRACKED_ROUTES,
            "the map is what holds the memory; if it grows with distinct labels \
             the process has an unbounded allocation keyed by whatever a caller sends"
        );

        let snapshot = metrics.snapshot();
        assert!(
            snapshot.route_labels().contains(&OVERFLOW_ROUTE),
            "requests past the ceiling are still counted, under a label that says \
             the registry stopped distinguishing"
        );
        assert_eq!(
            snapshot.tracked_routes, MAX_TRACKED_ROUTES,
            "the snapshot must report the map's size, not the number of series it \
             lists — those differ by the overflow row exactly when the ceiling is \
             reached, which is the moment the figure matters"
        );
        assert!(
            snapshot.route_labels().len() > snapshot.tracked_routes,
            "the overflow label is a series without being a tracked route; if these \
             were equal the assertion above would be proving nothing"
        );

        let total: u64 = snapshot.routes.iter().map(|route| route.count).sum();
        assert_eq!(
            total,
            u64::try_from(MAX_TRACKED_ROUTES * 20).expect("fits"),
            "folding a request into the overflow series must not lose it"
        );
    }

    #[test]
    fn an_unusual_method_cannot_grow_the_label_set() {
        // HTTP methods are an extensible token. Recording one as sent would be an
        // unbounded label arriving through the verb rather than the path.
        for index in 0..500 {
            let method =
                Method::from_bytes(format!("EVIL{index}").as_bytes()).expect("a valid token");
            assert_eq!(RouteMethod::classify(&method), RouteMethod::Other);
        }

        let metrics = Metrics::new();
        for index in 0..500 {
            let method =
                Method::from_bytes(format!("EVIL{index}").as_bytes()).expect("a valid token");
            metrics.record_request(
                RouteMethod::classify(&method),
                "/healthz",
                StatusCode::METHOD_NOT_ALLOWED,
                Duration::from_micros(50),
            );
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.routes.len(), 1, "one route, one method class");
        assert_eq!(snapshot.routes[0].method, RouteMethod::Other);
        assert_eq!(snapshot.routes[0].count, 500);
    }

    #[test]
    fn status_classes_are_counted_separately() {
        let metrics = Metrics::new();
        for status in [
            StatusCode::OK,
            StatusCode::CREATED,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            metrics.record_request(
                RouteMethod::Get,
                "/mixed",
                status,
                Duration::from_micros(100),
            );
        }

        let snapshot = metrics.snapshot();
        let sample = &snapshot.routes[0];
        assert_eq!(sample.in_status_class(StatusClass::Success), 2);
        assert_eq!(sample.in_status_class(StatusClass::ClientError), 1);
        assert_eq!(sample.in_status_class(StatusClass::ServerError), 1);
        assert_eq!(sample.in_status_class(StatusClass::Redirection), 0);
        assert_eq!(sample.count, 4);
    }

    #[test]
    fn the_in_flight_gauge_returns_to_zero() {
        let metrics = Metrics::new();
        assert_eq!(metrics.in_flight(), 0);
        {
            let _first = metrics.enter_request();
            let _second = metrics.enter_request();
            assert_eq!(metrics.in_flight(), 2);
        }
        assert_eq!(
            metrics.in_flight(),
            0,
            "a gauge that leaks reads as saturation forever"
        );
    }

    #[test]
    fn every_status_class_and_method_has_a_distinct_index() {
        // The arrays are indexed by these, so two variants sharing an index
        // would silently merge two counters.
        for (position, class) in StatusClass::ALL.into_iter().enumerate() {
            assert_eq!(class.index(), position);
        }
        for (position, method) in RouteMethod::ALL.into_iter().enumerate() {
            assert_eq!(method.index(), position);
        }
    }
}
