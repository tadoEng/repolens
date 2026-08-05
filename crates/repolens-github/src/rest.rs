//! The `reqwest` implementation of [`GitHubRepositorySource`].
//!
//! Written against the HTTP client directly rather than an SDK, because every
//! control this boundary is accountable for is one an SDK would hide: the
//! version header on every request, rate-limit headers read rather than
//! retried-through, an explicit redirect policy, response-size caps, and a
//! streamed archive that never lands in memory.
//!
//! Three rules hold everywhere in this file.
//!
//! **The credential never leaves its origin.** It lives in a
//! [`SecretString`] and is attached per request, only when the target's origin
//! is the one the client was configured for. Section 3 of the amendment to issue
//! #4 makes this a security control rather than a default to inherit, so
//! redirects are followed by hand and the behaviour is asserted by a test that
//! fails if `reqwest` ever changes its mind.
//!
//! **No error renders a URL, a token, or a response body.** `reqwest::Error`
//! prints the request URL through `Display` and `serde_json::Error` prints the
//! text it choked on; both are dropped in favour of a closed set of categories,
//! following the `ConfigError` precedent in `repolens-server`.
//!
//! **GitHub's JSON stays in [`crate::payload`].** What crosses this boundary is
//! a RepoLens domain type, so a field GitHub renames is a change here and
//! nowhere else.

use std::fmt;
use std::path::Path;

use futures_util::StreamExt;
use reqwest::header::{self, HeaderMap};
use reqwest::{Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use url::Url;

use repolens_core::{CommitSha, RepositoryCoordinate, TreeSha};

use crate::payload::{CommitPayload, RepositoryPayload, TreePayload};
use crate::policy::{BlobSelection, SkipReason, SkippedPath};
use crate::{
    ArchiveDownload, BlobContent, GITHUB_REST_API_VERSION, GitHubRepositorySource,
    GitHubSourceError, RepositoryTree, ResolvedCommit, ResolvedRepository, TreeEntry,
    TreeEntryKind, content_digest, limits,
};

/// GitHub's public REST entry point.
const DEFAULT_API_BASE: &str = "https://api.github.com/";

/// Carries [`GITHUB_REST_API_VERSION`]. Absent, GitHub falls back to an *older*
/// version rather than the newest, so this header is never optional.
const API_VERSION_HEADER: &str = "x-github-api-version";

/// Requests remaining in the current rate-limit window.
const RATE_LIMIT_REMAINING_HEADER: &str = "x-ratelimit-remaining";

/// Unix timestamp at which the current rate-limit window resets.
const RATE_LIMIT_RESET_HEADER: &str = "x-ratelimit-reset";

/// Seconds to wait, sent with GitHub's secondary rate limit.
const RETRY_AFTER_HEADER: &str = "retry-after";

/// GitHub's versioned JSON media type.
const ACCEPT_JSON: &str = "application/vnd.github+json";

/// Raw blob bytes.
///
/// The alternative is the default JSON representation, which is base64 and would
/// mean carrying a base64 decoder to undo an encoding we never wanted, plus a
/// third more bytes over the wire for every file.
const ACCEPT_RAW: &str = "application/vnd.github.raw";

/// The archive is a tarball; there is nothing to negotiate.
const ACCEPT_ANY: &str = "*/*";

/// GitHub rejects requests without a `User-Agent`.
const USER_AGENT_VALUE: &str = concat!("repolens/", env!("CARGO_PKG_VERSION"));

/// Whether a request may negotiate a compressed transfer encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transfer {
    /// Let the client negotiate. The size cap then bounds *decoded* bytes, which
    /// is the figure that bounds memory, and that is what a JSON response needs.
    Negotiated,
    /// Ask for the bytes exactly as stored. The archive's budget is denominated
    /// in compressed bytes, so a transparently decompressed response would make
    /// the number reported to the extractor describe something else entirely.
    Identity,
}

/// How to reach GitHub, and with what credential.
///
/// Reads no environment variable of its own. Configuration policy — which
/// variable, whether it is required, how it is reported when absent — belongs to
/// the process that has a `config` module, and duplicating it here would create
/// a second place for it to disagree with itself.
pub struct GitHubClientConfig {
    api_base: Url,
    token: Option<SecretString>,
}

impl GitHubClientConfig {
    /// GitHub's public API, with no credential.
    ///
    /// Unauthenticated ingestion works and is rate-limited to roughly sixty
    /// requests an hour, which is enough to prove the path and not enough to run
    /// on. A token raises the ceiling; it never widens what is visible, because
    /// only public repositories are analyzed.
    pub fn new() -> Self {
        Self {
            api_base: Url::parse(DEFAULT_API_BASE).expect("the default API base is a literal"),
            token: None,
        }
    }

    /// Points the client at a different REST root.
    ///
    /// Exists for tests against a local mock. It is also the definition of the
    /// credentialed origin: a token configured alongside a base is scoped to
    /// that base and is sent nowhere else.
    #[must_use]
    pub fn with_api_base(mut self, api_base: Url) -> Self {
        self.api_base = api_base;
        self
    }

    /// Attaches the analysis credential.
    #[must_use]
    pub fn with_token(mut self, token: SecretString) -> Self {
        self.token = Some(token);
        self
    }
}

impl Default for GitHubClientConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for GitHubClientConfig {
    /// Hand-written so that adding a field cannot silently start printing a
    /// secret. A derive would have been correct today and one careless field
    /// away from wrong.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubClientConfig")
            .field("api_base", &self.api_base.as_str())
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// A bounded, rate-aware GitHub REST client.
///
/// Redirects are followed by this type rather than by `reqwest`, which is built
/// with [`redirect::Policy::none`](reqwest::redirect::Policy::none). The archive
/// endpoint answers with a redirect to a different host, and forwarding
/// `Authorization` across that hop would hand the analysis token to a host that
/// never needed it. `reqwest` happens to strip sensitive headers cross-origin
/// today — but a behaviour we get for free is a behaviour that can be taken away
/// by a dependency bump, and this one is load-bearing.
pub struct GitHubRestClient {
    http: reqwest::Client,
    api_base: Url,
    token: Option<SecretString>,
}

impl fmt::Debug for GitHubRestClient {
    /// Hand-written for the same reason as [`GitHubClientConfig`]'s: a derive
    /// would be one careless field away from printing a secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubRestClient")
            .field("api_base", &self.api_base.as_str())
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            // The connection pool is omitted: it is large, says nothing about
            // this client's configuration, and is the kind of output that makes
            // people stop reading logs.
            .finish_non_exhaustive()
    }
}

impl GitHubRestClient {
    /// Builds a client from `config`.
    pub fn new(config: GitHubClientConfig) -> Result<Self, GitHubSourceError> {
        // Rejected once, here, so that every later URL construction can rely on
        // the base being usable. `mailto:` and `data:` URLs have no path to
        // extend and would otherwise fail per request instead of at startup.
        if config.api_base.cannot_be_a_base() {
            return Err(GitHubSourceError::InvalidApiBase);
        }

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(limits::REQUEST_TIMEOUT)
            .build()
            .map_err(|error| transport_failure(&error))?;

        Ok(Self {
            http,
            api_base: config.api_base,
            token: config.token,
        })
    }

    /// Retrieves the contents of `paths`, recording every file it could not.
    ///
    /// Takes the tree rather than fetching one, because the caller resolving a
    /// commit already has it, and because the tree is where a file's size is
    /// known *before* a request is spent on it. It is also what maps a path to
    /// the immutable blob SHA that content is addressed by — which is why this
    /// is safe to cache and
    /// [`fetch_selected_blobs`](GitHubRepositorySource::fetch_selected_blobs),
    /// which must fetch a tree first, is a convenience rather than the primary
    /// entry point.
    ///
    /// Requests are issued in `paths` order and the byte budget is spent in that
    /// order, so the caller's ranking — see [`select_paths`](crate::select_paths)
    /// — is what decides which files survive a tight budget.
    pub async fn collect_blobs(
        &self,
        coordinate: &RepositoryCoordinate,
        tree: &RepositoryTree,
        paths: &[String],
    ) -> Result<BlobSelection, GitHubSourceError> {
        let mut selection = BlobSelection::default();
        let mut spent: u64 = 0;

        for path in paths {
            if let Some(reason) = reject_before_request(tree, path, spent, &selection) {
                selection.skipped.push(SkippedPath {
                    path: path.clone(),
                    reason,
                });
                continue;
            }

            let entry = tree
                .entries
                .iter()
                .find(|entry| &entry.path == path)
                .expect("`reject_before_request` returns a reason when the path is absent");

            let bytes = match self.fetch_blob(coordinate, &entry.sha).await {
                Ok(bytes) => bytes,
                // The tree said it fit and the body disagreed. Recorded rather
                // than raised: one file GitHub mismeasured is a limitation of
                // the evidence, not a reason to discard the files already read.
                Err(GitHubSourceError::LimitExceeded {
                    limit_name: "file bytes",
                    limit,
                    observed,
                }) => {
                    spent = spent.saturating_add(limit);
                    selection.skipped.push(SkippedPath {
                        path: path.clone(),
                        reason: SkipReason::TooLarge {
                            size_bytes: observed,
                            limit_bytes: limit,
                        },
                    });
                    continue;
                }
                Err(error) => return Err(error),
            };

            // Judged after retrieval because a path cannot tell you this. The
            // bytes are still charged to the budget: they were transferred, and
            // pretending otherwise would let a repository of `.png` files cost
            // an unbounded amount while reporting that nothing was read.
            spent = spent.saturating_add(as_u64(bytes.len()));
            if is_binary(&bytes) {
                selection.skipped.push(SkippedPath {
                    path: path.clone(),
                    reason: SkipReason::Binary,
                });
                continue;
            }

            selection.retrieved.push(BlobContent {
                path: path.clone(),
                sha: entry.sha.clone(),
                content_digest: content_digest(&bytes),
                bytes,
            });
        }

        Ok(selection)
    }

    /// Retrieves one blob by its immutable object name.
    async fn fetch_blob(
        &self,
        coordinate: &RepositoryCoordinate,
        blob_sha: &str,
    ) -> Result<Vec<u8>, GitHubSourceError> {
        let url = self.endpoint(&[
            "repos",
            &coordinate.owner,
            &coordinate.name,
            "git",
            "blobs",
            blob_sha,
        ]);
        let response = self.get(url, ACCEPT_RAW, Transfer::Negotiated).await?;
        check_status(&response, || {
            GitHubSourceError::ReferenceNotFound(blob_sha.to_owned())
        })?;
        read_body(response, limits::MAX_FILE_BYTES, "file bytes").await
    }

    /// Builds an absolute endpoint URL from percent-encoded path segments.
    ///
    /// Segments are pushed through `url`'s own encoder rather than formatted
    /// into a string, so an owner named `../../orgs` addresses a repository with
    /// an unusual name instead of a different endpoint.
    fn endpoint(&self, segments: &[&str]) -> Url {
        let mut url = self.api_base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .expect("`GitHubRestClient::new` rejects a base with no path");
            path.pop_if_empty().extend(segments);
        }
        url
    }

    /// Issues a request, following redirects under this crate's own policy.
    async fn get(
        &self,
        url: Url,
        accept: &str,
        transfer: Transfer,
    ) -> Result<Response, GitHubSourceError> {
        let mut target = url;

        // One initial request plus at most `MAX_REDIRECT_HOPS` follow-ups, so a
        // redirect loop costs a bounded number of requests rather than a
        // rate-limit window.
        for _ in 0..=limits::MAX_REDIRECT_HOPS {
            let response = self.send_once(&target, accept, transfer).await?;
            match redirect_target(&response, &target)? {
                Some(next) => target = next,
                None => return Ok(response),
            }
        }

        Err(GitHubSourceError::LimitExceeded {
            limit_name: "redirect hops",
            limit: u64::from(limits::MAX_REDIRECT_HOPS),
            observed: u64::from(limits::MAX_REDIRECT_HOPS) + 1,
        })
    }

    /// Sends exactly one request, with the headers every request must carry.
    async fn send_once(
        &self,
        target: &Url,
        accept: &str,
        transfer: Transfer,
    ) -> Result<Response, GitHubSourceError> {
        let mut request = self
            .http
            .get(target.clone())
            .header(header::ACCEPT, accept)
            .header(header::USER_AGENT, USER_AGENT_VALUE)
            .header(API_VERSION_HEADER, GITHUB_REST_API_VERSION);

        if transfer == Transfer::Identity {
            request = request.header(header::ACCEPT_ENCODING, "identity");
        }

        // The one place a token is ever attached, and it is decided per hop
        // rather than per client.
        if let Some(token) = self.credential_for(target) {
            request = request.bearer_auth(token.expose_secret());
        }

        request
            .send()
            .await
            .map_err(|error| transport_failure(&error))
    }

    /// The credential to send to `target`, if any.
    ///
    /// Compared by *origin* — scheme, host, and port — not by host alone. Two
    /// services on one host are two trust domains, and the loopback pair the
    /// redirect test uses differs only by port, which is precisely the case a
    /// host-only comparison would wave through.
    fn credential_for(&self, target: &Url) -> Option<&SecretString> {
        if target.origin() == self.api_base.origin() {
            self.token.as_ref()
        } else {
            None
        }
    }
}

/// Why `path` will not be requested, if it will not be.
///
/// Every check that can be made from the tree is made here, before a request is
/// spent. The only judgement that cannot — whether the content is binary — is
/// made after retrieval, because no path can answer it.
fn reject_before_request(
    tree: &RepositoryTree,
    path: &str,
    spent: u64,
    selection: &BlobSelection,
) -> Option<SkipReason> {
    if selection.retrieved.len() >= limits::MAX_SELECTED_FILES {
        return Some(SkipReason::SelectionFull {
            limit: limits::MAX_SELECTED_FILES,
        });
    }

    let Some(entry) = tree.entries.iter().find(|entry| entry.path == path) else {
        return Some(SkipReason::NotInTree);
    };
    if entry.kind != TreeEntryKind::Blob {
        return Some(SkipReason::NotAFile);
    }

    // Checked again even though `select_paths` already did. A caller may pass any
    // list of paths, and a budget enforced only by the component that happens to
    // call first is not a budget.
    if let Some(size_bytes) = entry.size_bytes
        && size_bytes > limits::MAX_FILE_BYTES
    {
        return Some(SkipReason::TooLarge {
            size_bytes,
            limit_bytes: limits::MAX_FILE_BYTES,
        });
    }

    // Charged before the request, not after it, so the per-analysis budget is a
    // ceiling rather than a ceiling plus one more file. A blob whose size GitHub
    // omitted charges nothing here and is bounded instead by the per-file cap,
    // which leaves a residual overshoot of at most one file's worth — and only
    // for a response GitHub already declined to measure.
    if spent.saturating_add(entry.size_bytes.unwrap_or(0)) > limits::MAX_TOTAL_FILE_BYTES {
        return Some(SkipReason::BudgetSpent {
            limit_bytes: limits::MAX_TOTAL_FILE_BYTES,
        });
    }

    None
}

/// The next URL to request, or `None` when the response is the answer.
///
/// Only the five redirects that carry a `Location` are followed. `304 Not
/// Modified` is also a `3xx` and has no `Location`, so treating "is a redirect"
/// as "has somewhere to go" would turn a cache response into a malformed one.
fn redirect_target(response: &Response, current: &Url) -> Result<Option<Url>, GitHubSourceError> {
    if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
        return Ok(None);
    }

    let malformed = || GitHubSourceError::MalformedResponse {
        resource: "redirect",
    };

    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(malformed)?;

    // Resolved against the current URL, because `Location` is permitted to be
    // relative and GitHub's own redirects sometimes are.
    let next = current.join(location).map_err(|_| malformed())?;
    if next.scheme() != "https" && next.scheme() != "http" {
        return Err(malformed());
    }

    Ok(Some(next))
}

/// Translates a refusal into the boundary's vocabulary.
///
/// `missing` is a closure rather than a value so that the common path does not
/// clone a coordinate to build an error it will not raise.
fn check_status(
    response: &Response,
    missing: impl FnOnce() -> GitHubSourceError,
) -> Result<(), GitHubSourceError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if is_rate_limited(status, response.headers()) {
        return Err(rate_limit_error(response.headers()));
    }
    if status == StatusCode::NOT_FOUND {
        return Err(missing());
    }
    Err(GitHubSourceError::UnexpectedStatus {
        status: status.as_u16(),
    })
}

/// Whether a refusal is a rate-limit refusal.
///
/// The status alone cannot say. GitHub answers `403` for the primary limit and
/// `429` for the secondary one, and `403` also means "forbidden" for reasons
/// that will never improve by waiting. The headers are what distinguish "come
/// back later" from "go away", and getting it wrong in either direction is
/// expensive: retrying a real refusal burns the remaining budget, and failing a
/// rate limit permanently discards an analysis that would have succeeded.
fn is_rate_limited(status: StatusCode, headers: &HeaderMap) -> bool {
    if status != StatusCode::FORBIDDEN && status != StatusCode::TOO_MANY_REQUESTS {
        return false;
    }
    header_u64(headers, RATE_LIMIT_REMAINING_HEADER) == Some(0)
        || header_u64(headers, RETRY_AFTER_HEADER).is_some()
}

/// Builds the retry-safe error from GitHub's own headers.
///
/// The reset is carried as an instant, not converted to a remaining duration.
/// Converting would need a clock read here, and the result would already be
/// stale by the time the caller acted on it; an instant stays true however long
/// the error is queued.
fn rate_limit_error(headers: &HeaderMap) -> GitHubSourceError {
    let reset_at = header_u64(headers, RATE_LIMIT_RESET_HEADER)
        .and_then(|seconds| i64::try_from(seconds).ok())
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok());

    GitHubSourceError::RateLimited {
        retry_after_seconds: header_u64(headers, RETRY_AFTER_HEADER),
        reset_at,
    }
}

/// Reads one header as an unsigned integer, or `None` if it is absent or is not
/// one.
fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// Reads a response body, refusing to allocate more than `limit` bytes.
///
/// `Content-Length` is checked first as a cheap refusal, and then again while
/// reading, because a declared length is a claim about a body that has not
/// arrived yet.
async fn read_body(
    response: Response,
    limit: u64,
    limit_name: &'static str,
) -> Result<Vec<u8>, GitHubSourceError> {
    let exceeded = |observed| GitHubSourceError::LimitExceeded {
        limit_name,
        limit,
        observed,
    };

    if let Some(declared) = response.content_length()
        && declared > limit
    {
        return Err(exceeded(declared));
    }

    let cap = usize::try_from(limit).unwrap_or(usize::MAX);
    let mut body = Vec::new();
    let mut chunks = response.bytes_stream();

    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| transport_failure(&error))?;
        if body.len().saturating_add(chunk.len()) > cap {
            return Err(exceeded(as_u64(body.len().saturating_add(chunk.len()))));
        }
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

/// Streams a response to `destination`, refusing to write more than `cap`.
///
/// Never buffers the whole body: extraction runs on a size-limited volume, and
/// an archive held in memory would turn an exceeded budget into an out-of-memory
/// kill that strands the worker's lease instead of a catchable error.
async fn stream_to_file(
    response: Response,
    cap: u64,
    destination: &Path,
) -> Result<u64, GitHubSourceError> {
    let exceeded = |observed| GitHubSourceError::LimitExceeded {
        limit_name: "archive compressed bytes",
        limit: cap,
        observed,
    };

    if let Some(declared) = response.content_length()
        && declared > cap
    {
        return Err(exceeded(declared));
    }

    let mut file =
        tokio::fs::File::create(destination)
            .await
            .map_err(|_| GitHubSourceError::Io {
                operation: "create the archive file",
            })?;

    let mut written: u64 = 0;
    let mut chunks = response.bytes_stream();

    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| transport_failure(&error))?;
        written = written.saturating_add(as_u64(chunk.len()));
        if written > cap {
            return Err(exceeded(written));
        }
        file.write_all(&chunk)
            .await
            .map_err(|_| GitHubSourceError::Io {
                operation: "write the archive file",
            })?;
    }

    file.flush().await.map_err(|_| GitHubSourceError::Io {
        operation: "flush the archive file",
    })?;

    Ok(written)
}

/// Parses a JSON body into a payload type.
///
/// The `serde_json` error is discarded on purpose: its `Display` quotes the
/// input around the failure, which would copy part of a response body into a
/// log for a class of failures where the body is exactly what is unexpected.
fn parse_json<T: DeserializeOwned>(
    body: &[u8],
    resource: &'static str,
) -> Result<T, GitHubSourceError> {
    serde_json::from_slice(body).map_err(|_| GitHubSourceError::MalformedResponse { resource })
}

/// Maps GitHub's tree entry type to the domain's.
fn tree_entry_kind(raw: &str) -> Result<TreeEntryKind, GitHubSourceError> {
    match raw {
        "blob" => Ok(TreeEntryKind::Blob),
        "tree" => Ok(TreeEntryKind::Tree),
        // Git stores a submodule as a commit object inside a tree.
        "commit" => Ok(TreeEntryKind::Submodule),
        // Not quietly treated as a file. The set is Git's own and closed, so an
        // unrecognised value means the response is not what this code believes
        // it is reading, and guessing would put unknown content into evidence.
        _ => Err(GitHubSourceError::MalformedResponse {
            resource: "tree entry",
        }),
    }
}

/// Git's own binary test: a `NUL` byte inside the first eight thousand.
///
/// Borrowed rather than invented so that RepoLens and `git diff` never disagree
/// about a file, which matters because the report cites paths a reader will open
/// in a checkout.
fn is_binary(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .take(limits::BINARY_SNIFF_BYTES)
        .any(|byte| *byte == 0)
}

/// Widens a length for comparison against a budget.
///
/// Saturating rather than casting: on every platform this runs on the value fits,
/// and a silent wrap in a size check is the one failure mode a size check exists
/// to prevent.
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Wraps a transport failure as a category, never as a message.
fn transport_failure(error: &reqwest::Error) -> GitHubSourceError {
    GitHubSourceError::Transport(transport_category(error).to_owned())
}

/// Names the kind of transport failure without reproducing its text.
///
/// `reqwest::Error` renders the request URL through `Display`. The `ConfigError`
/// precedent in `repolens-server` is explicit that an error which echoes its
/// input is a disclosure waiting for the wrong input, and a closed set of
/// categories also keeps log cardinality fixed.
fn transport_category(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_redirect() {
        "redirect"
    } else if error.is_decode() {
        "decode"
    } else if error.is_body() {
        "body"
    } else if error.is_builder() {
        "builder"
    } else {
        "request"
    }
}

impl GitHubRestClient {
    /// See [`GitHubRepositorySource::resolve_repository`].
    async fn resolve_repository_inner(
        &self,
        coordinate: &RepositoryCoordinate,
    ) -> Result<ResolvedRepository, GitHubSourceError> {
        let url = self.endpoint(&["repos", &coordinate.owner, &coordinate.name]);
        let response = self.get(url, ACCEPT_JSON, Transfer::Negotiated).await?;
        check_status(&response, || {
            GitHubSourceError::RepositoryNotFound(coordinate.clone())
        })?;

        let body = read_body(response, limits::MAX_RESPONSE_BYTES, "response bytes").await?;
        let payload: RepositoryPayload = parse_json(&body, "repository")?;

        // Reported exactly as an absent repository. GitHub already answers `404`
        // for a private repository an anonymous caller cannot see, so separating
        // the two here would make RepoLens' answer depend on whether a token
        // happened to be configured — and would confirm the existence of a
        // repository the submitter is not entitled to know about.
        if payload.private {
            return Err(GitHubSourceError::RepositoryNotFound(coordinate.clone()));
        }

        if payload.size > limits::MAX_REPOSITORY_KILOBYTES {
            return Err(GitHubSourceError::LimitExceeded {
                limit_name: "repository kilobytes",
                limit: limits::MAX_REPOSITORY_KILOBYTES,
                observed: payload.size,
            });
        }

        // Taken from the response rather than the request, so a repository that
        // has been renamed is analyzed and recorded under the name it now has.
        let (owner, name) =
            payload
                .split_full_name()
                .ok_or(GitHubSourceError::MalformedResponse {
                    resource: "repository",
                })?;

        Ok(ResolvedRepository {
            coordinate: RepositoryCoordinate::new(owner, name),
            default_branch: payload.default_branch.clone(),
            archived: payload.archived,
            size_kilobytes: payload.size,
        })
    }

    /// See [`GitHubRepositorySource::resolve_commit`].
    async fn resolve_commit_inner(
        &self,
        coordinate: &RepositoryCoordinate,
        reference: &str,
    ) -> Result<ResolvedCommit, GitHubSourceError> {
        let url = self.endpoint(&[
            "repos",
            &coordinate.owner,
            &coordinate.name,
            "commits",
            reference,
        ]);
        let response = self.get(url, ACCEPT_JSON, Transfer::Negotiated).await?;
        check_status(&response, || {
            GitHubSourceError::ReferenceNotFound(reference.to_owned())
        })?;

        let body = read_body(response, limits::MAX_RESPONSE_BYTES, "response bytes").await?;
        let payload: CommitPayload = parse_json(&body, "commit")?;

        let malformed = || GitHubSourceError::MalformedResponse { resource: "commit" };

        // Both digests are validated even though only one is kept as a typed
        // value. They become the analysis' identity, and an identity assembled
        // from unvalidated strings is one that can be forged by a malformed
        // response rather than chosen.
        let sha = CommitSha::parse(&payload.sha).map_err(|_| malformed())?;
        let tree_sha = TreeSha::parse(&payload.commit.tree.sha).map_err(|_| malformed())?;

        Ok(ResolvedCommit {
            sha,
            tree_sha: tree_sha.as_str().to_owned(),
            committed_at: payload.commit.committer.date,
        })
    }

    /// See [`GitHubRepositorySource::fetch_tree`].
    async fn fetch_tree_inner(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
    ) -> Result<RepositoryTree, GitHubSourceError> {
        let mut url = self.endpoint(&[
            "repos",
            &coordinate.owner,
            &coordinate.name,
            "git",
            "trees",
            commit.as_str(),
        ]);
        url.query_pairs_mut().append_pair("recursive", "1");

        let response = self.get(url, ACCEPT_JSON, Transfer::Negotiated).await?;
        check_status(&response, || {
            GitHubSourceError::ReferenceNotFound(commit.to_string())
        })?;

        let body = read_body(response, limits::MAX_RESPONSE_BYTES, "response bytes").await?;
        let payload: TreePayload = parse_json(&body, "tree")?;

        // Two independent reasons the listing may be short, and both mean the
        // same thing to a reader: this is not the whole repository. Neither is an
        // error. A tree we could only partly see still supports every finding
        // drawn from what we did see, and refusing the analysis outright would
        // trade a qualified report for no report.
        let truncated = payload.truncated || payload.tree.len() > limits::MAX_TREE_ENTRIES;

        let mut entries = Vec::with_capacity(payload.tree.len().min(limits::MAX_TREE_ENTRIES));
        for entry in payload.tree.into_iter().take(limits::MAX_TREE_ENTRIES) {
            entries.push(TreeEntry {
                path: entry.path,
                sha: entry.sha,
                kind: tree_entry_kind(&entry.kind)?,
                size_bytes: entry.size,
            });
        }

        Ok(RepositoryTree {
            sha: payload.sha,
            entries,
            truncated,
        })
    }

    /// See [`GitHubRepositorySource::download_archive`].
    async fn download_archive_inner(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
        max_compressed_bytes: u64,
        destination: &Path,
    ) -> Result<ArchiveDownload, GitHubSourceError> {
        // The caller's budget, clamped. A caller asking for more than the
        // boundary's own ceiling does not get more: the ceiling exists because
        // of what this worker can survive, which is not something a caller is in
        // a position to know.
        let cap = max_compressed_bytes.min(limits::MAX_ARCHIVE_COMPRESSED_BYTES);

        let url = self.endpoint(&[
            "repos",
            &coordinate.owner,
            &coordinate.name,
            "tarball",
            commit.as_str(),
        ]);
        let response = self.get(url, ACCEPT_ANY, Transfer::Identity).await?;
        check_status(&response, || {
            GitHubSourceError::ReferenceNotFound(commit.to_string())
        })?;

        match stream_to_file(response, cap, destination).await {
            Ok(compressed_bytes) => Ok(ArchiveDownload { compressed_bytes }),
            Err(error) => {
                // A truncated tarball is not partial evidence, it is a file that
                // the extractor would open and fail on later, somewhere with less
                // context than here. Removed rather than left to be found.
                drop(tokio::fs::remove_file(destination).await);
                Err(error)
            }
        }
    }
}

impl GitHubRepositorySource for GitHubRestClient {
    fn resolve_repository(
        &self,
        coordinate: &RepositoryCoordinate,
    ) -> impl Future<Output = Result<ResolvedRepository, GitHubSourceError>> + Send {
        self.resolve_repository_inner(coordinate)
    }

    fn resolve_commit(
        &self,
        coordinate: &RepositoryCoordinate,
        reference: &str,
    ) -> impl Future<Output = Result<ResolvedCommit, GitHubSourceError>> + Send {
        self.resolve_commit_inner(coordinate, reference)
    }

    fn fetch_tree(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
    ) -> impl Future<Output = Result<RepositoryTree, GitHubSourceError>> + Send {
        self.fetch_tree_inner(coordinate, commit)
    }

    /// Fetches the tree first, then delegates to
    /// [`collect_blobs`](GitHubRestClient::collect_blobs).
    ///
    /// The extra request is the price of a signature that takes paths: a path
    /// alone does not name an immutable object, and content has to be addressed
    /// by blob SHA to be cacheable at all. Callers that already hold a tree —
    /// which is every caller that ran selection — should use `collect_blobs`,
    /// which also returns the files it skipped and why.
    async fn fetch_selected_blobs(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
        paths: &[String],
    ) -> Result<Vec<BlobContent>, GitHubSourceError> {
        let tree = self.fetch_tree_inner(coordinate, commit).await?;
        let selection = self.collect_blobs(coordinate, &tree, paths).await?;
        Ok(selection.retrieved)
    }

    fn download_archive(
        &self,
        coordinate: &RepositoryCoordinate,
        commit: &CommitSha,
        max_compressed_bytes: u64,
        destination: &Path,
    ) -> impl Future<Output = Result<ArchiveDownload, GitHubSourceError>> + Send {
        self.download_archive_inner(coordinate, commit, max_compressed_bytes, destination)
    }
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderMap;
    use secrecy::SecretString;
    use url::Url;

    use super::{
        GitHubClientConfig, GitHubRestClient, RATE_LIMIT_REMAINING_HEADER, RATE_LIMIT_RESET_HEADER,
        header_u64, is_binary, is_rate_limited, rate_limit_error, tree_entry_kind,
    };
    use crate::{GitHubSourceError, TreeEntryKind};

    const EXAMPLE_TOKEN: &str = "EXAMPLE_NOT_A_REAL_TOKEN";

    fn client_for(api_base: &str) -> GitHubRestClient {
        let config = GitHubClientConfig::new()
            .with_api_base(Url::parse(api_base).expect("test base is a literal"))
            .with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()));
        GitHubRestClient::new(config).expect("a http base is usable")
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).expect("valid name"),
                value.parse().expect("valid value"),
            );
        }
        map
    }

    #[test]
    fn the_credential_is_scoped_to_an_origin_including_its_port() {
        // Two loopback services differ only by port. A host-only comparison
        // would call them the same trust domain, which is exactly the mistake
        // this function exists to not make.
        let client = client_for("http://127.0.0.1:9001/");

        let same = Url::parse("http://127.0.0.1:9001/repos/o/r").expect("literal");
        let other_port = Url::parse("http://127.0.0.1:9002/archive").expect("literal");
        let other_scheme = Url::parse("https://127.0.0.1:9001/repos/o/r").expect("literal");
        let other_host = Url::parse("http://codeload.example.invalid/archive").expect("literal");

        assert!(client.credential_for(&same).is_some());
        assert!(client.credential_for(&other_port).is_none());
        assert!(client.credential_for(&other_scheme).is_none());
        assert!(client.credential_for(&other_host).is_none());
    }

    #[test]
    fn debug_output_never_contains_the_token() {
        // The client is held inside worker state that other code may print while
        // debugging. A derived `Debug` would have leaked it.
        let rendered = format!("{:?}", client_for("https://api.github.com/"));
        assert!(!rendered.contains(EXAMPLE_TOKEN), "{rendered}");
        assert!(rendered.contains("[redacted]"), "{rendered}");

        let config =
            GitHubClientConfig::new().with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()));
        let rendered = format!("{config:?}");
        assert!(!rendered.contains(EXAMPLE_TOKEN), "{rendered}");
    }

    #[test]
    fn path_segments_are_encoded_rather_than_interpolated() {
        // A coordinate reaches this crate from a user-supplied URL. If segments
        // were formatted into a string, this owner would address
        // `/repos/../../orgs/evil`.
        let client = client_for("https://api.github.com/");
        let url = client.endpoint(&["repos", "../../orgs", "a/b", "commits", "HEAD"]);

        assert_eq!(
            url.as_str(),
            "https://api.github.com/repos/..%2F..%2Forgs/a%2Fb/commits/HEAD"
        );
    }

    #[test]
    fn a_base_that_cannot_carry_a_path_is_refused_at_construction() {
        let config = GitHubClientConfig::new()
            .with_api_base(Url::parse("mailto:nobody@example.invalid").expect("literal"));
        assert!(matches!(
            GitHubRestClient::new(config),
            Err(GitHubSourceError::InvalidApiBase)
        ));
    }

    #[test]
    fn a_forbidden_response_is_only_a_rate_limit_when_the_headers_say_so() {
        let exhausted = headers(&[(RATE_LIMIT_REMAINING_HEADER, "0")]);
        let plenty = headers(&[(RATE_LIMIT_REMAINING_HEADER, "4999")]);

        assert!(is_rate_limited(reqwest::StatusCode::FORBIDDEN, &exhausted));
        assert!(is_rate_limited(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            &exhausted
        ));
        // A plain refusal. Retrying this would spend the remaining budget on a
        // request that will never be allowed.
        assert!(!is_rate_limited(reqwest::StatusCode::FORBIDDEN, &plenty));
        assert!(!is_rate_limited(reqwest::StatusCode::NOT_FOUND, &exhausted));
    }

    #[test]
    fn the_reset_instant_survives_header_parsing() {
        let map = headers(&[
            (RATE_LIMIT_REMAINING_HEADER, "0"),
            (RATE_LIMIT_RESET_HEADER, "1767225600"),
        ]);

        match rate_limit_error(&map) {
            GitHubSourceError::RateLimited { reset_at, .. } => {
                assert_eq!(
                    reset_at.map(time::OffsetDateTime::unix_timestamp),
                    Some(1_767_225_600)
                );
            }
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn an_unparseable_reset_header_is_absent_rather_than_wrong() {
        // A guessed instant would send a worker back at a time GitHub never
        // named, which is worse than admitting the header was unusable.
        let map = headers(&[
            (RATE_LIMIT_REMAINING_HEADER, "0"),
            (RATE_LIMIT_RESET_HEADER, "soon"),
        ]);
        assert_eq!(header_u64(&map, RATE_LIMIT_RESET_HEADER), None);

        match rate_limit_error(&map) {
            GitHubSourceError::RateLimited { reset_at, .. } => assert!(reset_at.is_none()),
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_tree_entry_type_is_never_guessed_to_be_a_file() {
        assert_eq!(tree_entry_kind("blob").expect("known"), TreeEntryKind::Blob);
        assert_eq!(tree_entry_kind("tree").expect("known"), TreeEntryKind::Tree);
        assert_eq!(
            tree_entry_kind("commit").expect("known"),
            TreeEntryKind::Submodule
        );
        assert!(tree_entry_kind("symlink-of-the-future").is_err());
    }

    #[test]
    fn binary_detection_uses_gits_own_window() {
        assert!(!is_binary(b"# A readme\n"));
        assert!(is_binary(b"\x89PNG\r\n\x1a\n\x00\x00"));

        // A `NUL` past the window is deliberately not found: matching Git's rule
        // matters more than catching every possible binary.
        let mut late = vec![b'a'; crate::limits::BINARY_SNIFF_BYTES];
        late.push(0);
        assert!(!is_binary(&late));
    }

    #[test]
    fn errors_never_render_a_url_or_a_token() {
        // The `ConfigError` precedent: an error that echoes its input is a
        // disclosure waiting for the wrong input.
        let rendered = GitHubSourceError::Transport("connect".to_owned()).to_string();
        assert!(!rendered.contains("http"), "{rendered}");
        assert!(!rendered.contains(EXAMPLE_TOKEN), "{rendered}");
    }
}
