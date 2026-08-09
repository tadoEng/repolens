//! Firebase ID tokens, minted in-process for the suites that need one.
//!
//! Shared by `tests/authentication.rs` and `tests/admin.rs`. Two suites now
//! assert what happens to a credential, and a second copy of this file would be
//! a second definition of "a valid token" — the two would drift, and the way
//! they would drift is one suite quietly proving less than the other while both
//! stayed green.
//!
//! Tokens are minted against a throwaway RSA key and the verifier is handed that
//! key instead of Google's. Recording a real Firebase token would have been
//! simpler and would have expired within the hour, taking the suite with it —
//! and it would have put a live credential in the repository.
//!
//! What that buys is the ability to construct a token failing exactly one check:
//! wrong audience, wrong issuer, expired, signed by a key the verifier does not
//! have, or belonging to a uid nobody allow-listed.
//!
//! Cargo compiles this module separately into each test binary, so a helper used
//! by one suite is dead code in the other. That is what the allow below is for,
//! and it is why it is scoped to this file rather than to a suite.
#![allow(dead_code)]

use std::collections::HashMap;

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header};
use serde::Serialize;

/// The Firebase project these tokens claim to come from.
pub(crate) const PROJECT: &str = "repolens-test-project";

/// The key id the verifier is taught to trust.
pub(crate) const KID: &str = "test-key-1";

/// The uid an ordinary signed-in caller carries.
pub(crate) const CALLER_UID: &str = "firebase-uid-123";

/// The RSA keypair this suite signs with, generated once per test binary.
///
/// Generated rather than committed. `.gitignore` blocks `*.pem` and `*.key` on
/// purpose, and carving an exception for a fixture would weaken a net that
/// exists to catch the real thing — a key that never touches disk cannot be
/// mistaken for a credential by a scanner or by the next reader.
///
/// Returned as DER plus the public modulus and exponent, which is deliberately
/// the same shape the verifier consumes in production: Google publishes JWKs,
/// so `DecodingKey::from_rsa_raw_components` is the path that actually ships
/// and therefore the path worth exercising.
///
/// 2048 bits, matching Google, paid once per binary.
pub(crate) struct TestKey {
    pub(crate) private_der: Vec<u8>,
    pub(crate) modulus: Vec<u8>,
    pub(crate) exponent: Vec<u8>,
}

pub(crate) fn key() -> &'static TestKey {
    use rsa::pkcs1::EncodeRsaPrivateKey as _;
    use rsa::traits::PublicKeyParts as _;
    use rsa::{RsaPrivateKey, RsaPublicKey};

    static KEY: std::sync::OnceLock<TestKey> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("a keypair is generated");
        let public = RsaPublicKey::from(&private);
        TestKey {
            private_der: private
                .to_pkcs1_der()
                .expect("the private key encodes")
                .as_bytes()
                .to_vec(),
            modulus: public.n().to_bytes_be(),
            exponent: public.e().to_bytes_be(),
        }
    })
}

/// The key set a verifier is built over, trusting [`KID`] and nothing else.
pub(crate) fn decoding_keys() -> HashMap<String, DecodingKey> {
    let mut keys = HashMap::new();
    keys.insert(
        KID.to_owned(),
        DecodingKey::from_rsa_raw_components(&key().modulus, &key().exponent),
    );
    keys
}

/// Firebase ID token claims, as far as this API reads them.
///
/// Every field is `Option` with `skip_serializing_if` so a test can mint a token
/// that **omits** a claim, not merely one that carries a wrong value. Required
/// and valid are different rules and each needs its own token.
#[derive(Serialize)]
pub(crate) struct TestClaims {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) iss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) iat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exp: Option<i64>,
}

pub(crate) fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// A token that passes every check, for the caller named by `uid`.
pub(crate) fn claims_for(uid: &str) -> TestClaims {
    TestClaims {
        sub: Some(uid.to_owned()),
        aud: Some(PROJECT.to_owned()),
        iss: Some(format!("https://securetoken.google.com/{PROJECT}")),
        iat: Some(now() - 60),
        auth_time: Some(now() - 120),
        exp: Some(now() + 3600),
    }
}

/// A token that passes every check, for an ordinary signed-in caller.
pub(crate) fn valid_claims() -> TestClaims {
    claims_for(CALLER_UID)
}

/// Signs a token with the fixture key.
pub(crate) fn mint(claims: &TestClaims, kid: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    let signing = EncodingKey::from_rsa_der(&key().private_der);
    jsonwebtoken::encode(&header, claims, &signing).expect("the token signs")
}
