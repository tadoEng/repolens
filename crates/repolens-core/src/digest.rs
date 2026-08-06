//! Content digests.
//!
//! One canonical spelling, owned here rather than at either end that produces
//! or renders it. The ingestion boundary hashes bytes and the wire contract
//! publishes the result; if each chose its own format, the mismatch would not
//! surface until integration — and would surface as evidence that silently
//! fails to match the commit it claims to pin.
//!
//! The algorithm is part of the value, not an assumption. A bare hex string
//! cannot say which function produced it, so a future move to SHA-512 or
//! BLAKE3 would silently reinterpret every stored digest.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical prefix. Changing this is a wire-format change.
const SHA256_PREFIX: &str = "sha256:";

/// Length of a hex-encoded SHA-256 digest.
const SHA256_HEX_LEN: usize = 64;

/// Why a string could not be accepted as a content digest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentDigestError {
    /// No recognised algorithm prefix.
    #[error("content digest must begin with a known algorithm prefix such as `sha256:`")]
    MissingAlgorithm,
    /// Right prefix, wrong digest length.
    #[error("sha256 digest must be {SHA256_HEX_LEN} hexadecimal characters, got {0}")]
    Length(usize),
    /// Contained something outside `[0-9a-f]`.
    ///
    /// Uppercase is rejected rather than normalised: two spellings of the same
    /// digest would compare unequal as strings, and digests are compared as
    /// strings everywhere they are stored.
    #[error("sha256 digest must be lowercase hexadecimal")]
    NotLowercaseHex,
}

/// A digest of retrieved content, as `sha256:<64 lowercase hex>`.
///
/// This is what makes a piece of evidence checkable: a reader can hash the file
/// at the analyzed commit and compare. It therefore digests the **full** source
/// content, never a truncated excerpt.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContentDigest(String);

impl ContentDigest {
    /// Wraps raw SHA-256 output.
    ///
    /// Takes bytes rather than a hex string so a caller cannot hand over a
    /// digest from some other algorithm and have it labelled `sha256:`.
    #[must_use]
    pub fn from_sha256(digest: [u8; 32]) -> Self {
        let mut value = String::with_capacity(SHA256_PREFIX.len() + SHA256_HEX_LEN);
        value.push_str(SHA256_PREFIX);
        for byte in digest {
            use fmt::Write as _;
            let _ = write!(value, "{byte:02x}");
        }
        Self(value)
    }

    /// Validates a string already in canonical form.
    ///
    /// # Errors
    ///
    /// Returns [`ContentDigestError`] when the prefix, length, or alphabet is
    /// wrong.
    pub fn parse(value: &str) -> Result<Self, ContentDigestError> {
        let hex = value
            .strip_prefix(SHA256_PREFIX)
            .ok_or(ContentDigestError::MissingAlgorithm)?;

        if hex.len() != SHA256_HEX_LEN {
            return Err(ContentDigestError::Length(hex.len()));
        }
        if !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ContentDigestError::NotLowercaseHex);
        }

        Ok(Self(value.to_owned()))
    }

    /// Borrows the canonical string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Regular expression the wire format satisfies, published in the OpenAPI
    /// document so a client can validate without reimplementing the rule.
    pub const PATTERN: &'static str = "^sha256:[0-9a-f]{64}$";
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ContentDigest {
    type Error = ContentDigestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<ContentDigest> for String {
    fn from(value: ContentDigest) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_sha256_output_renders_canonically() {
        let digest = ContentDigest::from_sha256([0xab; 32]);
        assert_eq!(digest.as_str(), format!("sha256:{}", "ab".repeat(32)));
        assert!(ContentDigest::parse(digest.as_str()).is_ok());
    }

    #[test]
    fn rejects_a_bare_hex_string() {
        // The exact drift this type exists to prevent: one side emitting bare
        // hex while the other publishes a prefixed form.
        let bare = "a".repeat(64);
        assert_eq!(
            ContentDigest::parse(&bare),
            Err(ContentDigestError::MissingAlgorithm)
        );
    }

    #[test]
    fn rejects_uppercase_rather_than_normalising_it() {
        // Two spellings of one digest would compare unequal as strings, and
        // digests are compared as strings everywhere they are stored.
        let upper = format!("sha256:{}", "A".repeat(64));
        assert_eq!(
            ContentDigest::parse(&upper),
            Err(ContentDigestError::NotLowercaseHex)
        );
    }

    #[test]
    fn rejects_wrong_lengths() {
        assert_eq!(
            ContentDigest::parse("sha256:abc"),
            Err(ContentDigestError::Length(3))
        );
    }

    #[test]
    fn round_trips_through_json() {
        let digest = ContentDigest::from_sha256([0x01; 32]);
        let json = serde_json::to_string(&digest).unwrap();
        assert_eq!(
            serde_json::from_str::<ContentDigest>(&json).unwrap(),
            digest
        );
    }

    #[test]
    fn deserialization_rejects_an_invalid_digest() {
        // The contract must not accept a digest it would later fail to verify.
        assert!(serde_json::from_str::<ContentDigest>("\"not-a-digest\"").is_err());
    }
}
