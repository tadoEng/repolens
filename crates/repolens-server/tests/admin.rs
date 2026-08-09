//! The authorisation gate on the operational snapshot, and what it publishes.
//!
//! Driven through the **real router** rather than by calling the extractor, for
//! the reason the CORS layer taught this repository once already: a gate that is
//! written and not mounted passes every unit test it has. What is asserted here
//! is what an HTTP client receives.
//!
//! Three cases carry the contract from issue #37 — anonymous, signed-in
//! non-admin, allow-listed admin — and the rest exist because each is a way the
//! door could be opened by accident: a deployment with no Firebase project, an
//! empty allowlist, a uid that differs only in case, a bad token reaching the
//! allowlist check at all.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use repolens_github::{GitHubClientConfig, GitHubRestClient};
use repolens_server::api;
use repolens_server::auth::FirebaseVerifier;
use repolens_server::state::AppState;
use secrecy::SecretString;
use support::{CALLER_UID, KID, PROJECT, claims_for, mint, now, valid_claims};
use tower::ServiceExt as _;

const OVERVIEW: &str = "/api/v1/admin/overview";

/// The uid the tests allow-list.
const ADMIN_UID: &str = "firebase-uid-admin-7f21";

fn github() -> GitHubRestClient {
    GitHubRestClient::new(GitHubClientConfig::new()).expect("constructible")
}

/// A router that verifies tokens and allow-lists `admins`.
fn app_with_admins(admins: &[&str]) -> axum::Router {
    let state = AppState::without_database(github())
        .with_verifier(std::sync::Arc::new(FirebaseVerifier::with_keys(
            PROJECT,
            support::decoding_keys(),
        )))
        .with_admin_uids(admins.iter().map(|uid| (*uid).to_owned()));

    api::build(state, None).0
}

/// The ordinary deployment: tokens are verified and one uid is an admin.
fn app() -> axum::Router {
    app_with_admins(&[ADMIN_UID])
}

/// A router with no verifier, standing in for a forgotten `FIREBASE_PROJECT_ID`.
///
/// The allowlist is populated on purpose. A deployment can configure admins and
/// forget the project, and the interesting question is whether the allowlist can
/// admit anybody when no identity can be established — it must not.
fn app_without_auth() -> axum::Router {
    api::build(
        AppState::without_database(github()).with_admin_uids([ADMIN_UID.to_owned()]),
        None,
    )
    .0
}

/// Sends a `GET` to `uri`, optionally bearing `token`.
async fn get(app: axum::Router, uri: &str, token: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().uri(uri);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let response = app
        .oneshot(builder.body(Body::empty()).expect("request builds"))
        .await
        .expect("router responds");

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body is readable");
    (status, bytes.to_vec())
}

/// The same, parsed as JSON.
async fn get_json(
    app: axum::Router,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let (status, bytes) = get(app, uri, token).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn admin_token() -> String {
    mint(&claims_for(ADMIN_UID), KID)
}

#[tokio::test]
async fn an_anonymous_caller_is_refused() {
    let (status, body) = get_json(app(), OVERVIEW, None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn a_signed_in_caller_who_is_not_an_admin_is_refused() {
    // The case that separates authentication from authorisation. This caller
    // holds a perfectly valid token for the right project — everything the
    // creation endpoint asks for — and still may not read this.
    let (status, body) = get_json(app(), OVERVIEW, Some(&mint(&valid_claims(), KID))).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body["code"], "FORBIDDEN",
        "a non-admin must not be told to sign in again; signing in is not the remedy"
    );
}

#[tokio::test]
async fn an_allow_listed_admin_is_served() {
    let (status, body) = get_json(app(), OVERVIEW, Some(&admin_token())).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    // The shape, not merely the status: a 200 carrying an error envelope would
    // pass a status-only assertion.
    assert!(body["process"]["build_sha"].is_string());
    assert!(body["process"]["uptime_seconds"].is_u64());
    assert!(
        body["process"].get("resident_bytes").is_some(),
        "resident_bytes is required-but-nullable and must always be present"
    );
    assert!(body["http"]["routes"].is_array());
}

#[tokio::test]
async fn an_empty_allowlist_admits_nobody() {
    // The forgotten-variable case, and the direction it has to fail in. A
    // deployment with no ADMIN_FIREBASE_UIDS serves its public endpoints and
    // refuses this one; the opposite default would publish process internals to
    // whoever found the path.
    for token in [mint(&valid_claims(), KID), admin_token()] {
        let (status, body) = get_json(app_with_admins(&[]), OVERVIEW, Some(&token)).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "FORBIDDEN");
    }
}

#[tokio::test]
async fn no_configured_project_closes_the_endpoint_rather_than_opening_it() {
    // 503 rather than 401, for the same reason creation answers 503: the fault
    // is ours, and a client that read it as a rejected credential would sign a
    // valid user out. The allowlist is configured here, so this also proves the
    // allowlist alone cannot admit anyone — an identity has to be established
    // first, and without a verifier none can be.
    for token in [None, Some(admin_token())] {
        let (status, body) = get_json(app_without_auth(), OVERVIEW, token.as_deref()).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "AUTHENTICATION_UNAVAILABLE");
    }
}

#[tokio::test]
async fn a_bad_credential_is_refused_as_unauthenticated_and_never_reaches_the_allowlist() {
    // Order matters for what a caller learns. Answering 403 to a garbage token
    // would tell a stranger that the identity they invented exists and merely
    // lacks permission.
    let expired = {
        let mut claims = claims_for(ADMIN_UID);
        claims.iat = Some(now() - 7200);
        claims.auth_time = Some(now() - 7200);
        claims.exp = Some(now() - 3600);
        mint(&claims, KID)
    };
    let wrong_project = {
        let mut claims = claims_for(ADMIN_UID);
        claims.aud = Some("someone-elses-project".to_owned());
        mint(&claims, KID)
    };
    let unknown_key = mint(&claims_for(ADMIN_UID), "not-our-kid");

    for token in ["not-a-jwt", &expired, &wrong_project, &unknown_key] {
        let (status, body) = get_json(app(), OVERVIEW, Some(token)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED, "should refuse {token:?}");
        assert_eq!(body["code"], "UNAUTHENTICATED");
    }
}

#[tokio::test]
async fn the_allowlist_is_case_sensitive_end_to_end() {
    // A Firebase uid is case-sensitive. An allowlist that folded case would
    // admit an identity nobody configured, and `config::parse_admin_uids`
    // already refuses to normalise for that reason — this asserts the rule
    // survives all the way to the response, rather than being kept at one end
    // and quietly relaxed at the other.
    let shouted = ADMIN_UID.to_uppercase();
    assert_ne!(shouted, ADMIN_UID, "the fixture uid must contain letters");

    let (status, _body) = get_json(app(), OVERVIEW, Some(&mint(&claims_for(&shouted), KID))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_snapshot_reports_the_traffic_this_process_actually_served() {
    // The endpoint and the recording middleware have to be looking at one
    // registry. Two would each show half the traffic, and the dashboard would
    // be wrong in a way that reads as a quiet week.
    let app = app();
    for _ in 0..3 {
        let (status, _) = get(app.clone(), "/healthz", None).await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, body) = get_json(app, OVERVIEW, Some(&admin_token())).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let routes = body["http"]["routes"].as_array().expect("a route table");
    let healthz = routes
        .iter()
        .find(|row| row["route"] == "/healthz")
        .unwrap_or_else(|| panic!("no /healthz row in {body}"));

    assert_eq!(healthz["method"], "GET");
    assert_eq!(healthz["requests"], 3);
    assert_eq!(healthz["responses"]["success"], 3);
    assert_eq!(healthz["responses"]["server_error"], 0);
    // The percentile carries the bucket it was read from, so a reader can tell
    // an interpolation from a measurement. Dropping those bounds in translation
    // would leave a figure that looks exact.
    assert!(healthz["latency"]["p50"]["lower_bound_micros"].is_u64());
    assert!(
        healthz["latency"]["p99"]
            .get("upper_bound_micros")
            .is_some(),
        "upper_bound_micros is required-but-nullable and must always be present"
    );

    assert_eq!(
        body["http"]["in_flight"], 1,
        "the request reading the snapshot is itself in flight, and the figure says so \
         rather than pretending the process is idle"
    );
    assert_eq!(body["http"]["max_tracked_routes"], 64);
}

#[tokio::test]
async fn a_route_label_is_never_a_concrete_path() {
    // The cardinality rule, asserted where a client can see it. An analysis id
    // in a label is an unbounded map key in a process that never restarts, and
    // it is also a repository-shaped identifier published to a dashboard.
    let app = app();
    let analysis = "/api/v1/analyses/019fdb48-0000-7000-8000-000000000001";
    // No database is configured, so this answers 503 — which is what makes it a
    // server error below, and the two classes have to stay apart.
    let (status, _) = get(app.clone(), analysis, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    // A path that matches no route at all: the URI here is the one input a
    // stranger writes in full, so it is the likeliest thing to end up as a key.
    let (status, _) = get(app.clone(), "/no-such-path-4f2a", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = get_json(app, OVERVIEW, Some(&admin_token())).await;
    assert_eq!(status, StatusCode::OK);

    let rendered = body.to_string();
    assert!(
        !rendered.contains("019fdb48"),
        "an analysis id reached the published route table: {rendered}"
    );
    assert!(
        !rendered.contains("no-such-path-4f2a"),
        "an unmatched URI reached the published route table: {rendered}"
    );

    let routes = body["http"]["routes"].as_array().expect("a route table");
    let row = |label: &str| {
        routes
            .iter()
            .find(|row| row["route"] == label)
            .unwrap_or_else(|| panic!("no {label} row in {body}"))
    };

    // The matched pattern is what should have been recorded, and the `{}` in it
    // is the proof: a concrete id could not produce that string.
    let matched = row("/api/v1/analyses/{analysis_id}");
    // A request that matched nothing is still counted, under a fixed label that
    // costs one series however many distinct paths are probed.
    let unmatched = row("<unmatched>");

    // The two status classes land in different fields. Asserted on traffic that
    // produced one of each, because a table of nothing but `2xx` would let
    // `client_error` and `server_error` be swapped in translation without a
    // single assertion noticing — and a dashboard that reported our faults as
    // the caller's is exactly the wrong way round.
    assert_eq!(matched["responses"]["server_error"], 1, "{matched}");
    assert_eq!(matched["responses"]["client_error"], 0, "{matched}");
    assert_eq!(unmatched["responses"]["client_error"], 1, "{unmatched}");
    assert_eq!(unmatched["responses"]["server_error"], 0, "{unmatched}");
}

/// Distinctive values, planted where this process actually holds its secrets.
///
/// Each is the *kind* of value the corresponding environment variable carries,
/// seeded through the same constructors `bin/server.rs` uses — so a future field
/// that reached into `AppState` for the database URL, the GitHub token, the
/// Firebase project, or the allow-listed uid would carry one of these into the
/// response and fail the test below.
///
/// They cannot be planted by setting environment variables: this workspace
/// forbids `unsafe_code`, and edition 2024 made `set_var` unsafe. Seeding the
/// state is the stronger version anyway — it is the state a handler can reach,
/// rather than the environment a handler would have to go and read.
const CANARY_DATABASE_URL: &str =
    "postgres://CANARY_USER:CANARY_PASSWORD_8b41d7@db.canary.invalid/repolens?sslmode=verify-full";
const CANARY_GITHUB_TOKEN: &str = "CANARY_GITHUB_TOKEN_8b41d7";
const CANARY_PROJECT: &str = "canary-firebase-project-8b41d7";

#[tokio::test]
async fn the_response_carries_no_configuration_the_process_holds() {
    // Asserted against the serialized response bytes, not against the DTO. A
    // field added to a struct is reviewed; a field added to a struct that
    // happens to serialize a connection string is what this catches — and it
    // catches it whatever route the value took to get there, including a
    // `Display` impl nobody read.
    let github = GitHubRestClient::new(
        GitHubClientConfig::new().with_token(SecretString::from(CANARY_GITHUB_TOKEN)),
    )
    .expect("constructible");

    // Lazy, so no connection is attempted and the host never has to exist. The
    // URL is still parsed and held, which is the point.
    let pool = AppState::connect_lazy(CANARY_DATABASE_URL).expect("the URL parses");

    let state = AppState::with_pool(pool, github)
        .with_verifier(std::sync::Arc::new(FirebaseVerifier::with_keys(
            CANARY_PROJECT,
            support::decoding_keys(),
        )))
        .with_admin_uids([ADMIN_UID.to_owned()]);

    // Minted for the canary project, since that is what this verifier accepts.
    let token = {
        let mut claims = claims_for(ADMIN_UID);
        claims.aud = Some(CANARY_PROJECT.to_owned());
        claims.iss = Some(format!("https://securetoken.google.com/{CANARY_PROJECT}"));
        mint(&claims, KID)
    };

    let (status, bytes) = get(api::build(state, None).0, OVERVIEW, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    let body = String::from_utf8(bytes).expect("the response is UTF-8");

    for secret in [
        CANARY_DATABASE_URL,
        "CANARY_PASSWORD_8b41d7",
        "CANARY_USER",
        CANARY_GITHUB_TOKEN,
        CANARY_PROJECT,
        // The reader's own identity. A snapshot that named who requested it
        // would put a Firebase uid on a dashboard, and the allowlist is
        // deliberately not logged for the same reason.
        ADMIN_UID,
        CALLER_UID,
        &token,
    ] {
        assert!(
            !body.contains(secret),
            "the operational snapshot published a value the process holds in confidence"
        );
    }

    // Shapes rather than exact values, so a *differently* named secret is
    // caught too. `postgres://` and a bearer prefix have no business in this
    // payload whatever variable they came from.
    for shape in ["postgres://", "sslmode", "Bearer ", "BEGIN "] {
        assert!(
            !body.contains(shape),
            "the operational snapshot published something shaped like a credential: {shape}"
        );
    }

    // Proves the assertions above were run against a real payload rather than
    // an empty one, which is the way a canary test quietly stops testing.
    assert!(
        body.contains("build_sha"),
        "the body is the snapshot: {body}"
    );
    assert!(body.len() > 100, "suspiciously short body: {body}");
}
