//! What may become a metric label, asserted against the registry itself.
//!
//! Every test here drives the **real** router and then reads the label set the
//! process actually holds. That is the difference that matters: a test that
//! inspected the middleware, or trusted a comment saying `MatchedPath` is used,
//! would keep passing through the one change that breaks this — someone
//! reaching for `request.uri()` because a matched path was inconvenient.
//!
//! Cardinality is a correctness rule here, not a style preference. Every
//! identifier that reaches a label is a permanent series in a process that never
//! restarts, so an unbounded label set is an unbounded allocation an anonymous
//! caller controls. It is also a disclosure: labels are what a dashboard renders,
//! and a repository name or a token that reached one has been published to
//! everyone who can see the dashboard.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use repolens_github::{GitHubClientConfig, GitHubRestClient};
use repolens_server::api;
use repolens_server::state::AppState;
use repolens_server::telemetry::metrics::{
    MAX_TRACKED_ROUTES, Metrics, RouteMethod, StatusClass, UNMATCHED_ROUTE,
};
use tower::ServiceExt as _;

/// A client with no token. Nothing here reaches the network.
fn unauthenticated_github() -> GitHubRestClient {
    GitHubRestClient::new(GitHubClientConfig::new())
        .expect("a client with the default API base and no token is always constructible")
}

/// The real router, plus the registry it records into.
fn app() -> (axum::Router, Metrics) {
    let state = AppState::without_database(unauthenticated_github());
    let metrics = state.metrics().clone();
    let (router, _openapi) = api::build(state, None);
    (router, metrics)
}

/// Drives one request through the router, returning its status.
async fn send(router: &axum::Router, request: Request<Body>) -> StatusCode {
    router
        .clone()
        .oneshot(request)
        .await
        .expect("router responds")
        .status()
}

/// A `GET` with no headers or body.
fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request builds")
}

/// An analysis id, in the form a real one takes in a URL.
const ANALYSIS_ID: &str = "019fdb48-6c0c-7c3e-9a2f-1d5b8e6a4c11";

#[tokio::test]
async fn a_concrete_path_never_becomes_a_label() {
    let (router, metrics) = app();

    for uri in [
        &format!("/api/v1/analyses/{ANALYSIS_ID}"),
        &format!("/api/v1/analyses/{ANALYSIS_ID}/report"),
    ] {
        send(&router, get(uri)).await;
    }

    let snapshot = metrics.snapshot();
    let labels = snapshot.route_labels();

    assert_eq!(
        labels,
        vec![
            "/api/v1/analyses/{analysis_id}",
            "/api/v1/analyses/{analysis_id}/report",
        ],
        "labels must be the matched pattern; a concrete path would create one \
         permanent series per analysis"
    );
    for label in &labels {
        assert!(
            !label.contains(ANALYSIS_ID),
            "the analysis id reached a label: {label}"
        );
    }
}

#[tokio::test]
async fn nothing_identifying_can_reach_a_label() {
    // Every place a caller can write into a request, carrying the four kinds of
    // value that must never be published: a repository, a Firebase uid, an
    // email, and a token. The path is the one that matters most, because it is
    // the only one a label is ever built from.
    const REPOSITORY: &str = "rust-lang-crates-io";
    const FIREBASE_UID: &str = "kR3xQ9pLmN0aBcDeFgHiJkLmNoP2";
    const EMAIL: &str = "someone@example.invalid";
    const TOKEN: &str = "ghp-EXAMPLENOTAREALTOKEN0123456789";

    let (router, metrics) = app();

    // A matched route, with the identifiers in the query string and headers.
    send(
        &router,
        Request::builder()
            .uri(format!(
                "/api/v1/analyses/{ANALYSIS_ID}?repository={REPOSITORY}&email={EMAIL}"
            ))
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("x-firebase-uid", FIREBASE_UID)
            .body(Body::empty())
            .expect("request builds"),
    )
    .await;

    // An unmatched route, with the identifiers in the path itself.
    for uri in [
        &format!("/{REPOSITORY}"),
        &format!("/users/{FIREBASE_UID}/settings"),
        &format!("/mail/{EMAIL}"),
        &format!("/{TOKEN}"),
    ] {
        send(&router, get(uri)).await;
    }

    // A body carrying a repository URL, through the one route that takes one.
    send(
        &router,
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/analyses")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(Body::from(format!(
                r#"{{"repository_url":"https://github.com/{REPOSITORY}/x"}}"#
            )))
            .expect("request builds"),
    )
    .await;

    let snapshot = metrics.snapshot();
    let labels = snapshot.route_labels();
    assert!(
        !labels.is_empty(),
        "nothing was recorded — the test is inert"
    );

    for label in &labels {
        for secret in [REPOSITORY, FIREBASE_UID, EMAIL, TOKEN, ANALYSIS_ID] {
            assert!(
                !label.contains(secret),
                "{secret:?} reached the label {label:?}"
            );
        }
        assert!(
            !label.contains('?') && !label.contains('@'),
            "a query string or an address in a label means the URI was recorded \
             rather than the matched pattern: {label:?}"
        );
    }

    // Positive half: the labels that *are* present are the patterns and the one
    // fixed unmatched label, and nothing else. Asserting only the absence of
    // known strings would pass just as well if labelling had stopped entirely.
    let mut expected = vec![
        "/api/v1/analyses",
        "/api/v1/analyses/{analysis_id}",
        UNMATCHED_ROUTE,
    ];
    expected.sort_unstable();
    assert_eq!(labels, expected);
}

#[tokio::test]
async fn many_distinct_unmatched_paths_cost_one_series() {
    // The 404 path is where an unbounded label set is cheapest to create: no
    // credential, no matched route, and a caller writes the whole string. A
    // thousand of them must not be a thousand series.
    let (router, metrics) = app();

    for index in 0..1_000 {
        send(&router, get(&format!("/does-not-exist/{index}"))).await;
    }

    assert_eq!(
        metrics.tracked_routes(),
        1,
        "one series for every unmatched path there will ever be; the map is what \
         holds the memory, so this is the assertion that bounds it"
    );

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.route_labels(), vec![UNMATCHED_ROUTE]);
    assert_eq!(snapshot.routes[0].count, 1_000, "and every one is counted");
    assert_eq!(
        snapshot.routes[0].in_status_class(StatusClass::ClientError),
        1_000
    );
}

#[tokio::test]
async fn the_router_cannot_fill_the_registry() {
    // The ceiling exists for a routing change nobody has made yet. This asserts
    // the shipped router stays far below it, so an overflow series appearing in
    // production is a signal rather than a normal condition.
    let (router, metrics) = app();

    for request in [
        get("/healthz"),
        get("/api/v1/system/probe"),
        get(&format!("/api/v1/analyses/{ANALYSIS_ID}")),
        get(&format!("/api/v1/analyses/{ANALYSIS_ID}/report")),
        get("/nowhere"),
    ] {
        send(&router, request).await;
    }

    let tracked = metrics.tracked_routes();
    assert!(
        tracked < MAX_TRACKED_ROUTES,
        "the router uses {tracked} of {MAX_TRACKED_ROUTES} labels; at the ceiling \
         the registry stops distinguishing routes and the table becomes useless"
    );
}

#[tokio::test]
async fn an_unknown_method_against_a_real_route_does_not_grow_the_label_set() {
    // `hyper` will deliver any valid token as a method, and axum answers it from
    // the *matched* route — so it arrives at the recording layer with an
    // ordinary route label attached and only the verb is caller-controlled.
    let (router, metrics) = app();

    for index in 0..200 {
        let method = Method::from_bytes(format!("EVIL{index}").as_bytes()).expect("a valid token");
        send(
            &router,
            Request::builder()
                .method(method)
                .uri("/healthz")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await;
    }

    let snapshot = metrics.snapshot();
    assert_eq!(
        snapshot.routes.len(),
        1,
        "two hundred verbs, one series: {:?}",
        snapshot.routes.iter().map(|r| r.method).collect::<Vec<_>>()
    );
    assert_eq!(snapshot.routes[0].route, "/healthz");
    assert_eq!(snapshot.routes[0].count, 200);
    assert_eq!(
        snapshot.routes[0].method,
        RouteMethod::Other,
        "the verb is folded into the closed set rather than passed through; \
         collapsing it onto a real method would keep the series count at one \
         while reporting two hundred rejected requests as GETs"
    );
}

#[tokio::test]
async fn a_request_is_counted_with_its_status_class_and_a_latency() {
    let (router, metrics) = app();

    assert_eq!(send(&router, get("/healthz")).await, StatusCode::OK);
    assert_eq!(send(&router, get("/nowhere")).await, StatusCode::NOT_FOUND);

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.in_flight, 0, "nothing is in flight afterwards");

    let healthz = snapshot
        .routes
        .iter()
        .find(|route| route.route == "/healthz")
        .expect("the liveness route was exercised");
    assert_eq!(healthz.count, 1);
    assert_eq!(healthz.in_status_class(StatusClass::Success), 1);
    assert_eq!(healthz.in_status_class(StatusClass::ClientError), 0);

    let percentile = healthz
        .latency
        .percentile(50)
        .expect("one observation is enough for a percentile");
    assert!(
        percentile.upper_bound_micros.is_some(),
        "a liveness response is nowhere near the top bucket; landing in the \
         overflow bucket would mean the histogram measured something else"
    );
    assert!(
        percentile.micros <= percentile.upper_bound_micros.expect("checked above"),
        "an estimate must sit inside the bucket it was read from"
    );

    let unmatched = snapshot
        .routes
        .iter()
        .find(|route| route.route == UNMATCHED_ROUTE)
        .expect("the 404 was recorded");
    assert_eq!(unmatched.in_status_class(StatusClass::ClientError), 1);
}
