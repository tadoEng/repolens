//! HTTP surface tests.
//!
//! Exercised through the real `axum` router rather than a mock, so the routing,
//! middleware stack, and OpenAPI collection under test are the ones that ship.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use repolens_github::{GitHubClientConfig, GitHubRestClient};
use repolens_server::api;
use repolens_server::state::AppState;
use tower::ServiceExt as _;

/// A client with no token, which is what an unconfigured deployment gets.
///
/// These tests never reach the network — none of the routes exercised here
/// makes an outbound request — so the client is here to satisfy the state, and
/// its being trivially constructible is the point: if building one could fail,
/// `AppState` would need to model its absence again.
fn unauthenticated_github() -> GitHubRestClient {
    GitHubRestClient::new(GitHubClientConfig::new())
        .expect("a client with the default API base and no token is always constructible")
}

/// Sends one request through the fully built router.
async fn get(uri: &str) -> (StatusCode, serde_json::Value) {
    let (app, _openapi) = api::build(AppState::without_database(unauthenticated_github()), None);

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

/// Sends one request with a body through the fully built router.
async fn send(
    method: &str,
    uri: &str,
    content_type: Option<&str>,
    body: &str,
) -> (StatusCode, Option<String>, serde_json::Value) {
    let (app, _openapi) = api::build(AppState::without_database(unauthenticated_github()), None);

    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = content_type {
        builder = builder.header("content-type", value);
    }

    let response = app
        .oneshot(
            builder
                .body(Body::from(body.to_owned()))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 65_536)
        .await
        .expect("body is readable");
    let parsed = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

    (status, content_type, parsed)
}

/// Asserts a response is the project's error envelope carrying `code`.
fn assert_envelope(
    status: StatusCode,
    expected: StatusCode,
    content_type: Option<&str>,
    body: &serde_json::Value,
    code: &str,
) {
    assert_eq!(status, expected);
    assert_eq!(
        content_type,
        Some("application/json"),
        "an error must be JSON like every other response; a client that has to \
         sniff content-type cannot rely on the contract"
    );
    assert_eq!(body["code"], code, "body was {body}");
    assert!(
        body["message"].as_str().is_some_and(|m| !m.is_empty()),
        "an envelope always explains itself: {body}"
    );
}

#[tokio::test]
async fn a_malformed_json_body_is_answered_with_the_envelope() {
    // `axum`'s own JsonRejection answers in plain text. That makes the envelope
    // a promise this API only sometimes keeps, which is worse than not making
    // it — the generated client types every error as ApiError.
    let (status, content_type, body) = send(
        "POST",
        "/api/v1/analyses",
        Some("application/json"),
        "{not json",
    )
    .await;

    assert_envelope(
        status,
        StatusCode::BAD_REQUEST,
        content_type.as_deref(),
        &body,
        "MALFORMED_REQUEST",
    );
}

#[tokio::test]
async fn a_missing_content_type_is_answered_with_the_envelope() {
    let (status, content_type, body) = send(
        "POST",
        "/api/v1/analyses",
        None,
        r#"{"repository_url":"https://github.com/rust-lang/crates.io"}"#,
    )
    .await;

    assert_envelope(
        status,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        content_type.as_deref(),
        &body,
        "MALFORMED_REQUEST",
    );
}

#[tokio::test]
async fn a_body_of_the_wrong_shape_is_answered_with_the_envelope() {
    // Valid JSON, wrong document. Distinct from a syntax error and reported
    // with its own status rather than flattened into one generic 400.
    let (status, content_type, body) = send(
        "POST",
        "/api/v1/analyses",
        Some("application/json"),
        r#"{"repository":"rust-lang/crates.io"}"#,
    )
    .await;

    assert_envelope(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        content_type.as_deref(),
        &body,
        "MALFORMED_REQUEST",
    );
}

#[tokio::test]
async fn an_oversized_body_is_answered_with_the_envelope() {
    // The limit is 16 KiB. This is well past it, and the rejection surfaces
    // through the JSON extractor rather than needing its own interception.
    let oversized = format!(r#"{{"repository_url":"{}"}}"#, "x".repeat(32 * 1024));
    let (status, content_type, body) = send(
        "POST",
        "/api/v1/analyses",
        Some("application/json"),
        &oversized,
    )
    .await;

    assert_envelope(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        content_type.as_deref(),
        &body,
        "REQUEST_TOO_LARGE",
    );
}

#[tokio::test]
async fn an_identifier_that_is_not_a_uuid_is_answered_with_the_envelope() {
    // 400, not 404. "You sent something that could never name an analysis" and
    // "no analysis has that id" are different facts, and only the first is a
    // typo the caller can fix.
    for uri in [
        "/api/v1/analyses/not-a-uuid",
        "/api/v1/analyses/not-a-uuid/report",
    ] {
        let (status, content_type, body) = send("GET", uri, None, "").await;
        assert_envelope(
            status,
            StatusCode::BAD_REQUEST,
            content_type.as_deref(),
            &body,
            "MALFORMED_REQUEST",
        );
    }
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

    let (app, _openapi) = api::build(AppState::without_database(unauthenticated_github()), None);
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
    let (app, _openapi) = api::build(AppState::without_database(unauthenticated_github()), None);

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
    let (_app, openapi) = api::build(AppState::without_database(unauthenticated_github()), None);

    for path in ["/healthz", "/api/v1/system/probe"] {
        assert!(
            openapi.paths.paths.contains_key(path),
            "a served route that is missing from the OpenAPI document would be \
             invisible to the generated TypeScript client: {path}"
        );
    }
}
