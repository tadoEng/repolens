//! Repository identity.
//!
//! An analysis is identified by owner, repository name, and an exact commit
//! SHA. Nothing else is canonical — notably not a downloaded archive, whose
//! bytes GitHub does not guarantee to be stable over time even for a fixed
//! commit.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Owner and repository name, as they appear in a GitHub URL path.
///
/// Parsing a user-supplied URL into a coordinate is the ingestion boundary's
/// job (issue #4); this type only carries the result.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RepositoryCoordinate {
    /// User or organization that owns the repository.
    pub owner: String,
    /// Repository name, without the owner prefix.
    pub name: String,
}

impl RepositoryCoordinate {
    /// Builds a coordinate from its two parts.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
        }
    }
}

impl fmt::Display for RepositoryCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// Why a string could not be accepted as a commit SHA.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommitShaError {
    /// Git object names from GitHub's REST API are full 40-character SHA-1
    /// digests. Abbreviations are rejected because they are not stable
    /// identities: an abbreviation that is unique today can collide later.
    #[error("commit SHA must be exactly 40 characters, got {0}")]
    Length(usize),
    /// Contained something outside `[0-9a-fA-F]`.
    #[error("commit SHA contains a non-hexadecimal character")]
    NotHexadecimal,
}

/// A full, validated, lowercase commit SHA.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitSha(String);

impl CommitSha {
    /// Validates and normalizes a commit SHA to lowercase.
    pub fn parse(value: &str) -> Result<Self, CommitShaError> {
        if value.len() != 40 {
            return Err(CommitShaError::Length(value.len()));
        }
        if !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CommitShaError::NotHexadecimal);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// Borrows the normalized SHA.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for CommitSha {
    type Error = CommitShaError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<CommitSha> for String {
    fn from(value: CommitSha) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::{CommitSha, CommitShaError, RepositoryCoordinate};

    const VALID: &str = "0584a2df65968a4e9e6859ef46bbed430408a3f1";

    #[test]
    fn coordinate_displays_as_owner_slash_name() {
        assert_eq!(
            RepositoryCoordinate::new("rust-lang", "crates.io").to_string(),
            "rust-lang/crates.io"
        );
    }

    #[test]
    fn parses_and_normalizes_case() {
        let sha = CommitSha::parse(&VALID.to_ascii_uppercase()).expect("valid sha");
        assert_eq!(sha.as_str(), VALID);
    }

    #[test]
    fn rejects_abbreviated_sha() {
        assert_eq!(CommitSha::parse("0584a2d"), Err(CommitShaError::Length(7)));
    }

    #[test]
    fn rejects_non_hexadecimal() {
        let mut value = VALID.to_owned();
        value.replace_range(0..1, "z");
        assert_eq!(
            CommitSha::parse(&value),
            Err(CommitShaError::NotHexadecimal)
        );
    }
}
