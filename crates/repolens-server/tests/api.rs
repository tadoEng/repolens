//! HTTP surface tests.
//!
//! Exercised through the real `axum` router rather than a mock, so the routing,
//! middleware stack, and OpenAPI collection under test are the ones that ship.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use repolens_server::api;
use tower::ServiceExt as _;

#[tokio::test]
async fn liveness_reports_ok() {
    let (app, _openapi) = api::build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("body is readable");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("body is JSON");

    assert_eq!(parsed["status"], "ok");
}

#[tokio::test]
async fn unknown_paths_are_not_found() {
    let (app, _openapi) = api::build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/system/probe")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    // Owned by issue #11. Asserted so that adding it is a deliberate act with a
    // failing test attached, rather than something that quietly already worked.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn openapi_document_is_collected_from_the_live_router() {
    let (_app, openapi) = api::build();

    assert!(
        openapi.paths.paths.contains_key("/healthz"),
        "a served route that is missing from the OpenAPI document would be \
         invisible to the generated TypeScript client"
    );
}
