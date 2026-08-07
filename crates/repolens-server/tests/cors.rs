//! The CORS policy, driven through the real router.
//!
//! The policy is a *browser* behaviour, so nothing here can be inferred from
//! reading the layer builder — a preflight either names the method or the
//! request is never sent. These drive the assembled `Router` for that reason.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use repolens_github::{GitHubClientConfig, GitHubRestClient};
use repolens_server::api;
use repolens_server::state::AppState;
use tower::ServiceExt as _;

const ORIGIN: &str = "https://repolens.example";

fn unauthenticated_github() -> GitHubRestClient {
    GitHubRestClient::new(GitHubClientConfig::new())
        .expect("a client with the default API base and no token is always constructible")
}

/// Sends a preflight for `method` against `uri`.
async fn preflight(uri: &str, method: &str) -> axum::http::HeaderMap {
    let state = AppState::without_database(unauthenticated_github());
    let (app, _openapi) = api::build(state, Some(ORIGIN));

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri(uri)
                .header("Origin", ORIGIN)
                .header("Access-Control-Request-Method", method)
                // Both, because the real request carries both: a JSON body and
                // a Firebase ID token. Preflighting only `content-type` is what
                // let a policy that forbade `authorization` stay green.
                .header(
                    "Access-Control-Request-Headers",
                    "content-type, authorization",
                )
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK, "the preflight must pass");
    response.headers().clone()
}

/// The permitted methods, lowercased and split.
fn allowed_methods(headers: &axum::http::HeaderMap) -> Vec<String> {
    header_list(headers, "access-control-allow-methods")
}

/// The permitted request headers, uppercased and split.
fn allowed_headers(headers: &axum::http::HeaderMap) -> Vec<String> {
    header_list(headers, "access-control-allow-headers")
}

fn header_list(headers: &axum::http::HeaderMap, name: &str) -> Vec<String> {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("a preflight response must carry {name}"))
        .to_str()
        .expect("the header is ASCII")
        .split(',')
        .map(|value| value.trim().to_ascii_uppercase())
        .collect()
}

#[tokio::test]
async fn the_preflight_permits_creating_an_analysis() {
    let headers = preflight("/api/v1/analyses", "POST").await;

    assert_eq!(
        headers
            .get("access-control-allow-origin")
            .expect("an exact origin is configured, so the header must be present"),
        ORIGIN
    );

    // The assertion that matters. `tower-http` answers a preflight with exactly
    // the configured method set, so a router allowing only GET fails here — and
    // *only* here: every other server-side test still passes, because the
    // browser is what refuses to send the request. Creating an analysis is the
    // one write this API has, and it is unreachable from the statically hosted
    // frontend without this.
    let methods = allowed_methods(&headers);
    assert!(
        methods.iter().any(|method| method == "POST"),
        "POST must be permitted or the frontend cannot create an analysis; got {methods:?}"
    );
}

#[tokio::test]
async fn the_preflight_permits_the_credential_the_frontend_sends() {
    // The production-only failure this asserts against: the browser preflights
    // the signed-in POST, sees that `authorization` is not permitted, and never
    // sends the request. Nothing server-side notices, because nothing
    // server-side is involved — which is why this is checked here and not
    // inferred from the handler compiling.
    let headers = allowed_headers(&preflight("/api/v1/analyses", "POST").await);

    assert!(
        headers.iter().any(|header| header == "AUTHORIZATION"),
        "creating an analysis carries a Firebase ID token; without this the          browser never sends the POST. Got {headers:?}"
    );
    assert!(
        headers.iter().any(|header| header == "CONTENT-TYPE"),
        "the request body is JSON; got {headers:?}"
    );
}

#[tokio::test]
async fn the_preflight_still_permits_reading() {
    // Polling an analysis and fetching its report are both GETs. Adding POST
    // must not have replaced the method set.
    let methods = allowed_methods(&preflight("/api/v1/analyses/x/report", "GET").await);
    assert!(
        methods.iter().any(|method| method == "GET"),
        "GET must stay permitted for polling and reading a report; got {methods:?}"
    );
}

#[tokio::test]
async fn the_policy_names_one_exact_origin_and_never_a_wildcard() {
    // A wildcard would have to be revisited the moment an endpoint needs
    // credentials, and it is the kind of widening that happens by omission
    // rather than by decision.
    let headers = preflight("/api/v1/analyses", "POST").await;
    let origin = headers
        .get("access-control-allow-origin")
        .expect("the header must be present")
        .to_str()
        .expect("the header is ASCII");

    assert_eq!(origin, ORIGIN);
    assert_ne!(origin, "*");
}

#[tokio::test]
async fn an_unparseable_origin_serves_without_cors_rather_than_refusing_to_start() {
    // Refusing to start would take the API down over a variable that only
    // affects browsers. Serving without CORS fails visibly in the browser and
    // loudly in the log.
    let state = AppState::without_database(unauthenticated_github());
    let (app, _openapi) = api::build(state, Some("not a header value\u{7f}"));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/system/probe")
                .header("Origin", ORIGIN)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "an unusable origin must yield no CORS header rather than a permissive one"
    );
}
