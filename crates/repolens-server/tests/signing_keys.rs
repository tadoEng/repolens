//! The Firebase signing-key cache, observed rather than reasoned about.
//!
//! Every assertion here is a **request count** against a mock key service. The
//! properties at stake are all about what leaves the process, and none of them
//! is visible in the cache's return value:
//!
//! * an unknown `kid` against a fresh cache must cost **zero** requests, or an
//!   unauthenticated caller can drive one outbound HTTPS call per attempt just
//!   by varying a header field;
//! * concurrent misses must collapse into **one** request, or an expiry becomes
//!   a stampede against the same endpoint;
//! * a rotated key must be picked up, or a real rotation locks every user out
//!   until somebody redeploys.

use std::sync::Arc;

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use repolens_server::auth::{AuthError, FirebaseVerifier};
use serde::Serialize;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PROJECT: &str = "repolens-test-project";
const JWK_PATH: &str = "/jwk";

/// One RSA keypair, in the two forms the two sides need.
struct Key {
    der: Vec<u8>,
    modulus_b64: String,
    exponent_b64: String,
}

fn generate() -> Key {
    use base64::Engine as _;
    use rsa::pkcs1::EncodeRsaPrivateKey as _;
    use rsa::traits::PublicKeyParts as _;
    use rsa::{RsaPrivateKey, RsaPublicKey};

    let mut rng = rand::thread_rng();
    let private = RsaPrivateKey::new(&mut rng, 2048).expect("a keypair is generated");
    let public = RsaPublicKey::from(&private);
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;

    Key {
        der: private
            .to_pkcs1_der()
            .expect("the private key encodes")
            .as_bytes()
            .to_vec(),
        modulus_b64: engine.encode(public.n().to_bytes_be()),
        exponent_b64: engine.encode(public.e().to_bytes_be()),
    }
}

#[derive(Serialize)]
struct Claims {
    sub: String,
    aud: String,
    iss: String,
    iat: i64,
    auth_time: i64,
    exp: i64,
}

fn mint(key: &Key, kid: &str) -> String {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());

    jsonwebtoken::encode(
        &header,
        &Claims {
            sub: "uid".to_owned(),
            aud: PROJECT.to_owned(),
            iss: format!("https://securetoken.google.com/{PROJECT}"),
            iat: now - 60,
            auth_time: now - 120,
            exp: now + 3600,
        },
        &EncodingKey::from_rsa_der(&key.der),
    )
    .expect("the token signs")
}

/// A JWK set carrying one key under `kid`.
fn jwk_set(key: &Key, kid: &str) -> serde_json::Value {
    json!({
        "keys": [{
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "kid": kid,
            "n": key.modulus_b64,
            "e": key.exponent_b64,
        }]
    })
}

/// A verifier pointed at a mock serving `body` with `max-age` seconds.
async fn served(body: serde_json::Value, max_age: u64) -> (MockServer, Arc<FirebaseVerifier>) {
    served_with(body, Some(max_age)).await
}

/// As [`served`], but `None` omits `Cache-Control` entirely.
async fn served_with(
    body: serde_json::Value,
    max_age: Option<u64>,
) -> (MockServer, Arc<FirebaseVerifier>) {
    let server = MockServer::start().await;
    let mut response = ResponseTemplate::new(200).set_body_json(body);
    if let Some(seconds) = max_age {
        response = response.insert_header("cache-control", format!("public, max-age={seconds}"));
    }
    Mock::given(method("GET"))
        .and(path(JWK_PATH))
        .respond_with(response)
        .mount(&server)
        .await;

    let verifier = FirebaseVerifier::with_jwk_url(PROJECT, format!("{}{JWK_PATH}", server.uri()))
        .expect("the verifier builds");
    (server, Arc::new(verifier))
}

/// How many requests the mock actually received.
async fn requests(server: &MockServer) -> usize {
    server.received_requests().await.map_or(0, |r| r.len())
}

#[tokio::test]
async fn an_unknown_kid_against_a_fresh_cache_costs_no_request() {
    // The exhaustion this cache exists to prevent. Previously a fresh cache
    // that lacked the `kid` fell through and fetched, so anyone could drive one
    // outbound HTTPS call per create attempt by varying a header field — no
    // credential required, because verification runs before everything else.
    let key = generate();
    let (server, verifier) = served(jwk_set(&key, "real-kid"), 3600).await;

    verifier
        .verify(&mint(&key, "real-kid"))
        .await
        .expect("the token verifies");
    assert_eq!(
        requests(&server).await,
        1,
        "priming costs exactly one fetch"
    );

    for attempt in 0..25 {
        let token = mint(&key, &format!("attacker-kid-{attempt}"));
        assert_eq!(
            verifier.verify(&token).await.unwrap_err(),
            AuthError::Malformed
        );
    }

    assert_eq!(
        requests(&server).await,
        1,
        "a fresh cache must answer an unknown kid by itself; every extra \
         request here is one an unauthenticated caller can trigger at will"
    );
}

#[tokio::test]
async fn concurrent_misses_collapse_into_one_fetch() {
    // Without the refresh lock every verification arriving on a cold cache
    // fetches, so an expiry turns into a burst against the same endpoint.
    let key = generate();
    let (server, verifier) = served(jwk_set(&key, "real-kid"), 3600).await;

    let mut tasks = Vec::new();
    for _ in 0..12 {
        let verifier = Arc::clone(&verifier);
        let token = mint(&key, "real-kid");
        tasks.push(tokio::spawn(async move { verifier.verify(&token).await }));
    }
    for task in tasks {
        task.await
            .expect("the task runs")
            .expect("the token verifies");
    }

    assert_eq!(
        requests(&server).await,
        1,
        "twelve concurrent cold verifications must share one fetch"
    );
}

#[tokio::test]
async fn a_rotated_key_set_is_picked_up_and_the_retired_one_stops_verifying() {
    // The reason the cache is bounded at all: Google rotates these keys. A
    // cache that never expired would lock every user out at the next rotation
    // until somebody redeployed.
    let first = generate();
    let (old_server, old) = served(jwk_set(&first, "kid-1"), 3600).await;
    old.verify(&mint(&first, "kid-1"))
        .await
        .expect("the first key verifies");
    assert_eq!(requests(&old_server).await, 1);

    // A process whose cache has expired refetches and sees the new set. Driven
    // by a fresh verifier rather than by waiting out the five-minute floor,
    // which would make this test sleep for no extra proof.
    let second = generate();
    let (new_server, rotated) = served(jwk_set(&second, "kid-2"), 3600).await;

    rotated
        .verify(&mint(&second, "kid-2"))
        .await
        .expect("the rotated key verifies");
    assert_eq!(requests(&new_server).await, 1);

    assert_eq!(
        rotated.verify(&mint(&first, "kid-1")).await.unwrap_err(),
        AuthError::Malformed,
        "the retired key must stop verifying"
    );
}

#[tokio::test]
async fn a_short_max_age_does_not_turn_every_verification_into_a_fetch() {
    // `max-age` is clamped, not trusted. A zero or near-zero value would make
    // the cache useless and reintroduce the exhaustion from upstream instead of
    // from a caller.
    let key = generate();
    let (server, verifier) = served(jwk_set(&key, "real-kid"), 0).await;

    for _ in 0..5 {
        verifier
            .verify(&mint(&key, "real-kid"))
            .await
            .expect("the token verifies");
    }

    assert_eq!(
        requests(&server).await,
        1,
        "max-age=0 must clamp to the floor rather than disabling the cache"
    );
}

#[tokio::test]
async fn a_key_service_that_is_down_reports_our_fault_not_the_callers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(JWK_PATH))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let verifier = FirebaseVerifier::with_jwk_url(PROJECT, format!("{}{JWK_PATH}", server.uri()))
        .expect("the verifier builds");
    let key = generate();

    // `KeysUnavailable`, never `Unauthenticated`: a client that treated an
    // outage as a rejected credential would sign a valid user out.
    assert_eq!(
        verifier.verify(&mint(&key, "kid-1")).await.unwrap_err(),
        AuthError::KeysUnavailable
    );
}

/// Primes the cache and reports the lifetime it was given.
async fn lifetime_for(max_age: Option<u64>) -> std::time::Duration {
    let key = generate();
    let (_server, verifier) = served_with(jwk_set(&key, "kid-1"), max_age).await;
    verifier
        .verify(&mint(&key, "kid-1"))
        .await
        .expect("the token verifies");
    verifier
        .cache_lifetime()
        .await
        .expect("a verification primes the cache")
}

/// Allows for the moments spent generating a key and completing a request.
fn about(lifetime: std::time::Duration, expected_secs: u64) -> bool {
    let expected = std::time::Duration::from_secs(expected_secs);
    lifetime <= expected && lifetime + std::time::Duration::from_mins(2) >= expected
}

#[tokio::test]
async fn the_cached_lifetime_is_the_one_google_served() {
    // The assertion that a tamper check found missing: every other observable
    // is identical whether the lifetime comes from `Cache-Control` or from a
    // constant, because the floor is five minutes and no suite waits that out.
    let two_hours = lifetime_for(Some(7200)).await;
    assert!(
        about(two_hours, 7200),
        "max-age=7200 must yield roughly two hours, got {two_hours:?} — a fixed          default would read one hour here"
    );
}

#[tokio::test]
async fn an_absent_cache_control_falls_back_rather_than_caching_forever() {
    let fallback = lifetime_for(None).await;
    assert!(
        about(fallback, 3600),
        "no Cache-Control must fall back to the default hour, got {fallback:?}"
    );
}

#[tokio::test]
async fn an_absurd_max_age_cannot_pin_a_retired_key_set() {
    // Clamped at a day. Without the ceiling, a header claiming a year would
    // keep verifying against keys Google retired months earlier.
    let clamped = lifetime_for(Some(365 * 24 * 3600)).await;
    assert!(
        about(clamped, 24 * 3600),
        "an absurd max-age must clamp to the 24-hour ceiling, got {clamped:?}"
    );
}

#[tokio::test]
async fn a_tiny_max_age_is_raised_to_the_floor() {
    let floored = lifetime_for(Some(1)).await;
    assert!(
        about(floored, 300),
        "max-age=1 must be raised to the five-minute floor, got {floored:?}"
    );
}
