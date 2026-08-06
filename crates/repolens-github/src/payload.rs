//! GitHub's JSON, mirrored exactly and kept inside this crate.
//!
//! Nothing here is `pub`. These structs are the shape of somebody else's API,
//! and the moment one escapes, a field GitHub renames becomes a change to
//! RepoLens' own contract. The boundary's job is to translate: everything in
//! this module is consumed by [`crate::rest`] and converted into a domain type
//! before it is returned.
//!
//! Only the fields RepoLens actually reads are declared. `serde` ignores the
//! rest, which is what makes a new field in a GitHub response a non-event
//! instead of a deserialization failure.

use serde::Deserialize;
use time::OffsetDateTime;

/// `GET /repos/{owner}/{repo}`.
#[derive(Debug, Deserialize)]
pub(crate) struct RepositoryPayload {
    /// `owner/name` **after** any rename GitHub applied. The canonical
    /// coordinate is taken from here rather than from the request, so a moved
    /// repository is analyzed under the name it actually has.
    pub(crate) full_name: String,
    pub(crate) default_branch: String,
    pub(crate) archived: bool,
    /// Whole-repository size in kilobytes, as GitHub counts it.
    pub(crate) size: u64,
    /// Absent for an unauthenticated caller, in which case a visible repository
    /// is public by definition.
    #[serde(default)]
    pub(crate) private: bool,
}

/// `GET /repos/{owner}/{repo}/commits/{ref}`.
#[derive(Debug, Deserialize)]
pub(crate) struct CommitPayload {
    pub(crate) sha: String,
    pub(crate) commit: CommitDetailPayload,
}

/// The Git commit object nested inside a commit response.
#[derive(Debug, Deserialize)]
pub(crate) struct CommitDetailPayload {
    pub(crate) tree: GitObjectPayload,
    /// The *committer* date, not the author date. Rebasing rewrites the former
    /// and preserves the latter, so the committer date is the one that answers
    /// "how current is the state we analyzed?".
    pub(crate) committer: GitActorPayload,
}

/// A bare reference to another Git object.
#[derive(Debug, Deserialize)]
pub(crate) struct GitObjectPayload {
    pub(crate) sha: String,
}

/// The name/email/date triple Git records for an author or committer.
#[derive(Debug, Deserialize)]
pub(crate) struct GitActorPayload {
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) date: OffsetDateTime,
}

/// `GET /repos/{owner}/{repo}/git/trees/{sha}?recursive=1`.
#[derive(Debug, Deserialize)]
pub(crate) struct TreePayload {
    pub(crate) sha: String,
    pub(crate) tree: Vec<TreeEntryPayload>,
    /// GitHub's own admission that it did not return everything. Carried
    /// through to [`crate::RepositoryTree`] unchanged.
    pub(crate) truncated: bool,
}

/// One entry of a recursive tree listing.
#[derive(Debug, Deserialize)]
pub(crate) struct TreeEntryPayload {
    pub(crate) path: String,
    pub(crate) sha: String,
    /// `blob`, `tree`, or `commit`. Named `kind` here because `type` is a
    /// keyword.
    #[serde(rename = "type")]
    pub(crate) kind: String,
    /// Present for blobs only.
    #[serde(default)]
    pub(crate) size: Option<u64>,
}

impl RepositoryPayload {
    /// Splits `full_name` into its owner and name halves.
    ///
    /// Returns `None` for anything that is not exactly one slash, rather than
    /// guessing: a coordinate is the analysis' identity, and half of one
    /// recovered from a malformed string would be an identity nobody chose.
    pub(crate) fn split_full_name(&self) -> Option<(&str, &str)> {
        let (owner, name) = self.full_name.split_once('/')?;
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return None;
        }
        Some((owner, name))
    }
}

#[cfg(test)]
mod tests {
    use super::{RepositoryPayload, TreePayload};

    fn repository(full_name: &str) -> RepositoryPayload {
        RepositoryPayload {
            full_name: full_name.to_owned(),
            default_branch: "main".to_owned(),
            archived: false,
            size: 1,
            private: false,
        }
    }

    #[test]
    fn splits_a_well_formed_full_name() {
        let payload = repository("rust-lang/crates.io");
        assert_eq!(payload.split_full_name(), Some(("rust-lang", "crates.io")));
    }

    #[test]
    fn refuses_to_guess_at_a_malformed_full_name() {
        for full_name in ["", "owner", "/name", "owner/", "owner/name/extra"] {
            assert_eq!(
                repository(full_name).split_full_name(),
                None,
                "should refuse: {full_name:?}"
            );
        }
    }

    #[test]
    fn unknown_fields_do_not_break_deserialization() {
        // GitHub adds fields to responses without warning. If that were a hard
        // failure, ingestion would break on a day nothing in this repository
        // changed.
        let json = r#"{
            "sha": "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
            "url": "https://example.invalid/ignored",
            "tree": [],
            "truncated": false,
            "a_field_invented_next_year": 1
        }"#;
        let parsed: TreePayload = serde_json::from_str(json).expect("unknown fields are ignored");
        assert!(!parsed.truncated);
    }
}
