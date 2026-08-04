//! The GitHub REST boundary.
//!
//! RepoLens ingests over REST rather than GraphQL because its subject is the
//! immutable Git object graph, not GitHub's social graph. The decisive property
//! is that the recursive tree endpoint reports `truncated: true` explicitly:
//! bounded traversal becomes *honest*, which the evidence contract requires.
//! GraphQL's node-complexity limits and partial-result behaviour would make
//! "we could not see everything" much harder to report truthfully.
//!
//! Two ingestion modes live behind one trait and must not be conflated:
//!
//! * tree and selected blobs — canonical evidence;
//! * the archive — **ephemeral transport for line counting only**, never
//!   canonical evidence. Identity stays owner + repository + commit SHA + tree
//!   SHA. The tarball's own hash is deliberately not part of it: GitHub does
//!   not guarantee archive bytes are stable over time even for a fixed commit.
//!
//! Line *counting* is not GitHub-specific and does not live here; see
//! [`repolens_core::composition`].

use std::future::Future;
use std::path::Path;

use repolens_core::{CommitSha, RepositoryCoordinate};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// The REST API version sent as `X-GitHub-Api-Version` on **every** request.
///
/// Exact-pinned rather than tracked, because the API version is part of the
/// reproducibility contract: it changes what the analyzer sees.
///
/// Omitting the header does not mean "latest" — GitHub defaults an absent
/// header to the older `2022-11-28`, so relying on the implicit default would
/// silently pin us to a different version than the one we tested against.
/// Previous versions stay supported for at least 24 months after a successor
/// ships, so this is a deliberate upgrade, never an incidental one.
pub const GITHUB_REST_API_VERSION: &str = "2026-03-10";

/// Repository metadata needed before an analysis can start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRepository {
    /// Canonical coordinate after any owner/name redirect GitHub applied.
    pub coordinate: RepositoryCoordinate,
    /// Branch used when the submitter gave no explicit reference.
    pub default_branch: String,
    /// Archived repositories are analyzable but must be labelled as such.
    pub archived: bool,
    /// GitHub's reported size, used to reject work before downloading it.
    pub size_kilobytes: u64,
}

/// A reference resolved to an exact, immutable commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCommit {
    /// The commit itself.
    pub sha: CommitSha,
    /// Root tree of that commit. Part of canonical identity alongside `sha`.
    pub tree_sha: String,
    /// Commit timestamp, displayed so a reader can see how current the
    /// analyzed state is.
    pub committed_at: OffsetDateTime,
}

/// What a tree entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TreeEntryKind {
    /// A file.
    Blob,
    /// A directory.
    Tree,
    /// A submodule, whose contents are not part of this repository.
    Submodule,
}

/// One entry from a recursive tree listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// Repository-relative path.
    pub path: String,
    /// Git object name for the entry.
    pub sha: String,
    /// Entry kind.
    pub kind: TreeEntryKind,
    /// Size in bytes; absent for trees and submodules.
    pub size_bytes: Option<u64>,
}

/// A recursive tree listing, with GitHub's truncation flag preserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryTree {
    /// Root tree SHA this listing describes.
    pub sha: String,
    /// Entries returned.
    pub entries: Vec<TreeEntry>,
    /// `true` when GitHub could not return the whole tree.
    ///
    /// Must be propagated into the report as a limitation. Dropping it would
    /// turn "we did not see everything" into a false claim of completeness,
    /// which is the exact failure the evidence contract exists to prevent.
    pub truncated: bool,
}

/// The contents of one selected blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobContent {
    /// Repository-relative path the blob was requested for.
    pub path: String,
    /// Git object name of the blob.
    pub sha: String,
    /// Raw bytes. Bounded by the per-blob cap the implementation applies at
    /// fetch time, not by the caller after the fact.
    pub bytes: Vec<u8>,
}

/// Result of streaming an archive to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveDownload {
    /// Compressed bytes actually written, for the extraction budget.
    pub compressed_bytes: u64,
}

/// Failures at the GitHub boundary.
///
/// **PROVISIONAL.** The stable machine-readable error codes the API exposes to
/// clients are owned by the `analysis-v1` fixtures (issue #14); this enum is
/// the internal taxonomy those codes will be mapped from.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GitHubSourceError {
    /// Absent, private, or otherwise invisible to the analysis token.
    #[error("repository {0} was not found or is not publicly accessible")]
    RepositoryNotFound(RepositoryCoordinate),
    /// A reference did not resolve to a commit.
    #[error("reference `{0}` did not resolve to a commit")]
    ReferenceNotFound(String),
    /// Rate limit exhausted. `retry_after_seconds` comes from GitHub's own
    /// headers when present, so back-off is measured rather than guessed.
    #[error("GitHub rate limit exhausted")]
    RateLimited {
        /// Seconds to wait before retrying, if GitHub said.
        retry_after_seconds: Option<u64>,
    },
    /// A configured ceiling was hit. Both numbers are recorded so the report
    /// can state the limit and the observed value rather than only failing.
    #[error("{limit_name} exceeded: observed {observed}, limit {limit}")]
    LimitExceeded {
        /// Which control tripped.
        limit_name: &'static str,
        /// Configured ceiling.
        limit: u64,
        /// Value that exceeded it.
        observed: u64,
    },
    /// Transport, TLS, or protocol failure.
    #[error("transport failure: {0}")]
    Transport(String),
}

/// Everything RepoLens is allowed to ask GitHub for.
///
/// Implemented over `reqwest` at issue #4 rather than an SDK, because the
/// controls that matter here are exactly what an SDK abstracts away: the
/// version header on every request, rate-limit header parsing, request budgets,
/// bounded streaming, response-size caps, and — as a security control — an
/// explicit redirect policy that must not forward `Authorization` across hosts
/// when the archive endpoint redirects. That last one is asserted by a test,
/// not assumed from library defaults.
pub trait GitHubRepositorySource {
    /// Confirms the repository exists, is public, and is small enough to try.
    fn resolve_repository(
        &self,
        coordinate: &RepositoryCoordinate,
    ) -> impl Future<Output = Result<ResolvedRepository, GitHubSourceError>> + Send;

    /// Resolves a branch, tag, or SHA to an exact commit and its root tree.
    fn resolve_commit(
        &self,
        coordinate: &RepositoryCoordinate,
        reference: &str,
    ) -> impl Future<Output = Result<ResolvedCommit, GitHubSourceError>> + Send;

    /// Lists the tree recursively, preserving GitHub's truncation flag.
    fn fetch_tree(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
    ) -> impl Future<Output = Result<RepositoryTree, GitHubSourceError>> + Send;

    /// Fetches a bounded, explicitly chosen set of blobs — never the whole
    /// repository. This is where semantic and architectural evidence comes
    /// from.
    fn fetch_selected_blobs(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
        paths: &[String],
    ) -> impl Future<Output = Result<Vec<BlobContent>, GitHubSourceError>> + Send;

    /// Streams the commit archive to `destination`, refusing to write more than
    /// `max_compressed_bytes`.
    ///
    /// Streamed to a path rather than returned in memory: extraction happens on
    /// a size-limited volume so that exceeding the budget is a catchable error
    /// instead of an out-of-memory kill that strands a worker lease.
    fn download_archive(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
        max_compressed_bytes: u64,
        destination: &Path,
    ) -> impl Future<Output = Result<ArchiveDownload, GitHubSourceError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::GITHUB_REST_API_VERSION;

    /// The implicit default GitHub applies when the header is absent.
    const IMPLICIT_DEFAULT: &str = "2022-11-28";

    #[test]
    fn rest_api_version_is_pinned() {
        assert_eq!(GITHUB_REST_API_VERSION, "2026-03-10");
    }

    #[test]
    fn rest_api_version_is_not_the_implicit_default() {
        assert_ne!(
            GITHUB_REST_API_VERSION, IMPLICIT_DEFAULT,
            "sending the version GitHub already defaults to would make the \
             header pointless and hide an accidental downgrade"
        );
    }
}
