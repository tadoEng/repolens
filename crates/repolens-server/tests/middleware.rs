//! The layer stack, exercised as a stack.
//!
//! A panic and a timeout are the two failures no shipped route can produce, so
//! they are driven through [`api::apply_layers`] applied to throwaway routes
//! rather than through `api::build`. Putting a handler that panics — or one
//! that outlasts the request budget — into the real router to make it testable
//! would ship a fault injector.
//!
//! What matters is that this is the *same* stack: the previous version of these
//! tests called the conversion functions directly, which proved the functions
//! worked and proved nothing about whether they were mounted, in what order, or
//! whether a later layer overwrote their work.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use repolens_server::api;
use tower::ServiceExt as _;

/// The shipped budget is 30 seconds; asserting against it would mean a
/// 30-second test.
const TEST_TIMEOUT: Duration = Duration::from_millis(50);

/// Sends one request through `router` wrapped in the production layer stack.
async fn through_the_stack(router: Router, uri: &str) -> (StatusCode, Option<String>, String) {
    let app = api::apply_layers(router, None, TEST_TIMEOUT);

    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("the stack always answers, even when the handler does not");

    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 65_536)
        .await
        .expect("body is readable");

    (
        status,
        content_type,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// A payload shaped like the things a real panic message can carry.
const SECRET_SHAPED_PAYLOAD: &str =
    "postgres://EXAMPLE_USER:EXAMPLE_PASSWORD@db.example.invalid/repolens at /home/runner/src/x.rs";

/// A handler that panics. An `async fn` rather than a closure because a
/// diverging closure body gives the router no response type to infer.
async fn boom() -> String {
    panic!("{SECRET_SHAPED_PAYLOAD}")
}

#[tokio::test]
async fn a_panicking_handler_answers_with_the_envelope() {
    let router = Router::new().route("/boom", get(boom));

    let (status, content_type, body) = through_the_stack(router, "/boom").await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        content_type.as_deref(),
        Some("application/json"),
        "the default CatchPanicLayer answers in plain text, which is the single \
         most surprising thing a JSON client can receive; got {content_type:?}"
    );

    let parsed: serde_json::Value = serde_json::from_str(&body).expect("the body is the envelope");
    assert_eq!(parsed["code"], "INTERNAL_ERROR");
    assert!(parsed["message"].as_str().is_some_and(|m| !m.is_empty()));
}

#[tokio::test]
async fn a_panic_payload_never_reaches_the_caller() {
    let router = Router::new().route("/boom", get(boom));

    let (_status, _content_type, body) = through_the_stack(router, "/boom").await;

    // A panic message is built by whatever code panicked, so it can carry a
    // connection string, an internal path, or a fragment of a repository file
    // the handler was holding.
    for fragment in [
        "EXAMPLE_PASSWORD",
        "db.example.invalid",
        "/home/runner",
        "postgres://",
    ] {
        assert!(
            !body.contains(fragment),
            "the panic payload leaked {fragment:?} into the response: {body}"
        );
    }
}

#[tokio::test]
async fn a_handler_that_outlasts_the_budget_answers_with_the_envelope() {
    let router = Router::new().route(
        "/slow",
        get(|| async {
            tokio::time::sleep(TEST_TIMEOUT * 20).await;
            Json(serde_json::json!({ "never": "sent" }))
        }),
    );

    let (status, content_type, body) = through_the_stack(router, "/slow").await;

    assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
    assert_eq!(
        content_type.as_deref(),
        Some("application/json"),
        "TimeoutLayer builds its own response with an empty body; got {content_type:?}"
    );

    let parsed: serde_json::Value = serde_json::from_str(&body).expect("the body is the envelope");
    assert_eq!(parsed["code"], "REQUEST_TIMED_OUT");
}

#[tokio::test]
async fn a_handler_inside_the_budget_is_left_alone() {
    // The timeout rewrite keys on status alone, which is only sound while it
    // leaves every other response exactly as it found it.
    let router = Router::new().route(
        "/quick",
        get(|| async { Json(serde_json::json!({ "status": "ok" })) }),
    );

    let (status, _content_type, body) = through_the_stack(router, "/quick").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"status":"ok"}"#);
}
