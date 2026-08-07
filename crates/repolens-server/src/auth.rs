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

/// How long a fetched key set is reused before being refetched.
///
/// Google rotates these keys and serves a `Cache-Control` max-age, but a fixed
/// conservative ceiling is enough here and has one fewer failure mode than
/// parsing the header. Rotation is not abrupt: retired keys keep verifying
/// outstanding tokens for their lifetime, so a stale set fails closed by
/// missing a `kid` rather than by accepting anything.
const KEY_CACHE_TTL: Duration = Duration::from_hours(1);

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

/// Cached decoding keys and when they were fetched.
struct CachedKeys {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

/// Verifies Firebase ID tokens for one project.
pub struct FirebaseVerifier {
    project_id: String,
    http: reqwest::Client,
    cache: RwLock<Option<CachedKeys>>,
    /// Fixed key set, used by tests instead of the network.
    ///
    /// `None` in every deployed configuration. Its presence is what lets the
    /// verification rules be tested against tokens this process mints itself,
    /// rather than against a recorded token that would expire and take the
    /// suite with it.
    fixed_keys: Option<HashMap<String, DecodingKey>>,
}

impl FirebaseVerifier {
    /// Builds a verifier that fetches Google's keys on demand.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new(project_id: impl Into<String>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            project_id: project_id.into(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .https_only(true)
                .build()?,
            cache: RwLock::new(None),
            fixed_keys: None,
        })
    }

    /// Builds a verifier over a fixed key set. Tests only.
    #[must_use]
    pub fn with_keys(project_id: impl Into<String>, keys: HashMap<String, DecodingKey>) -> Self {
        Self {
            project_id: project_id.into(),
            http: reqwest::Client::new(),
            cache: RwLock::new(None),
            fixed_keys: Some(keys),
        }
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
        if let Some(fixed) = &self.fixed_keys {
            return fixed.get(kid).cloned().ok_or(AuthError::Malformed);
        }

        // Fast path: a fresh cached set that already has the key.
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref()
                && cached.fetched_at.elapsed() < KEY_CACHE_TTL
                && let Some(key) = cached.keys.get(kid)
            {
                return Ok(key.clone());
            }
        }

        // Refetch. An unknown `kid` on a fresh set is a bad token, but an
        // unknown `kid` on a *stale* set is very likely rotation, and telling
        // those apart is worth one request.
        let fetched = self.fetch_keys().await?;
        let key = fetched.get(kid).cloned();

        *self.cache.write().await = Some(CachedKeys {
            keys: fetched,
            fetched_at: Instant::now(),
        });

        key.ok_or(AuthError::Malformed)
    }

    /// Fetches and parses Google's current key set.
    async fn fetch_keys(&self) -> Result<HashMap<String, DecodingKey>, AuthError> {
        let response = self
            .http
            .get(GOOGLE_JWK_URL)
            .send()
            .await
            .map_err(|error| {
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

        Ok(keys)
    }
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
