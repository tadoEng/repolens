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
use repolens_server::telemetry::metrics::{Metrics, StatusClass};
use tower::ServiceExt as _;

/// The shipped budget is 30 seconds; asserting against it would mean a
/// 30-second test.
const TEST_TIMEOUT: Duration = Duration::from_millis(50);

/// Sends one request through `router` wrapped in the production layer stack.
async fn through_the_stack(router: Router, uri: &str) -> (StatusCode, Option<String>, String) {
    let (status, content_type, body, _metrics) = through_the_stack_recording(router, uri).await;
    (status, content_type, body)
}

/// As [`through_the_stack`], returning the registry the stack recorded into.
async fn through_the_stack_recording(
    router: Router,
    uri: &str,
) -> (StatusCode, Option<String>, String, Metrics) {
    let metrics = Metrics::new();
    let app = api::apply_layers(router, None, TEST_TIMEOUT, &metrics);

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
        metrics,
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
async fn a_panicking_handler_is_still_counted_as_a_server_error() {
    // The position of the metrics layer relative to the panic layer decides what
    // a 5xx count means. Inside it, the unwind would pass straight through the
    // recording call and the request would disappear from the figures — an error
    // rate that *falls* as the service breaks. This is that ordering asserted as
    // a behaviour, because it is invisible in the builder.
    let router = Router::new().route("/boom", get(boom));

    let (status, _content_type, _body, metrics) =
        through_the_stack_recording(router, "/boom").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.routes.len(), 1, "one request, one series");
    let sample = &snapshot.routes[0];
    assert_eq!(sample.count, 1);
    assert_eq!(sample.in_status_class(StatusClass::ServerError), 1);
    assert_eq!(
        snapshot.in_flight, 0,
        "the gauge is lowered by a guard, which drops while unwinding; a gauge \
         that leaks on panic reads as saturation forever"
    );
}

#[tokio::test]
async fn a_timed_out_handler_is_counted_as_the_status_the_caller_saw() {
    let router = Router::new().route(
        "/slow",
        get(|| async {
            tokio::time::sleep(TEST_TIMEOUT * 20).await;
            Json(serde_json::json!({ "never": "sent" }))
        }),
    );

    let (status, _content_type, _body, metrics) =
        through_the_stack_recording(router, "/slow").await;
    assert_eq!(status, StatusCode::REQUEST_TIMEOUT);

    let snapshot = metrics.snapshot();
    let sample = &snapshot.routes[0];
    assert_eq!(
        sample.in_status_class(StatusClass::ClientError),
        1,
        "408 is what the caller received, so 408 is what is counted; recording \
         the status the handler intended would describe a response nobody got"
    );
}

#[tokio::test]
async fn a_request_is_in_flight_while_it_is_being_served() {
    // The gauge is only worth having if it is ever non-zero. Asserted from
    // inside the handler, which is the one place a request is provably still
    // being served.
    let metrics = Metrics::new();
    let observed = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));

    let seen = std::sync::Arc::clone(&observed);
    let inner = metrics.clone();
    let router = Router::new().route(
        "/serving",
        get(move || async move {
            seen.store(inner.in_flight(), std::sync::atomic::Ordering::Relaxed);
            Json(serde_json::json!({ "status": "ok" }))
        }),
    );

    let app = api::apply_layers(router, None, TEST_TIMEOUT, &metrics);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/serving")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        observed.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "a request being served must be counted as in flight"
    );
    assert_eq!(metrics.in_flight(), 0, "and must not be once it is done");
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
