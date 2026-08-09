//! The authentication gate on analysis creation.
//!
//! Tokens are **minted in-process** against a throwaway RSA key, and the
//! verifier is handed that key instead of Google's — see [`support`], which
//! `tests/admin.rs` shares so that neither suite can end up asserting against
//! its own private idea of a valid token.
//!
//! What that buys is the ability to test the checks that matter by constructing
//! tokens that fail exactly one of them: wrong audience, wrong issuer, expired,
//! signed by a key the verifier does not have.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use repolens_github::{GitHubClientConfig, GitHubRestClient};
use repolens_server::api;
use repolens_server::auth::FirebaseVerifier;
use repolens_server::state::AppState;
use support::{KID, PROJECT, decoding_keys, mint, now, valid_claims};
use tower::ServiceExt as _;

/// A router whose verifier trusts the fixture key and nothing else.
fn app_with_auth() -> axum::Router {
    let github = GitHubRestClient::new(GitHubClientConfig::new()).expect("constructible");
    let state = AppState::without_database(github).with_verifier(std::sync::Arc::new(
        FirebaseVerifier::with_keys(PROJECT, decoding_keys()),
    ));

    api::build(state, None).0
}

/// A router with no verifier configured at all.
fn app_without_auth() -> axum::Router {
    let github = GitHubRestClient::new(GitHubClientConfig::new()).expect("constructible");
    api::build(AppState::without_database(github), None).0
}

/// Posts a creation request, optionally bearing `token`.
async fn create(app: axum::Router, token: Option<&str>) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/analyses")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let response = app
        .oneshot(
            builder
                .body(Body::from(
                    r#"{"repository_url":"https://github.com/rust-lang/crates.io"}"#,
                ))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 16_384)
        .await
        .expect("body is readable");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn creation_without_a_token_is_refused() {
    let (status, body) = create(app_with_auth(), None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn creation_with_a_valid_token_gets_past_the_gate() {
    // The state has no database, so the handler answers 503 — and that is the
    // proof: reaching the store means the gate admitted the caller. Asserting
    // 202 would need a database and would be testing creation, not the gate.
    let (status, body) = create(app_with_auth(), Some(&mint(&valid_claims(), KID))).await;

    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "a valid token must not be refused: {body}"
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["code"], "WORKER_FAILED_RETRIABLE",
        "the request reached the store, which is what proves the gate opened"
    );
}

#[tokio::test]
async fn a_token_for_another_project_is_refused() {
    // The check that stops anyone with *any* Firebase project from creating
    // work here: a token minted by a different project is signed by the same
    // Google keys in production, and only `aud` and `iss` separate them.
    let mut claims = valid_claims();
    claims.aud = Some("someone-elses-project".to_owned());

    let (status, body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn a_token_from_the_wrong_issuer_is_refused() {
    let mut claims = valid_claims();
    claims.iss = Some("https://accounts.example.invalid/".to_owned());

    let (status, _body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let mut claims = valid_claims();
    claims.iat = Some(now() - 7200);
    claims.auth_time = Some(now() - 7200);
    claims.exp = Some(now() - 3600);

    let (status, body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn a_token_signed_by_an_unknown_key_is_refused() {
    let (status, _body) =
        create(app_with_auth(), Some(&mint(&valid_claims(), "not-our-kid"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_garbage_token_is_refused() {
    for token in ["", "not-a-jwt", "a.b.c", "Bearer"] {
        let (status, _body) = create(app_with_auth(), Some(token)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "should refuse {token:?}");
    }
}

#[tokio::test]
async fn creation_is_closed_when_no_project_is_configured() {
    // The failure direction that matters. A deployment that forgot
    // FIREBASE_PROJECT_ID must not serve an anonymous, public, work-creating
    // endpoint — and 503 rather than 401 says the fault is ours, so a client
    // does not sign a valid user out over it.
    let (status, body) = create(app_without_auth(), None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "AUTHENTICATION_UNAVAILABLE");
}

#[tokio::test]
async fn a_valid_token_cannot_open_creation_that_is_closed() {
    let (status, body) = create(app_without_auth(), Some(&mint(&valid_claims(), KID))).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "AUTHENTICATION_UNAVAILABLE");
}

#[tokio::test]
async fn reading_stays_anonymous() {
    // The other half of the contract: a report is shared by URL, and the
    // unguessable id is the capability. Gating reads would break every shared
    // link and is not what #13 asks for.
    //
    // No database is configured, so these answer 503 rather than 200 — but 503
    // is the *store* answering, which means no credential was demanded first.
    for uri in [
        "/api/v1/analyses/00000000-0000-7000-8000-000000000000",
        "/api/v1/analyses/00000000-0000-7000-8000-000000000000/report",
        "/api/v1/system/probe",
        "/healthz",
    ] {
        let response = app_with_auth()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} must remain readable without signing in"
        );
    }
}

/// Posts an arbitrary body past the gate, bearing a valid token.
async fn create_body(body: &str) -> (StatusCode, Option<String>, serde_json::Value) {
    let token = mint(&valid_claims(), KID);
    let response = app_with_auth()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/analyses")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
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
    (
        status,
        content_type,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn past_the_gate_the_body_envelope_still_applies() {
    // These proofs used to live in `tests/api.rs`, where creation was
    // anonymous. The gate now answers first, so this is the only place a
    // malformed body reaches the JSON extractor at all — and the envelope has
    // to survive the extra layer rather than being shadowed by it.
    for (body, expected, code) in [
        ("{not json", StatusCode::BAD_REQUEST, "MALFORMED_REQUEST"),
        (
            r#"{"repository":"wrong shape"}"#,
            StatusCode::UNPROCESSABLE_ENTITY,
            "MALFORMED_REQUEST",
        ),
    ] {
        let (status, content_type, parsed) = create_body(body).await;

        assert_eq!(status, expected, "for {body:?}: {parsed}");
        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert_eq!(parsed["code"], code, "for {body:?}");
    }
}

#[tokio::test]
async fn past_the_gate_an_oversized_body_is_still_refused() {
    let oversized = format!(r#"{{"repository_url":"{}"}}"#, "x".repeat(32 * 1024));
    let (status, _content_type, parsed) = create_body(&oversized).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(parsed["code"], "REQUEST_TOO_LARGE");
}

#[tokio::test]
async fn an_invalid_repository_url_is_rejected_only_after_authentication() {
    // The order that matters for abuse: URL validation is work done on behalf
    // of a caller, so it happens after the caller is known.
    let (status, _content_type, parsed) = create_body(r#"{"repository_url":"not a url"}"#).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(parsed["code"], "INVALID_REPOSITORY_URL");
}

/// The window a token minted "now" would sit in. Well past the 60s leeway the
/// verifier allows for clock skew, so these are unambiguously future-dated.
const CLEARLY_FUTURE: i64 = 3600;

#[tokio::test]
async fn a_token_issued_in_the_future_is_refused() {
    // Firebase requires `iat` to be in the past, and `jsonwebtoken` does not
    // check it — `Validation` recognises `exp`, `nbf`, `aud`, `iss` and `sub`
    // and nothing else. Left to it, this check reads as present and never runs.
    let mut claims = valid_claims();
    claims.iat = Some(now() + CLEARLY_FUTURE);

    let (status, body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn a_token_whose_authentication_is_in_the_future_is_refused() {
    // `auth_time` answers a different question from `iat`: a refreshed token
    // carries a new `iat` and the original `auth_time`, so a future value here
    // is not covered by the check above.
    let mut claims = valid_claims();
    claims.auth_time = Some(now() + CLEARLY_FUTURE);

    let (status, body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn a_token_without_an_issued_at_claim_is_refused() {
    // Required, not merely validated-when-present. A token that simply omits
    // the claim must not slip past the range check by having nothing to compare.
    let mut claims = valid_claims();
    claims.iat = None;

    let (status, _body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_without_an_auth_time_claim_is_refused() {
    let mut claims = valid_claims();
    claims.auth_time = None;

    let (status, _body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_issued_within_the_clock_skew_window_is_still_accepted() {
    // The other direction, and the reason the leeway exists: Google's clock and
    // this process's clock are not the same clock. Rejecting a token a second
    // "in the future" would be an intermittent sign-in failure nobody could
    // reproduce.
    let mut claims = valid_claims();
    claims.iat = Some(now() + 5);
    claims.auth_time = Some(now() + 5);

    let (status, body) = create(app_with_auth(), Some(&mint(&claims, KID))).await;
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "a few seconds of skew must not refuse a valid token: {body}"
    );
}
