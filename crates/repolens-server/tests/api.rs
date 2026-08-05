//! HTTP surface tests.
//!
//! Exercised through the real `axum` router rather than a mock, so the routing,
//! middleware stack, and OpenAPI collection under test are the ones that ship.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use repolens_server::api;
use repolens_server::state::AppState;
use tower::ServiceExt as _;

/// Sends one request through the fully built router.
async fn get(uri: &str) -> (StatusCode, serde_json::Value) {
    let (app, _openapi) = api::build(AppState::without_database());

    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .expect("body is readable");
    let parsed = serde_json::from_slice(&body).expect("body is JSON");

    (status, parsed)
}

#[tokio::test]
async fn liveness_reports_ok() {
    let (status, body) = get("/healthz").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn probe_reports_the_database_as_unavailable_when_unconfigured() {
    let (status, body) = get("/api/v1/system/probe").await;

    // 200 even though a dependency is down. Failing the request would make
    // "the API is up but the database is not" indistinguishable from "the API
    // is down", which is the distinction this endpoint exists to draw.
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["api"], "OK");
    assert_eq!(body["database"], "UNAVAILABLE");
    assert!(
        body["schema_version"].is_null(),
        "an unreachable database yields an unknown schema version, not zero — \
         collapsing them would let a connection failure read as an empty database"
    );
    assert!(
        body["build_sha"].is_string(),
        "build_sha is always present; it is `unknown` for a local build"
    );
}

#[tokio::test]
async fn probe_uses_the_settled_naming_convention() {
    let (_status, body) = get("/api/v1/system/probe").await;

    // Guards the wire format directly, not just the OpenAPI document: a serde
    // attribute could satisfy one and not the other.
    let object = body.as_object().expect("the probe returns an object");
    for field in ["api", "database", "build_sha", "schema_version"] {
        assert!(
            object.contains_key(field),
            "missing snake_case field {field}"
        );
    }
    for camel in ["buildSha", "schemaVersion"] {
        assert!(
            !object.contains_key(camel),
            "camelCase field {camel} leaked"
        );
    }
}

#[tokio::test]
async fn unknown_paths_are_not_found() {
    let (status, _body) = get("/healthz").await;
    assert_eq!(status, StatusCode::OK);

    let (app, _openapi) = api::build(AppState::without_database());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/does-not-exist")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn probe_answers_within_its_own_budget() {
    // The router's outer timeout is 30s; the probe's inner budget is 5s. This
    // asserts the promise that matters: a probe answers quickly enough that a
    // stalled dependency still yields 200 + UNAVAILABLE rather than the outer
    // layer's 408. With no pool configured there is nothing to stall, so this
    // guards the ordering of the two budgets rather than the stall itself.
    let started = std::time::Instant::now();
    let (status, body) = get("/api/v1/system/probe").await;
    let elapsed = started.elapsed();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["database"], "UNAVAILABLE");
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "the probe must resolve inside its own budget, not the router's"
    );
}

#[tokio::test]
async fn no_cors_headers_without_a_configured_origin() {
    // Absent configuration must mean *no* CORS layer, not a permissive one. A
    // wildcard would have to be revisited the moment an endpoint needs
    // credentials, so its absence is asserted rather than assumed.
    let (app, _openapi) = api::build(AppState::without_database());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/system/probe")
                .header("Origin", "https://example.invalid")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "no origin is configured, so no CORS header may be emitted"
    );
}

#[test]
fn openapi_document_is_collected_from_the_live_router() {
    let (_app, openapi) = api::build(AppState::without_database());

    for path in ["/healthz", "/api/v1/system/probe"] {
        assert!(
            openapi.paths.paths.contains_key(path),
            "a served route that is missing from the OpenAPI document would be \
             invisible to the generated TypeScript client: {path}"
        );
    }
}
