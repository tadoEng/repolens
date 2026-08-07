//! Firebase ID token verification.
//!
//! Creating an analysis spends GitHub request budget and database rows on
//! behalf of whoever asked, so it is the one write this API has and it is
//! authenticated. Reading progress and reading a report stay anonymous: the
//! unguessable analysis id *is* the capability, which is what lets a report be
//! shared by URL.
//!
//! # No service-account credential
//!
//! Verifying an ID token needs Google's **public** signing keys and the project
//! id, both of which are public. There is deliberately no service account here:
//! the Admin SDK's private key would be a high-value secret to hold, rotate and
//! leak, and it buys nothing this endpoint needs — minting tokens and
//! administering users are not operations RepoLens performs.
//!
//! # What is checked
//!
//! Everything Google documents as required for an ID token, because a
//! verification that skips one of these accepts a token minted for a different
//! project or one that has expired:
//!
//! * RS256, and a `kid` present in the current key set;
//! * signature against that key;
//! * `aud` equal to the project id;
//! * `iss` equal to `https://securetoken.google.com/<project id>`;
//! * `exp` in the future — enforced by `jsonwebtoken`;
//! * `iat` and `auth_time` both present and **not** in the future — enforced
//!   here, because `Validation` does not know those claims. Its
//!   `set_required_spec_claims` recognises `exp`, `nbf`, `aud`, `iss` and
//!   `sub` and nothing else, so leaving these to it would have been a check
//!   that reads as present and never runs;
//! * `sub` non-empty — it becomes the user identity.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use time::OffsetDateTime;
use tokio::sync::RwLock;

/// Where Google publishes the public keys that verify ID tokens.
///
/// The JWK form rather than the X.509 form: `jsonwebtoken` builds a decoding
/// key straight from the RSA modulus and exponent, so no certificate parsing is
/// needed.
const GOOGLE_JWK_URL: &str =
    "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";

/// Fallback lifetime when Google's response carries no usable `max-age`.
///
/// Rotation is not abrupt — retired keys keep verifying outstanding tokens for
/// their lifetime — so a slightly stale set fails closed by missing a `kid`
/// rather than by accepting anything.
const DEFAULT_KEY_CACHE_TTL: Duration = Duration::from_hours(1);

/// Floor on the cached lifetime.
///
/// A `max-age` of zero or a few seconds would turn every verification into a
/// fetch, which is the same denial-of-service the cache exists to prevent —
/// only triggered by Google rather than by a caller.
const MIN_KEY_CACHE_TTL: Duration = Duration::from_mins(5);

/// Ceiling on the cached lifetime, so an absurd `max-age` cannot pin a
/// retired key set indefinitely.
const MAX_KEY_CACHE_TTL: Duration = Duration::from_hours(24);

/// Ceiling on the key-set response. Google's is a few kilobytes.
const MAX_JWK_BYTES: usize = 64 * 1024;

/// Tolerance for clock difference between Google and this process.
///
/// Matches `jsonwebtoken`'s own default leeway, so `exp` and the claims checked
/// by hand below treat time the same way. Without it a correctly issued token
/// could be refused for being a second "in the future", which presents as an
/// intermittent sign-in failure with no reproduction.
const CLOCK_SKEW_LEEWAY_SECONDS: i64 = 60;

/// Why a token was not accepted.
///
/// A closed set, and deliberately coarse where it faces the caller: telling an
/// unauthenticated caller *which* check failed helps someone probing far more
/// than it helps someone with a valid token. The distinction is kept here for
/// the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// No `Authorization` header, or not a `Bearer` one.
    #[error("no bearer token was presented")]
    Missing,
    /// Structurally not a JWT, or not signed with RS256, or `kid` unknown.
    #[error("the token is malformed or signed by an unknown key")]
    Malformed,
    /// Signature, issuer, audience, or timing check failed.
    #[error("the token did not verify")]
    Invalid,
    /// The token verified but has expired.
    #[error("the token has expired")]
    Expired,
    /// Google's key set could not be fetched, so nothing can be verified.
    ///
    /// Distinct because it is *our* fault, not the caller's: it must answer
    /// `503`, not `401`, or a client would sign out a perfectly valid user
    /// because our dependency was briefly unreachable.
    #[error("the signing keys are unavailable")]
    KeysUnavailable,
}

impl AuthError {
    /// Stable, low-cardinality label for logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Malformed => "malformed",
            Self::Invalid => "invalid",
            Self::Expired => "expired",
            Self::KeysUnavailable => "keys_unavailable",
        }
    }
}

/// The claims RepoLens reads. Everything else Firebase sends is ignored.
///
/// All three are non-`Option`, which is what makes them **required**: a token
/// missing any one fails to deserialize and is rejected. `Validation` cannot
/// require `iat` or `auth_time` — it does not model them — so absence has to be
/// enforced by the shape of this struct rather than by configuration.
#[derive(Debug, Deserialize)]
struct Claims {
    /// Firebase user id. Becomes [`AuthenticatedUser::uid`].
    sub: String,
    /// When the token was issued. Must not be in the future.
    iat: i64,
    /// When the user actually authenticated. Must not be in the future.
    ///
    /// Distinct from `iat`: a refreshed token carries a *new* `iat` and the
    /// *original* `auth_time`, so the two answer different questions and a
    /// future value in either is a token that was not honestly minted.
    auth_time: i64,
}

/// One RSA key from Google's JWK set.
#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

#[derive(Debug, Deserialize)]
struct JwkSet {
    keys: Vec<Jwk>,
}

/// A verified caller.
///
/// Holds the Firebase uid and nothing else. Email, display name and photo are
/// deliberately absent: this slice authenticates a request, it does not build a
/// user profile, and a field nothing reads is a field that leaks into a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Firebase user id, stable for the account.
    pub uid: String,
}

/// Cached decoding keys and when they stop being trusted.
///
/// An expiry rather than a fetch instant, because the lifetime is Google's to
/// state: it is derived from the `Cache-Control: max-age` served with the key
/// set, bounded by [`MIN_KEY_CACHE_TTL`] and [`MAX_KEY_CACHE_TTL`].
struct CachedKeys {
    keys: HashMap<String, DecodingKey>,
    expires_at: Instant,
}

impl CachedKeys {
    fn is_fresh(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

/// Verifies Firebase ID tokens for one project.
pub struct FirebaseVerifier {
    project_id: String,
    /// Where the key set is fetched from. A field rather than a constant so a
    /// test can point it at a mock and count the requests that actually leave.
    jwk_url: String,
    http: reqwest::Client,
    cache: RwLock<Option<CachedKeys>>,
    /// Held across a refresh so concurrent misses collapse into one fetch.
    ///
    /// Separate from the `RwLock` on purpose: the read lock must not be held
    /// across an await, and a write lock held for the duration of an HTTP
    /// request would block every reader on the network.
    refresh: tokio::sync::Mutex<()>,
}

impl FirebaseVerifier {
    /// Builds a verifier that fetches Google's keys on demand.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new(project_id: impl Into<String>) -> Result<Self, reqwest::Error> {
        Self::with_jwk_url(project_id, GOOGLE_JWK_URL)
    }

    /// Builds a verifier that fetches its keys from `jwk_url`.
    ///
    /// Public so a test can serve a key set it controls, rotate it, and count
    /// the requests. `https_only` is relaxed here because a local mock speaks
    /// plain HTTP; [`new`](Self::new) is what deployments call and it keeps the
    /// restriction.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn with_jwk_url(
        project_id: impl Into<String>,
        jwk_url: impl Into<String>,
    ) -> Result<Self, reqwest::Error> {
        let url = jwk_url.into();
        let https_only = url.starts_with("https://");
        Ok(Self {
            project_id: project_id.into(),
            jwk_url: url,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .https_only(https_only)
                .build()?,
            cache: RwLock::new(None),
            refresh: tokio::sync::Mutex::new(()),
        })
    }

    /// Builds a verifier over a fixed key set that never expires.
    ///
    /// Seeds the ordinary cache rather than living in a separate field, so the
    /// tests that use it exercise the real lookup path — including the rule
    /// that a fresh cache answers an unknown `kid` by itself. A verifier built
    /// this way never reaches the network, because the cache never expires.
    #[must_use]
    pub fn with_keys(project_id: impl Into<String>, keys: HashMap<String, DecodingKey>) -> Self {
        let verifier = Self::with_jwk_url(project_id, GOOGLE_JWK_URL)
            .expect("the default client is always constructible");
        *verifier
            .cache
            .try_write()
            .expect("a freshly built verifier is uncontended") = Some(CachedKeys {
            keys,
            expires_at: Instant::now() + MAX_KEY_CACHE_TTL,
        });
        verifier
    }

    /// How much longer the cached key set stays trusted, if one is cached.
    ///
    /// Exists so a test can prove the lifetime is **Google's** rather than one
    /// this process invented. Every other observable — whether a refetch
    /// happens — is indistinguishable inside a test, because the floor on the
    /// cached lifetime is five minutes and no suite should sleep that long.
    /// Without this, replacing the served `max-age` with a fixed default breaks
    /// no assertion, which is exactly what a tamper check found.
    pub async fn cache_lifetime(&self) -> Option<Duration> {
        let cache = self.cache.read().await;
        cache
            .as_ref()
            .map(|cached| cached.expires_at.saturating_duration_since(Instant::now()))
    }

    /// The project this verifier accepts tokens for.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Verifies a bearer token and returns who presented it.
    ///
    /// # Errors
    ///
    /// Returns the reason the token was not accepted. `KeysUnavailable` means
    /// the failure was ours.
    pub async fn verify(&self, token: &str) -> Result<AuthenticatedUser, AuthError> {
        let header = jsonwebtoken::decode_header(token).map_err(|_| AuthError::Malformed)?;
        if header.alg != Algorithm::RS256 {
            // `alg` is attacker-controlled, so it is checked rather than
            // trusted. Accepting whatever it names is the classic JWT
            // confusion attack.
            return Err(AuthError::Malformed);
        }
        let kid = header.kid.ok_or(AuthError::Malformed)?;

        let key = self.decoding_key(&kid).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.project_id]);
        validation.set_issuer(&[format!(
            "https://securetoken.google.com/{}",
            self.project_id
        )]);
        validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);

        let decoded = jsonwebtoken::decode::<Claims>(token, &key, &validation).map_err(
            |error| match error.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
                _ => AuthError::Invalid,
            },
        )?;

        if decoded.claims.sub.is_empty() {
            return Err(AuthError::Invalid);
        }

        // `iat` and `auth_time` must be in the past. Checked here because
        // `jsonwebtoken` validates `exp` and `nbf` and stops there.
        //
        // The same leeway `Validation` applies to `exp` is applied here, and for
        // the same reason: the two clocks are not the same clock, and rejecting
        // a token whose issuer is a second ahead of us would be a flaky sign-in
        // that nobody could reproduce.
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let ceiling = now.saturating_add(CLOCK_SKEW_LEEWAY_SECONDS);
        if decoded.claims.iat > ceiling || decoded.claims.auth_time > ceiling {
            return Err(AuthError::Invalid);
        }

        Ok(AuthenticatedUser {
            uid: decoded.claims.sub,
        })
    }

    /// Returns the decoding key for `kid`, refreshing the set if needed.
    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
        // A fresh cache is the whole answer, including when it says no.
        //
        // This used to fall through to a fetch when the cache was fresh but the
        // `kid` was unknown, which made the network reachable from an
        // unauthenticated request: any syntactically valid JWT with a new
        // random `kid` forced one outbound HTTPS call, and a stream of them
        // forced a stream of calls. An unknown `kid` against keys Google
        // published minutes ago is a bad token, not evidence of rotation, so it
        // is now rejected without leaving the process.
        if let Some(key) = self.cached_key(kid).await? {
            return Ok(key);
        }

        // Only a missing or expired cache reaches here. One refresh at a time:
        // without this, N concurrent verifications after an expiry all fetch,
        // which is a stampede against the same endpoint the check above exists
        // to protect.
        let _refreshing = self.refresh.lock().await;

        // Re-check under the lock. Whoever held it before us has very likely
        // just refreshed, and serving their result is the point of waiting.
        if let Some(key) = self.cached_key(kid).await? {
            return Ok(key);
        }

        let (keys, ttl) = self.fetch_keys().await?;
        let key = keys.get(kid).cloned();

        *self.cache.write().await = Some(CachedKeys {
            keys,
            expires_at: Instant::now() + ttl,
        });

        key.ok_or(AuthError::Malformed)
    }

    /// Looks `kid` up in the cache, if the cache is still trusted.
    ///
    /// Returns `Ok(None)` only when there is nothing usable to consult — no
    /// cache, or an expired one. A **fresh** cache that lacks the key returns
    /// `Err(Malformed)`, which is what keeps an unknown `kid` off the network.
    async fn cached_key(&self, kid: &str) -> Result<Option<DecodingKey>, AuthError> {
        let cache = self.cache.read().await;
        let Some(cached) = cache.as_ref() else {
            return Ok(None);
        };
        if !cached.is_fresh() {
            return Ok(None);
        }
        cached
            .keys
            .get(kid)
            .cloned()
            .map(Some)
            .ok_or(AuthError::Malformed)
    }

    /// Fetches and parses Google's current key set.
    async fn fetch_keys(&self) -> Result<(HashMap<String, DecodingKey>, Duration), AuthError> {
        let response = self.http.get(&self.jwk_url).send().await.map_err(|error| {
            // Category only. A `reqwest` error renders the URL, and this
            // one runs on every cold verification.
            tracing::warn!(
                transport = if error.is_timeout() {
                    "timeout"
                } else {
                    "request"
                },
                "could not fetch Google's signing keys"
            );
            AuthError::KeysUnavailable
        })?;

        if !response.status().is_success() {
            tracing::warn!(
                status = response.status().as_u16(),
                "Google's key endpoint answered with an unexpected status"
            );
            return Err(AuthError::KeysUnavailable);
        }

        // Read before the body is consumed. Google states how long its key set
        // may be reused; honouring that is what keeps a rotation visible
        // instead of pinned behind an interval we invented.
        let ttl = max_age(response.headers());

        let body = response
            .bytes()
            .await
            .map_err(|_| AuthError::KeysUnavailable)?;
        if body.len() > MAX_JWK_BYTES {
            tracing::warn!("Google's key set exceeded the accepted size");
            return Err(AuthError::KeysUnavailable);
        }

        let set: JwkSet = serde_json::from_slice(&body).map_err(|_| AuthError::KeysUnavailable)?;

        let keys: HashMap<String, DecodingKey> = set
            .keys
            .iter()
            .filter_map(|jwk| {
                DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                    .ok()
                    .map(|key| (jwk.kid.clone(), key))
            })
            .collect();

        if keys.is_empty() {
            tracing::warn!("Google's key set contained no usable RSA keys");
            return Err(AuthError::KeysUnavailable);
        }

        Ok((keys, ttl))
    }
}

/// The `Cache-Control: max-age` of a key-set response, clamped.
///
/// Clamped rather than trusted: a zero or near-zero `max-age` would make every
/// verification fetch — the same exhaustion the cache prevents, arriving from
/// upstream instead of from a caller — and an enormous one would pin a retired
/// key set. An absent or unparseable header falls back to
/// [`DEFAULT_KEY_CACHE_TTL`].
fn max_age(headers: &reqwest::header::HeaderMap) -> Duration {
    let served = headers
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(',')
                .filter_map(|directive| {
                    directive
                        .trim()
                        .strip_prefix("max-age=")
                        .or_else(|| directive.trim().strip_prefix("s-maxage="))
                })
                .find_map(|seconds| seconds.trim().parse::<u64>().ok())
        })
        .map(Duration::from_secs);

    served.map_or(DEFAULT_KEY_CACHE_TTL, |ttl| {
        ttl.clamp(MIN_KEY_CACHE_TTL, MAX_KEY_CACHE_TTL)
    })
}

impl std::fmt::Debug for FirebaseVerifier {
    /// Names the project and nothing else. There is no secret here, but a
    /// `Debug` that dumped key material into a log would be a surprise worth
    /// preventing structurally.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirebaseVerifier")
            .field("project_id", &self.project_id)
            .finish_non_exhaustive()
    }
}

/// Extracts a bearer token from an `Authorization` header value.
///
/// The scheme is compared case-insensitively, which RFC 9110 requires, and the
/// token must be non-empty — `Bearer ` with nothing after it is a missing
/// credential, not an invalid one.
#[must_use]
pub fn bearer_token(header: Option<&str>) -> Option<&str> {
    let value = header?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Shared handle, because the verifier holds a key cache worth sharing.
pub type SharedVerifier = Arc<FirebaseVerifier>;

#[cfg(test)]
mod tests {
    use super::{AuthError, FirebaseVerifier, bearer_token};

    #[test]
    fn a_bearer_token_is_recognised_however_it_is_cased() {
        assert_eq!(bearer_token(Some("Bearer abc")), Some("abc"));
        assert_eq!(bearer_token(Some("bearer abc")), Some("abc"));
        assert_eq!(bearer_token(Some("BEARER abc")), Some("abc"));
    }

    #[test]
    fn anything_that_is_not_a_bearer_token_is_absent_rather_than_invalid() {
        // The distinction matters: absent means "sign in", invalid means "your
        // session is broken", and a UI does different things with each.
        for header in [
            None,
            Some(""),
            Some("Bearer"),
            Some("Bearer "),
            Some("Basic abc"),
        ] {
            assert_eq!(bearer_token(header), None, "should reject {header:?}");
        }
    }

    #[tokio::test]
    async fn a_token_that_is_not_a_jwt_is_malformed() {
        let verifier =
            FirebaseVerifier::with_keys("demo-project", std::collections::HashMap::new());
        assert_eq!(
            verifier.verify("not-a-jwt").await.unwrap_err(),
            AuthError::Malformed
        );
    }

    #[test]
    fn the_verifier_debug_impl_names_only_the_project() {
        let verifier =
            FirebaseVerifier::with_keys("demo-project", std::collections::HashMap::new());
        let rendered = format!("{verifier:?}");
        assert!(rendered.contains("demo-project"));
        assert!(!rendered.contains("DecodingKey"));
    }
}
