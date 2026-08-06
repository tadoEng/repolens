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

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt;
use reqwest::header::{self, HeaderMap};
use reqwest::{Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use url::{Host, Url};

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

/// Names tried when opening the archive's temporary file before giving up.
///
/// More than one because the name carries a process id and a counter, and a
/// previous run killed mid-download can leave one behind; more than a handful
/// would mean something other than a collision is wrong, and retrying would
/// only postpone reporting it.
const PART_FILE_ATTEMPTS: u32 = 8;

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
    allow_insecure_loopback: bool,
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
            allow_insecure_loopback: false,
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

    /// Permits plain HTTP, but only to a loopback address.
    ///
    /// **Development and tests only.** The suite runs against a local mock that
    /// speaks HTTP, and the alternative — accepting HTTP whenever the host looks
    /// local enough — is how a production deployment ends up sending an analysis
    /// token in cleartext because a hostname resolved somewhere unexpected.
    /// Making it an explicit, named opt-in means the insecure path exists in one
    /// place that a reviewer can grep for, and is off by default.
    ///
    /// Loopback and nothing else: traffic to `127.0.0.0/8` or `::1` does not
    /// leave the machine, so there is no path for it to be read or rewritten on.
    #[must_use]
    pub fn allow_insecure_loopback(mut self) -> Self {
        self.allow_insecure_loopback = true;
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
            // Redacted rather than printed, because a configuration is
            // `Debug`-printed while it still holds whatever was configured —
            // including the embedded credential that construction will reject.
            .field("api_base", &redacted_url(&self.api_base))
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .field("allow_insecure_loopback", &self.allow_insecure_loopback)
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
    allow_insecure_loopback: bool,
}

impl fmt::Debug for GitHubRestClient {
    /// Hand-written for the same reason as [`GitHubClientConfig`]'s: a derive
    /// would be one careless field away from printing a secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubRestClient")
            .field("api_base", &redacted_url(&self.api_base))
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            // The connection pool is omitted: it is large, says nothing about
            // this client's configuration, and is the kind of output that makes
            // people stop reading logs.
            .finish_non_exhaustive()
    }
}

impl GitHubRestClient {
    /// Builds a client from `config`.
    ///
    /// Every rule the base has to satisfy is checked once, here, rather than per
    /// request: a configuration that would send a token in cleartext is a
    /// deployment mistake, and a deployment mistake should stop a process at
    /// startup instead of being discovered from a packet capture.
    pub fn new(config: GitHubClientConfig) -> Result<Self, GitHubSourceError> {
        check_api_base(&config.api_base, config.allow_insecure_loopback)?;

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(limits::REQUEST_TIMEOUT)
            .build()
            .map_err(|error| transport_failure(&error))?;

        Ok(Self {
            http,
            api_base: config.api_base,
            token: config.token,
            allow_insecure_loopback: config.allow_insecure_loopback,
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
        let mut attempted: usize = 0;
        let mut seen: HashSet<&str> = HashSet::with_capacity(paths.len());

        for path in paths {
            // A repeated path is not a second file. Whatever became of it —
            // retrieved or skipped — was decided and recorded the first time it
            // appeared, and requesting it again would spend a request on bytes
            // already held.
            if !seen.insert(path.as_str()) {
                continue;
            }

            let remaining = limits::MAX_TOTAL_FILE_BYTES.saturating_sub(spent);
            if let Some(reason) = reject_before_request(tree, path, remaining, attempted) {
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

            // What is left of the analysis budget, never the per-file ceiling on
            // its own. Passing the per-file cap here is what let a blob GitHub
            // declined to measure transfer a further megabyte after the total
            // was already spent.
            let cap = limits::MAX_FILE_BYTES.min(remaining);

            // Counted before the request rather than after a successful one.
            // This is the ceiling on how many times this loop may talk to
            // GitHub, and a count that only successes advanced would bound
            // nothing: a caller submitting binary paths would skip every result
            // and still spend a request on each.
            attempted += 1;

            let bytes = match self.fetch_blob(coordinate, &entry.sha, cap).await {
                Ok(bytes) => bytes,
                // The tree said it fit and the body disagreed. Recorded rather
                // than raised: one file GitHub mismeasured is a limitation of
                // the evidence, not a reason to discard the files already read.
                Err(ReadFailure::Oversized {
                    observed,
                    transferred,
                }) => {
                    spent = spent.saturating_add(transferred);
                    selection.skipped.push(SkippedPath {
                        path: path.clone(),
                        reason: oversize_reason(cap, observed),
                    });
                    continue;
                }
                Err(ReadFailure::Other(error)) => return Err(error),
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

    /// Retrieves one blob by its immutable object name, reading at most `cap`
    /// bytes.
    ///
    /// The cap is a parameter rather than a constant because it is the caller
    /// that knows how much of the analysis budget is left, and a reader handed
    /// only the per-file ceiling can overshoot the per-analysis one by a whole
    /// file.
    async fn fetch_blob(
        &self,
        coordinate: &RepositoryCoordinate,
        blob_sha: &str,
        cap: u64,
    ) -> Result<Vec<u8>, ReadFailure> {
        let url = self.endpoint(&[
            "repos",
            &coordinate.owner,
            &coordinate.name,
            "git",
            "blobs",
            blob_sha,
        ]);
        let response = self
            .get(url, ACCEPT_RAW, Transfer::Negotiated)
            .await
            .map_err(ReadFailure::Other)?;
        check_status(&response, || {
            GitHubSourceError::ReferenceNotFound(blob_sha.to_owned())
        })
        .map_err(ReadFailure::Other)?;
        read_body(response, cap).await
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

            let next = {
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok());
                redirect_target(
                    response.status().as_u16(),
                    location,
                    &target,
                    self.allow_insecure_loopback,
                )?
            };

            match next {
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
///
/// `attempted` is how many requests this collection has already issued, and
/// `remaining` how much of the per-analysis budget is left.
fn reject_before_request(
    tree: &RepositoryTree,
    path: &str,
    remaining: u64,
    attempted: usize,
) -> Option<SkipReason> {
    // Attempts, not results. `MAX_SELECTED_FILES` is stated as a ceiling on what
    // one analysis costs GitHub, and counting only the files that came back
    // usable would bound the output list while leaving the request count to the
    // caller's imagination.
    if attempted >= limits::MAX_SELECTED_FILES {
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

    // A size the tree reports is charged before the request, so a file that
    // cannot fit costs nothing at all. A size GitHub omitted cannot be charged
    // in advance — it is bounded instead by the cap the caller passes to the
    // body reader, which is this same remainder.
    if remaining == 0 || entry.size_bytes.unwrap_or(0) > remaining {
        return Some(SkipReason::BudgetSpent {
            limit_bytes: limits::MAX_TOTAL_FILE_BYTES,
        });
    }

    None
}

/// Which ceiling a body exceeded, said in the report's vocabulary.
///
/// The two are not interchangeable to a reader: one file being pathological and
/// the analysis having run out of budget call for different responses, and the
/// cap in force is what distinguishes them.
fn oversize_reason(cap: u64, observed: u64) -> SkipReason {
    if cap < limits::MAX_FILE_BYTES {
        SkipReason::BudgetSpent {
            limit_bytes: limits::MAX_TOTAL_FILE_BYTES,
        }
    } else {
        SkipReason::TooLarge {
            size_bytes: observed,
            limit_bytes: limits::MAX_FILE_BYTES,
        }
    }
}

/// The next URL to request, or `None` when the response is the answer.
///
/// Only the five redirects that carry a `Location` are followed. `304 Not
/// Modified` is also a `3xx` and has no `Location`, so treating "is a redirect"
/// as "has somewhere to go" would turn a cache response into a malformed one.
///
/// Takes the status and the header rather than the response, so that the rule
/// this function exists for — where a redirect may lead — is testable without a
/// live TLS server to be redirected off.
fn redirect_target(
    status: u16,
    location: Option<&str>,
    current: &Url,
    allow_insecure_loopback: bool,
) -> Result<Option<Url>, GitHubSourceError> {
    if !matches!(status, 301 | 302 | 303 | 307 | 308) {
        return Ok(None);
    }

    let malformed = || GitHubSourceError::MalformedResponse {
        resource: "redirect",
    };

    let location = location.ok_or_else(malformed)?;

    // Resolved against the current URL, because `Location` is permitted to be
    // relative and GitHub's own redirects sometimes are.
    let next = current.join(location).map_err(|_| malformed())?;

    // A hop that began under TLS never leaves it, development option or not.
    // The option exists so a local mock can speak plain HTTP, not so a response
    // from a production origin can walk the analysis — and its credential — off
    // the encrypted path it started on.
    if current.scheme() == "https" && next.scheme() != "https" {
        return Err(GitHubSourceError::InsecureRedirect);
    }

    // Every hop is held to the rule the configured base was held to. A redirect
    // is a URL chosen by the responding server, which is precisely why it does
    // not get a weaker rule than the one an operator had to satisfy by hand.
    secure_transport(&next, allow_insecure_loopback)
        .map_err(|_| GitHubSourceError::InsecureRedirect)?;

    Ok(Some(next))
}

/// Whether `url` may be requested at all, given the development option.
///
/// HTTPS everywhere, with one hole that has to be opened deliberately: plain
/// HTTP to a loopback address, which is what the test suite's mock speaks and
/// what never leaves the machine. Anything else would mean the analysis token,
/// the evidence, and the archive all cross a network in cleartext.
fn secure_transport(url: &Url, allow_insecure_loopback: bool) -> Result<(), &'static str> {
    if url.scheme() == "https" {
        return Ok(());
    }
    if url.scheme() == "http" && allow_insecure_loopback && is_loopback(url) {
        return Ok(());
    }
    Err("only HTTPS is accepted, or HTTP to a loopback address under the development option")
}

/// Whether `url` addresses this machine and only this machine.
///
/// Compared against the parsed host rather than the string, so that neither
/// `127.1` nor `[::1]` nor a name that merely starts with `localhost` can be
/// mistaken for — or hidden from — the loopback rule.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        // Resolution is the operating system's business, but `localhost` is
        // reserved for loopback by RFC 6761 and is what a local mock is usually
        // reached by.
        Some(Host::Domain(name)) => name == "localhost",
        None => false,
    }
}

/// Every rule the configured REST base has to satisfy.
///
/// All of them are about what the base *is*, not about what a request to it
/// returns, so all of them can be — and are — settled once at construction.
fn check_api_base(api_base: &Url, allow_insecure_loopback: bool) -> Result<(), GitHubSourceError> {
    let invalid = |reason| GitHubSourceError::InvalidApiBase { reason };

    // `mailto:` and `data:` URLs have no path to extend, so every later
    // endpoint construction would fail per request instead of at startup.
    if api_base.cannot_be_a_base() {
        return Err(invalid("it cannot be used as a URL base"));
    }

    secure_transport(api_base, allow_insecure_loopback).map_err(invalid)?;

    // A credential in the base is a credential in every log line that prints a
    // URL, and it would be sent as basic auth alongside the bearer token that
    // this boundary attaches deliberately.
    if !api_base.username().is_empty() || api_base.password().is_some() {
        return Err(invalid("it must not carry a username or password"));
    }

    // Endpoints are built by extending the base's path. A query or a fragment
    // would be silently dropped from every request built from it, so a base
    // carrying either does not mean what whoever configured it thinks it means.
    if api_base.query().is_some() {
        return Err(invalid("it must not carry a query string"));
    }
    if api_base.fragment().is_some() {
        return Err(invalid("it must not carry a fragment"));
    }

    Ok(())
}

/// Renders a URL with every part a secret is put in removed.
///
/// Used by both `Debug` implementations. A configuration is printable before it
/// is validated, so this assumes the worst about what it holds.
fn redacted_url(url: &Url) -> String {
    let mut safe = url.clone();
    let _ = safe.set_username("");
    let _ = safe.set_password(None);
    safe.set_query(None);
    safe.set_fragment(None);
    safe.to_string()
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
/// `429` is one unconditionally. GitHub documents it for both the primary and
/// the secondary limit, and documents the header-less case explicitly — wait at
/// least a minute — so a `429` that carries no timing hint is a rate limit with
/// no hint, not a permanent refusal. Treating it as permanent discarded an
/// analysis that a minute's wait would have completed.
///
/// `403` keeps the stricter test, because GitHub also answers `403` for the
/// primary limit *and* for refusals that will never improve by waiting. There
/// the headers are the only thing that distinguishes "come back later" from "go
/// away", and retrying a real refusal spends the rest of the window on a request
/// that can never be allowed.
fn is_rate_limited(status: StatusCode, headers: &HeaderMap) -> bool {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return true;
    }
    if status != StatusCode::FORBIDDEN {
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

/// Why a bounded read produced no body.
///
/// The oversized case is kept apart from every other failure because the caller
/// charges the analysis budget for what actually crossed the wire, and a
/// declared length refused before the first byte cost nothing at all.
enum ReadFailure {
    /// The body did not fit the cap in force for this request.
    Oversized {
        /// What exceeded the cap: a declared length, or the point at which the
        /// stream was cut.
        observed: u64,
        /// Bytes that actually arrived. Zero when a declared length was refused
        /// before the body started.
        transferred: u64,
    },
    /// Anything else, already in the boundary's vocabulary.
    Other(GitHubSourceError),
}

impl ReadFailure {
    /// Renders the failure as the boundary's own error, naming the ceiling.
    ///
    /// For callers that only propagate. The one caller that charges a budget
    /// matches on the variant instead, because by then the distinction between
    /// "declared" and "transferred" has been flattened away.
    fn into_error(self, limit_name: &'static str, limit: u64) -> GitHubSourceError {
        match self {
            Self::Oversized { observed, .. } => GitHubSourceError::LimitExceeded {
                limit_name,
                limit,
                observed,
            },
            Self::Other(error) => error,
        }
    }
}

/// Reads a response body, refusing to allocate more than `limit` bytes.
///
/// `Content-Length` is checked first as a cheap refusal, and then again while
/// reading, because a declared length is a claim about a body that has not
/// arrived yet.
async fn read_body(response: Response, limit: u64) -> Result<Vec<u8>, ReadFailure> {
    if let Some(declared) = response.content_length()
        && declared > limit
    {
        // Nothing has been transferred: the refusal happens on the headers, and
        // the body is dropped unread.
        return Err(ReadFailure::Oversized {
            observed: declared,
            transferred: 0,
        });
    }

    let mut body: Vec<u8> = Vec::new();
    let mut chunks = response.bytes_stream();

    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| ReadFailure::Other(transport_failure(&error)))?;
        let arrived = as_u64(body.len()).saturating_add(as_u64(chunk.len()));
        if arrived > limit {
            // The chunk that broke the cap arrived before it could be refused,
            // so it is charged. Reporting only the bytes kept would understate
            // what the transfer cost.
            return Err(ReadFailure::Oversized {
                observed: arrived,
                transferred: arrived,
            });
        }
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

/// Reads a JSON response through the shared response ceiling.
async fn read_response_body(response: Response) -> Result<Vec<u8>, GitHubSourceError> {
    read_body(response, limits::MAX_RESPONSE_BYTES)
        .await
        .map_err(|failure| failure.into_error("response bytes", limits::MAX_RESPONSE_BYTES))
}

/// The ceiling breach an archive reports.
fn archive_exceeded(cap: u64, observed: u64) -> GitHubSourceError {
    GitHubSourceError::LimitExceeded {
        limit_name: "archive compressed bytes",
        limit: cap,
        observed,
    }
}

/// Opens a file next to `destination` that did not exist a moment ago.
///
/// `create_new`, so an existing file is never opened and therefore never
/// truncated — including `destination` itself, which may be a file the caller
/// wrote and still needs. A sibling rather than a temporary directory, so the
/// rename that follows stays within one filesystem: a cross-device rename is a
/// copy, and copying an archive is the transfer this whole path exists to do
/// only once.
async fn create_part_file(
    destination: &Path,
) -> Result<(PathBuf, tokio::fs::File), GitHubSourceError> {
    /// Distinguishes concurrent downloads within one process; the process id
    /// distinguishes them between processes.
    static NEXT_SERIAL: AtomicU64 = AtomicU64::new(0);

    let failed = || GitHubSourceError::Io {
        operation: "create the archive file",
    };

    let Some(file_name) = destination.file_name() else {
        return Err(failed());
    };

    for _ in 0..PART_FILE_ATTEMPTS {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        let mut candidate_name = file_name.to_owned();
        candidate_name.push(format!(".{}-{serial}.part", std::process::id()));
        let candidate = destination.with_file_name(candidate_name);

        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
            .await
        {
            Ok(file) => return Ok((candidate, file)),
            // A leftover from a run that was killed mid-download. Trying the
            // next name is cheaper than deciding whether it is safe to remove
            // something another process may still be writing.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(failed()),
        }
    }

    Err(failed())
}

/// Streams a response into `file`, refusing to write more than `cap`.
///
/// Never buffers the whole body: extraction runs on a size-limited volume, and
/// an archive held in memory would turn an exceeded budget into an out-of-memory
/// kill that strands the worker's lease instead of a catchable error.
///
/// Takes the file by value, so the handle is closed before the caller renames
/// it into place.
async fn stream_to_file(
    response: Response,
    cap: u64,
    mut file: tokio::fs::File,
) -> Result<u64, GitHubSourceError> {
    let mut written: u64 = 0;
    let mut chunks = response.bytes_stream();

    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| transport_failure(&error))?;
        written = written.saturating_add(as_u64(chunk.len()));
        if written > cap {
            return Err(archive_exceeded(cap, written));
        }
        file.write_all(&chunk)
            .await
            .map_err(|_| GitHubSourceError::Io {
                operation: "write the archive file",
            })?;
    }

    // Durable before it is named. A rename publishes the path to the extractor,
    // and publishing a name whose bytes are still in a write-back cache would
    // make a crash look like a corrupt archive rather than an absent one.
    file.sync_all().await.map_err(|_| GitHubSourceError::Io {
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

        let body = read_response_body(response).await?;
        let payload: RepositoryPayload = parse_json(&body, "repository")?;

        // Reported exactly as an absent repository. GitHub already answers `404`
        // for a private repository an anonymous caller cannot see, so separating
        // the two here would make RepoLens' answer depend on whether a token
        // happened to be configured — and would confirm the existence of a
        // repository the submitter is not entitled to know about.
        if payload.private {
            return Err(GitHubSourceError::RepositoryNotFound(coordinate.clone()));
        }

        // The size ceiling is **not** enforced here, and that is deliberate.
        //
        // It used to be, above this point, which meant an oversized repository
        // was rejected before `full_name` had been parsed — so the caller
        // received `LimitExceeded` with no way to learn the canonical
        // coordinate, and a renamed repository reached a terminal state under
        // the address the submitter typed. Rejecting is a policy decision the
        // caller has to record against a specific repository, so it belongs
        // where that repository is known.
        //
        // `size_kilobytes` below carries the number for the caller to judge.
        // Nothing has been downloaded at this point: this is one metadata
        // request, so refusing here bought no bandwidth that refusing one step
        // later does not.

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

        let body = read_response_body(response).await?;
        let payload: CommitPayload = parse_json(&body, "commit")?;

        let malformed = || GitHubSourceError::MalformedResponse { resource: "commit" };

        // Both digests are validated and both stay typed. They become the
        // analysis' identity, and an identity assembled from unvalidated
        // strings is one that can be forged by a malformed response rather than
        // chosen. Keeping the tree as a `TreeSha` all the way to the wire DTO
        // is what stops it from being swapped with `sha` downstream.
        let sha = CommitSha::parse(&payload.sha).map_err(|_| malformed())?;
        let tree_sha = TreeSha::parse(&payload.commit.tree.sha).map_err(|_| malformed())?;

        Ok(ResolvedCommit {
            sha,
            tree_sha,
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

        let body = read_response_body(response).await?;
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

        // Refused on the declared length before anything is created, so the
        // common oversized case never touches the filesystem at all.
        if let Some(declared) = response.content_length()
            && declared > cap
        {
            return Err(archive_exceeded(cap, declared));
        }

        let (part, file) = create_part_file(destination).await?;

        // Everything after this point works on a file this call created. A
        // truncated tarball is not partial evidence — it is a file the extractor
        // would open and fail on later, somewhere with less context than here —
        // so it is removed rather than left to be found. `destination` is
        // deliberately not what gets removed: on a failure it still holds
        // whatever the caller had there, which was never this call's to delete.
        let outcome = match stream_to_file(response, cap, file).await {
            Ok(compressed_bytes) => tokio::fs::rename(&part, destination)
                .await
                .map(|()| ArchiveDownload { compressed_bytes })
                .map_err(|_| GitHubSourceError::Io {
                    operation: "move the archive file into place",
                }),
            Err(error) => Err(error),
        };

        if outcome.is_err() {
            drop(tokio::fs::remove_file(&part).await);
        }

        outcome
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
        header_u64, is_binary, is_rate_limited, rate_limit_error, redirect_target, tree_entry_kind,
    };
    use crate::{GitHubSourceError, TreeEntryKind};

    const EXAMPLE_TOKEN: &str = "EXAMPLE_NOT_A_REAL_TOKEN";

    fn client_for(api_base: &str) -> GitHubRestClient {
        GitHubRestClient::new(config_for(api_base)).expect("the test base is usable")
    }

    /// A configuration for `api_base`, with the loopback allowance a local base
    /// needs and a production base ignores.
    fn config_for(api_base: &str) -> GitHubClientConfig {
        GitHubClientConfig::new()
            .with_api_base(Url::parse(api_base).expect("test base is a literal"))
            .with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()))
            .allow_insecure_loopback()
    }

    fn url(value: &str) -> Url {
        Url::parse(value).expect("literal")
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
            Err(GitHubSourceError::InvalidApiBase { .. })
        ));
    }

    #[test]
    fn a_base_that_would_carry_the_token_in_cleartext_is_refused() {
        // The credential is attached to every request to the configured origin.
        // A plain-HTTP origin therefore means the analysis token on the wire in
        // cleartext, which no amount of care elsewhere in this file recovers
        // from.
        let insecure = GitHubClientConfig::new()
            .with_api_base(url("http://api.example.invalid/"))
            .with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()));

        assert!(matches!(
            GitHubRestClient::new(insecure),
            Err(GitHubSourceError::InvalidApiBase { .. })
        ));
    }

    #[test]
    fn plain_http_to_loopback_needs_the_development_option() {
        // Off by default, even for loopback: the suite has to ask for the
        // insecure path by name, so nothing acquires it by accident.
        let without = GitHubClientConfig::new().with_api_base(url("http://127.0.0.1:9001/"));
        assert!(matches!(
            GitHubRestClient::new(without),
            Err(GitHubSourceError::InvalidApiBase { .. })
        ));

        for local in [
            "http://127.0.0.1:9001/",
            "http://localhost:9001/",
            "http://[::1]:9001/",
        ] {
            assert!(
                GitHubRestClient::new(config_for(local)).is_ok(),
                "{local} is loopback and the option was given"
            );
        }

        // The option is loopback-only. A public host does not become acceptable
        // because a developer switched it on.
        let elsewhere = GitHubClientConfig::new()
            .with_api_base(url("http://api.example.invalid/"))
            .allow_insecure_loopback();
        assert!(matches!(
            GitHubRestClient::new(elsewhere),
            Err(GitHubSourceError::InvalidApiBase { .. })
        ));
    }

    #[test]
    fn a_base_carrying_a_secret_or_a_query_is_refused() {
        // Userinfo would be sent as basic auth beside the bearer token and
        // printed by anything that logs a URL; a query or fragment is silently
        // dropped when endpoints extend the path, so a base carrying either does
        // not mean what whoever configured it thinks it means.
        for base in [
            "https://user:hunter2@api.github.com/",
            "https://api.github.com/?access_token=secret",
            "https://api.github.com/#fragment",
        ] {
            let config = GitHubClientConfig::new().with_api_base(url(base));
            assert!(
                matches!(
                    GitHubRestClient::new(config),
                    Err(GitHubSourceError::InvalidApiBase { .. })
                ),
                "{base} was accepted"
            );
        }
    }

    #[test]
    fn debug_output_never_contains_a_credential_embedded_in_the_base() {
        // A configuration is printable before it is validated, so the redaction
        // cannot rely on construction having rejected this base already.
        let config = GitHubClientConfig::new()
            .with_api_base(url("https://user:hunter2@api.github.com/?token=secret"));

        let rendered = format!("{config:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(!rendered.contains("secret"), "{rendered}");
    }

    #[test]
    fn a_redirect_never_walks_the_request_off_tls() {
        let secure = url("https://api.github.com/repos/o/r/tarball/abc");

        assert!(matches!(
            redirect_target(
                302,
                Some("http://codeload.example.invalid/x"),
                &secure,
                false
            ),
            Err(GitHubSourceError::InsecureRedirect)
        ));
        // Not even under the development option: that option exists so a local
        // mock can speak HTTP, not so a production origin can be downgraded.
        assert!(matches!(
            redirect_target(302, Some("http://127.0.0.1:9001/x"), &secure, true),
            Err(GitHubSourceError::InsecureRedirect)
        ));

        // The hops that are allowed still are. A signed archive URL on another
        // HTTPS host is the whole reason redirects are followed at all.
        let followed = redirect_target(
            302,
            Some("https://codeload.example.invalid/x?token=signed"),
            &secure,
            false,
        )
        .expect("an HTTPS hop is followed");
        assert_eq!(
            followed.map(|next| next.to_string()),
            Some("https://codeload.example.invalid/x?token=signed".to_owned())
        );
    }

    #[test]
    fn a_local_redirect_may_not_leave_the_machine_in_cleartext() {
        // The development option is scoped the same way on a hop as it is on the
        // base, so a mock cannot redirect the suite — or a developer's token —
        // onto a public plain-HTTP host.
        let local = url("http://127.0.0.1:9001/repos/o/r");

        assert!(
            redirect_target(302, Some("http://127.0.0.1:9002/x"), &local, true)
                .expect("loopback to loopback is what the option is for")
                .is_some()
        );
        assert!(matches!(
            redirect_target(302, Some("http://codeload.example.invalid/x"), &local, true),
            Err(GitHubSourceError::InsecureRedirect)
        ));
    }

    #[test]
    fn a_response_that_is_not_a_redirect_has_nowhere_to_go() {
        // `304` is a `3xx` with no `Location`. Treating "is a redirect" as "has
        // somewhere to go" would turn a cache response into a malformed one.
        let current = url("https://api.github.com/repos/o/r");

        assert_eq!(
            redirect_target(304, None, &current, false).expect("not a redirect"),
            None
        );
        assert!(matches!(
            redirect_target(302, None, &current, false),
            Err(GitHubSourceError::MalformedResponse {
                resource: "redirect"
            })
        ));

        // `Location` is permitted to be relative, and GitHub's own sometimes is.
        let relative = redirect_target(301, Some("/repos/o/renamed"), &current, false)
            .expect("a relative location resolves")
            .expect("a redirect");
        assert_eq!(relative.as_str(), "https://api.github.com/repos/o/renamed");
    }

    #[test]
    fn a_forbidden_response_is_only_a_rate_limit_when_the_headers_say_so() {
        let exhausted = headers(&[(RATE_LIMIT_REMAINING_HEADER, "0")]);
        let plenty = headers(&[(RATE_LIMIT_REMAINING_HEADER, "4999")]);

        assert!(is_rate_limited(reqwest::StatusCode::FORBIDDEN, &exhausted));
        // A plain refusal. Retrying this would spend the remaining budget on a
        // request that will never be allowed.
        assert!(!is_rate_limited(reqwest::StatusCode::FORBIDDEN, &plenty));
        assert!(!is_rate_limited(reqwest::StatusCode::NOT_FOUND, &exhausted));
    }

    #[test]
    fn every_too_many_requests_is_a_rate_limit_whatever_its_headers_say() {
        // GitHub documents `429` for both limits and defines the header-less
        // case — wait at least a minute. A `429` is therefore never a permanent
        // refusal, and the headers only ever add a timing hint.
        for map in [
            headers(&[]),
            headers(&[(RATE_LIMIT_REMAINING_HEADER, "4999")]),
            headers(&[(RATE_LIMIT_REMAINING_HEADER, "0")]),
        ] {
            assert!(is_rate_limited(
                reqwest::StatusCode::TOO_MANY_REQUESTS,
                &map
            ));
        }
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
