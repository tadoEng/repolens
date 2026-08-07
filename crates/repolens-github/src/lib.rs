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
//!
//! # Layout
//!
//! This module owns the contract — the domain types and the trait. The parts
//! that implement it are deliberately separate and only one of them is public:
//!
//! * [`GitHubRestClient`] — the `reqwest` implementation and its budgets.
//! * [`select_paths`] — which files are worth reading, as a pure function of a
//!   tree, so that two runs of one commit choose the same evidence.
//! * `payload` — `serde` mirrors of GitHub's JSON, private on purpose. If one
//!   escaped, a field GitHub renamed would become a change to RepoLens' own
//!   contract.

pub mod limits;

mod payload;
mod policy;
mod rest;

pub use policy::{BlobSelection, FileSelection, SkipReason, SkippedPath, select_paths};
pub use rest::{GitHubClientConfig, GitHubRestClient};

use std::future::Future;
use std::path::Path;

use repolens_core::{CommitSha, ContentDigest, RepositoryCoordinate, TreeSha};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    ///
    /// A [`TreeSha`] rather than a `String`, which is what makes transposing
    /// the two halves of the identity a compile error rather than a review
    /// question. Both are 40-character SHA-1 digests, so a `String` here would
    /// accept `sha` — the exact substitution that once wrote the commit SHA
    /// into both fields and read as correct because it was a well-formed
    /// digest. Convert to a string only when producing the wire DTO.
    pub tree_sha: TreeSha,
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
    /// The SHA this listing was **requested by**, echoed back by GitHub.
    ///
    /// Not necessarily a tree SHA. The tree endpoint accepts a commit SHA and
    /// resolves it to that commit's root tree, but reports the SHA it was given
    /// rather than the tree it resolved to — so fetching by commit returns the
    /// commit SHA here, for exactly the same entries.
    ///
    /// **Use [`ResolvedCommit::tree_sha`] when the canonical tree SHA is what
    /// you need.** That value comes from the commit object itself and is the
    /// tree either way. Taking this field instead writes the commit SHA into
    /// both halves of the identity, which reads as correct because it is a
    /// well-formed digest.
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
    /// SHA-256 of [`bytes`](BlobContent::bytes), in the contract's canonical
    /// spelling. See [`content_digest`].
    pub content_digest: ContentDigest,
    /// Raw bytes. Bounded by the per-blob cap the implementation applies at
    /// fetch time, not by the caller after the fact.
    pub bytes: Vec<u8>,
}

/// SHA-256 of retrieved bytes, as a [`ContentDigest`].
///
/// Recorded alongside every piece of retrieved evidence so that a finding can be
/// traced to the exact bytes it was drawn from — the point of an evidence-backed
/// report is that a reader can check it, and a citation to a path is only a
/// citation to whatever that path holds today.
///
/// The spelling is `repolens-core`'s rather than this crate's. Ingestion
/// produces digests and the wire contract publishes them; when each owned its
/// own format the mismatch surfaced only at integration, as evidence that
/// silently failed to match the commit it claimed to pin.
///
/// Deliberately not the Git blob SHA, which [`BlobContent::sha`] already
/// carries. That is SHA-1 over a length-prefixed object rather than over the
/// content, so it answers a different question, and SHA-1 is no longer a digest
/// anything should be pinned on. Keeping both means a mismatch between them is
/// itself detectable.
pub fn content_digest(bytes: &[u8]) -> ContentDigest {
    ContentDigest::from_sha256(Sha256::digest(bytes).into())
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
    /// Rate limit exhausted. Both fields come from GitHub's own headers when
    /// present, so back-off is measured rather than guessed.
    ///
    /// The message names no URL and no header value, because this error is the
    /// one most likely to be logged in bulk.
    #[error("GitHub rate limit exhausted")]
    RateLimited {
        /// Seconds to wait before retrying, if GitHub said. Sent with the
        /// secondary rate limit; usually absent for the primary one.
        retry_after_seconds: Option<u64>,
        /// Instant the current window resets, from `x-ratelimit-reset`.
        ///
        /// An instant rather than a remaining duration on purpose. Converting
        /// here would need a clock read at the point of failure, and the result
        /// would already be stale by the time a queued retry acted on it.
        reset_at: Option<OffsetDateTime>,
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
    /// GitHub answered with a status this boundary does not model. The code is
    /// carried; the body is not, because an unmodelled status is exactly the
    /// case where the body is least likely to be something we should repeat.
    #[error("GitHub answered with an unexpected status: {status}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
    },
    /// A response was not the shape its endpoint promises.
    ///
    /// Names the resource, never the body. `serde_json` quotes the input around
    /// the failure through `Display`, which would copy part of a response into a
    /// log for precisely the class of failure where the response is unexpected.
    #[error("GitHub's {resource} response could not be interpreted")]
    MalformedResponse {
        /// Which response disappointed: `repository`, `commit`, `tree`, ….
        resource: &'static str,
    },
    /// The configured REST base is not one this boundary will send a request —
    /// or a credential — to. Raised once at construction rather than on every
    /// request.
    ///
    /// Names the rule that was broken, never the URL: a base is exactly the
    /// place a credential can be embedded, and an error that echoed it would
    /// print that credential into a log.
    #[error("the configured API base is unusable: {reason}")]
    InvalidApiBase {
        /// Which rule the base broke, as a fixed phrase.
        reason: &'static str,
    },
    /// A redirect pointed somewhere this boundary will not follow.
    ///
    /// Separate from [`MalformedResponse`](GitHubSourceError::MalformedResponse)
    /// because the response was well-formed and the refusal was ours: following
    /// it would have carried the analysis — and the evidence it collects — onto
    /// a transport that anyone on the path can read or rewrite.
    ///
    /// Names nothing about the target, which for the archive endpoint carries a
    /// signed query string.
    #[error("a redirect onto an insecure transport was refused")]
    InsecureRedirect,
    /// Writing retrieved bytes to disk failed.
    ///
    /// The path is named by the caller and is not rendered here.
    #[error("could not {operation}")]
    Io {
        /// What was being attempted, as a fixed phrase.
        operation: &'static str,
    },
    /// Transport, TLS, or protocol failure.
    ///
    /// Carries a category — `timeout`, `connect`, `redirect`, … — never the
    /// underlying message, which renders the request URL.
    #[error("transport failure: {0}")]
    Transport(String),
}

impl GitHubSourceError {
    /// Whether retrying the same request could plausibly succeed later.
    ///
    /// The distinction is what separates a worker that backs off from one that
    /// hammers a closed door: a rate limit and a transport blip pass, while a
    /// missing repository or an exceeded ceiling will fail identically forever
    /// and should be recorded rather than retried.
    ///
    /// [`UnexpectedStatus`](GitHubSourceError::UnexpectedStatus) is retryable
    /// only for server-side codes. A `4xx` that reached here was not a rate
    /// limit and not a `404`, so it is a refusal that waiting cannot improve.
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::RateLimited { .. } | Self::Transport(_) => true,
            Self::UnexpectedStatus { status } => *status >= 500,
            Self::RepositoryNotFound(_)
            | Self::ReferenceNotFound(_)
            | Self::LimitExceeded { .. }
            | Self::MalformedResponse { .. }
            | Self::InvalidApiBase { .. }
            | Self::InsecureRedirect
            | Self::Io { .. } => false,
        }
    }
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
    use repolens_core::ContentDigest;

    use super::{GITHUB_REST_API_VERSION, GitHubSourceError, content_digest};

    /// The implicit default GitHub applies when the header is absent.
    const IMPLICIT_DEFAULT: &str = "2022-11-28";

    #[test]
    fn identical_content_digests_identically() {
        // The property the reproducibility contract rests on: the same commit
        // read twice yields the same evidence identity. If the digest depended
        // on anything but the bytes, "we analyzed the same thing" would be
        // unverifiable.
        let first = content_digest(b"# RepoLens\n");
        let second = content_digest(b"# RepoLens\n");

        assert_eq!(first, second);
        assert!(ContentDigest::parse(first.as_str()).is_ok());
    }

    #[test]
    fn the_digest_is_the_contracts_spelling_rather_than_bare_hex() {
        // The drift the shared type exists to prevent: this crate emitting bare
        // hex while the wire contract publishes `sha256:<hex>`. Asserted here
        // because the two ends are compiled separately and would otherwise only
        // disagree once a real analysis was published.
        let digest = content_digest(b"# RepoLens\n");

        assert!(digest.as_str().starts_with("sha256:"), "{digest}");
        assert_eq!(digest.as_str().len(), "sha256:".len() + 64);
    }

    #[test]
    fn a_single_changed_byte_changes_the_digest() {
        assert_ne!(
            content_digest(b"# RepoLens\n"),
            content_digest(b"# RepoLens")
        );
    }

    #[test]
    fn the_empty_file_has_a_digest_rather_than_no_digest() {
        // An empty file is evidence too — a zero-byte `LICENSE` is a fact about
        // a repository — so it must be citable like any other.
        assert_eq!(
            content_digest(b"").as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn only_failures_that_can_pass_are_retryable() {
        assert!(
            GitHubSourceError::RateLimited {
                retry_after_seconds: Some(60),
                reset_at: None,
            }
            .is_retryable()
        );
        assert!(GitHubSourceError::UnexpectedStatus { status: 502 }.is_retryable());
        // Waiting will not create a repository, nor shrink one past a ceiling.
        assert!(!GitHubSourceError::UnexpectedStatus { status: 451 }.is_retryable());
        assert!(
            !GitHubSourceError::LimitExceeded {
                limit_name: "tree entries",
                limit: 1,
                observed: 2,
            }
            .is_retryable()
        );
    }

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
