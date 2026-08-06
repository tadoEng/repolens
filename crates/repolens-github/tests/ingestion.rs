//! Ingestion against a local mock of GitHub.
//!
//! An integration test rather than a unit test, deliberately: from out here only
//! the public API exists, so a test that compiles is also evidence that GitHub's
//! JSON never escaped the crate.
//!
//! No test reaches the network. A suite that talks to GitHub fails for reasons
//! unrelated to the code it is testing, and the two behaviours that matter most
//! here — rate-limit exhaustion and a cross-host redirect — cannot be provoked
//! against the real API on purpose anyway.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use repolens_core::{CommitSha, RepositoryCoordinate};
use repolens_github::{
    GITHUB_REST_API_VERSION, GitHubClientConfig, GitHubRepositorySource, GitHubRestClient,
    GitHubSourceError, SkipReason, limits,
};
use secrecy::SecretString;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Named so that a scanner, or a reviewer skimming a diff, is never in doubt.
const EXAMPLE_TOKEN: &str = "EXAMPLE_NOT_A_REAL_TOKEN";

const COMMIT_SHA: &str = "0584a2df65968a4e9e6859ef46bbed430408a3f1";
const TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const README_BLOB_SHA: &str = "1111111111111111111111111111111111111111";
const BIG_BLOB_SHA: &str = "2222222222222222222222222222222222222222";
const IMAGE_BLOB_SHA: &str = "3333333333333333333333333333333333333333";
const COPY_BLOB_SHA: &str = "4444444444444444444444444444444444444444";

const README_BYTES: &[u8] = b"# RepoLens\n\nEvidence, not vibes.\n";

fn coordinate() -> RepositoryCoordinate {
    RepositoryCoordinate::new("tadoEng", "repolens")
}

fn commit() -> CommitSha {
    CommitSha::parse(COMMIT_SHA).expect("a literal digest")
}

/// A client pointed at `server`, carrying a credential scoped to it.
fn client(server: &MockServer) -> GitHubRestClient {
    client_at(&server.uri())
}

/// A client pointed at any local base.
///
/// The mock speaks plain HTTP, which the boundary refuses unless it is asked
/// for by name — see
/// [`allow_insecure_loopback`](GitHubClientConfig::allow_insecure_loopback).
/// That the whole suite has to opt in is the point: production configuration
/// cannot reach this path by omission.
fn client_at(api_base: &str) -> GitHubRestClient {
    let config = GitHubClientConfig::new()
        .with_api_base(Url::parse(api_base).expect("the local base is a valid URL"))
        .with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()))
        .allow_insecure_loopback();
    GitHubRestClient::new(config).expect("a loopback base is usable under the option")
}

/// A path in the system temporary directory that no other test will collide
/// with.
fn scratch_path(label: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "repolens-{label}-{}-{unique}.tar.gz",
        std::process::id()
    ))
}

/// Files sitting beside `path` whose names begin with its own.
///
/// The archive is written to a sibling temporary file, so this is what proves
/// the temporary file was cleaned up rather than merely renamed out of the way.
fn siblings_of(path: &Path) -> Vec<String> {
    let directory = path.parent().expect("a scratch path has a parent");
    let prefix = path
        .file_name()
        .expect("a scratch path names a file")
        .to_string_lossy()
        .into_owned();

    std::fs::read_dir(directory)
        .expect("the temporary directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&prefix) && *name != prefix)
        .collect()
}

/// A one-shot HTTP server whose body arrives without a declared length.
///
/// Exists because a canned-response mock cannot produce one: `Content-Length` is
/// what such a mock knows best, and the failure that matters here is the one
/// that only appears *after* the first byte — a cap enforced against a body that
/// is still arriving. Returns the base URL; the server answers any path.
async fn undeclared_length_server(chunk: Vec<u8>, chunks: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port is available");
    let address = listener.local_addr().expect("the listener is bound");

    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };

        // Enough of the request to know it arrived; a GET has no body worth
        // draining, and the response does not depend on it.
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).await;

        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await;
        for _ in 0..chunks {
            let _ = stream
                .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                .await;
            let _ = stream.write_all(&chunk).await;
            let _ = stream.write_all(b"\r\n").await;
        }
        // No terminating chunk on purpose: the cap trips first, and a client
        // that insisted on a well-formed ending would hang instead of failing.
        let _ = stream.flush().await;
    });

    format!("http://{address}")
}

fn repository_body() -> serde_json::Value {
    json!({
        "full_name": "tadoEng/repolens",
        "default_branch": "master",
        "archived": false,
        "size": 1234,
        "private": false,
    })
}

fn commit_body() -> serde_json::Value {
    json!({
        "sha": COMMIT_SHA,
        "commit": {
            "tree": { "sha": TREE_SHA },
            "committer": { "date": "2026-08-04T19:58:17Z" },
        },
    })
}

/// A tree holding one of every case the retrieval budget has to decide about.
fn tree_body(truncated: bool) -> serde_json::Value {
    json!({
        "sha": TREE_SHA,
        "truncated": truncated,
        "tree": [
            { "path": "README.md", "sha": README_BLOB_SHA, "type": "blob", "size": README_BYTES.len() },
            { "path": "docs/README.md", "sha": COPY_BLOB_SHA, "type": "blob", "size": README_BYTES.len() },
            { "path": "generated.rs", "sha": BIG_BLOB_SHA, "type": "blob", "size": limits::MAX_FILE_BYTES + 1 },
            { "path": "logo.png", "sha": IMAGE_BLOB_SHA, "type": "blob", "size": 8 },
            { "path": "src", "sha": TREE_SHA, "type": "tree" },
            { "path": "web/vendored", "sha": COMMIT_SHA, "type": "commit" },
        ],
    })
}

/// Mounts the endpoints every happy-path test needs.
async fn mount_repository(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(repository_body()))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/commits/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit_body()))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/git/trees/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_body(false)))
        .mount(server)
        .await;

    for (blob_sha, bytes) in [
        (README_BLOB_SHA, README_BYTES.to_vec()),
        (COPY_BLOB_SHA, README_BYTES.to_vec()),
        // A PNG signature: the `NUL` bytes are what make it binary.
        (IMAGE_BLOB_SHA, b"\x89PNG\r\n\x1a\n\x00\x00".to_vec()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/tadoEng/repolens/git/blobs/{blob_sha}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(server)
            .await;
    }
}

/// Every request the mock saw carried the pinned version header.
async fn assert_version_header_everywhere(server: &MockServer, expected_requests: usize) {
    let requests = server
        .received_requests()
        .await
        .expect("the mock records requests by default");

    assert_eq!(
        requests.len(),
        expected_requests,
        "the number of requests is itself part of the budget"
    );

    for request in &requests {
        let sent = request
            .headers
            .get("x-github-api-version")
            .and_then(|value| value.to_str().ok());

        assert_eq!(
            sent,
            Some(GITHUB_REST_API_VERSION),
            "{} was sent without the pinned API version; GitHub would have \
             answered it under the older default instead of failing",
            request.url.path()
        );
    }
}

#[tokio::test]
async fn every_request_carries_the_pinned_api_version() {
    // Asserted across all five operations rather than on one, because the header
    // is the kind of thing that gets added where it was noticed and forgotten
    // where it was not — and a missing one downgrades the API silently rather
    // than failing.
    let server = MockServer::start().await;
    mount_repository(&server).await;

    let archive = scratch_path("version");
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/tarball/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 64]))
        .mount(&server)
        .await;

    let client = client(&server);

    client
        .resolve_repository(&coordinate())
        .await
        .expect("the repository resolves");
    client
        .resolve_commit(&coordinate(), COMMIT_SHA)
        .await
        .expect("the commit resolves");
    client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");
    client
        .fetch_selected_blobs(&coordinate(), &commit(), &["README.md".to_owned()])
        .await
        .expect("the blob is fetched");
    client
        .download_archive(&coordinate(), &commit(), 4096, &archive)
        .await
        .expect("the archive downloads");

    // Five calls, six requests: `fetch_selected_blobs` fetches a tree of its own
    // before it can address content by blob SHA.
    assert_version_header_everywhere(&server, 6).await;

    drop(std::fs::remove_file(&archive));
}

#[tokio::test]
async fn a_truncated_tree_is_data_rather_than_a_failure() {
    // The whole reason ingestion is REST and not GraphQL. If truncation were an
    // error, a repository too large to list would produce no report instead of a
    // report that says what it could not see.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/git/trees/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_body(true)))
        .mount(&server)
        .await;

    let tree = client(&server)
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("truncation is never an error");

    assert!(tree.truncated);
    assert_eq!(tree.sha, TREE_SHA);
    // The entries GitHub *did* return are still evidence.
    assert_eq!(tree.entries.len(), 6);
}

#[tokio::test]
async fn rate_limit_exhaustion_carries_the_reset_instant() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-limit", "60")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1767225600"),
        )
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("an exhausted limit is a failure");

    assert!(
        error.is_retryable(),
        "a rate limit must be retryable, or the worker discards an analysis that \
         would have succeeded twenty minutes later"
    );

    match error {
        GitHubSourceError::RateLimited {
            reset_at,
            retry_after_seconds,
        } => {
            assert_eq!(
                reset_at.map(time::OffsetDateTime::unix_timestamp),
                Some(1_767_225_600),
                "the reset instant is what makes the retry measured rather than guessed"
            );
            // The primary limit sends no `retry-after`; absent is the honest
            // answer, not a number we invented.
            assert_eq!(retry_after_seconds, None);
        }
        other => panic!("expected a typed rate limit, got {other:?}"),
    }
}

#[tokio::test]
async fn a_plain_forbidden_is_not_mistaken_for_a_rate_limit() {
    // `403` with budget remaining will not improve by waiting. Retrying it would
    // spend the rest of the window on a request that can never be allowed.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(403).insert_header("x-ratelimit-remaining", "58"))
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("a refusal is a failure");

    assert!(matches!(
        error,
        GitHubSourceError::UnexpectedStatus { status: 403 }
    ));
    assert!(!error.is_retryable());
}

#[tokio::test]
async fn the_per_analysis_byte_budget_stops_retrieval_and_says_so() {
    // Files that are individually fine and collectively are not. The budget is
    // charged before each request, so it is a ceiling rather than a ceiling plus
    // one more file — and the file that did not fit is named rather than
    // silently missing.
    let per_file = limits::MAX_FILE_BYTES;
    let fits = usize::try_from(limits::MAX_TOTAL_FILE_BYTES / per_file).expect("a small count");

    let server = MockServer::start().await;
    let entries: Vec<serde_json::Value> = (0..=fits)
        .map(|index| {
            json!({
                "path": format!("bulk/file_{index}.rs"),
                "sha": format!("{:040x}", 0xa0 + index),
                "type": "blob",
                "size": per_file,
            })
        })
        .collect();

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/git/trees/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": TREE_SHA,
            "truncated": false,
            "tree": entries,
        })))
        .mount(&server)
        .await;

    let body = vec![b'a'; usize::try_from(per_file).expect("a file that fits in memory")];
    for index in 0..=fits {
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/tadoEng/repolens/git/blobs/{:040x}",
                0xa0 + index
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
    }

    let client = client(&server);
    let tree = client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");
    let requested: Vec<String> = tree
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();

    let selection = client
        .collect_blobs(&coordinate(), &tree, &requested)
        .await
        .expect("an exhausted budget is a limitation, not a failure");

    assert_eq!(selection.retrieved.len(), fits);
    assert_eq!(
        selection.skipped,
        vec![repolens_github::SkippedPath {
            path: format!("bulk/file_{fits}.rs"),
            reason: SkipReason::BudgetSpent {
                limit_bytes: limits::MAX_TOTAL_FILE_BYTES,
            },
        }]
    );

    // The file that did not fit cost no request either.
    let blob_requests = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().contains("/git/blobs/"))
        .count();
    assert_eq!(blob_requests, fits);
}

#[tokio::test]
async fn binary_files_cannot_buy_more_requests_than_the_ceiling_allows() {
    // The ceiling is a ceiling on what one analysis costs GitHub, so it has to
    // count attempts. A binary file is judged only after it has been downloaded
    // and never joins `retrieved`, so a count of successful results would let a
    // caller submitting nothing but binary paths request every one of them.
    const REQUESTED: usize = limits::MAX_SELECTED_FILES * 4;

    let server = MockServer::start().await;
    let entries: Vec<serde_json::Value> = (0..REQUESTED)
        .map(|index| {
            json!({
                "path": format!("assets/image_{index:04}.png"),
                "sha": format!("{:040x}", index),
                "type": "blob",
                "size": 8,
            })
        })
        .collect();

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/git/trees/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": TREE_SHA,
            "truncated": false,
            "tree": entries,
        })))
        .mount(&server)
        .await;

    // One matcher for every blob: which SHA was asked for does not matter here,
    // only how many times anything was.
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/repos/tadoEng/repolens/git/blobs/[0-9a-f]{40}$",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(b"\x89PNG\r\n\x1a\n\x00\x00".to_vec()),
        )
        .mount(&server)
        .await;

    let client = client(&server);
    let tree = client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");
    let requested: Vec<String> = tree
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();

    let selection = client
        .collect_blobs(&coordinate(), &tree, &requested)
        .await
        .expect("skipping is never a failure");

    let blob_requests = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().contains("/git/blobs/"))
        .count();
    assert_eq!(
        blob_requests,
        limits::MAX_SELECTED_FILES,
        "a caller must not be able to buy more GitHub requests than the ceiling"
    );

    assert!(selection.retrieved.is_empty(), "every file was binary");
    // Every requested path is accounted for: the ones that were fetched and
    // found binary, and the ones the ceiling stopped.
    assert_eq!(selection.skipped.len(), REQUESTED);
    assert_eq!(
        selection
            .skipped
            .iter()
            .filter(|skipped| skipped.reason == SkipReason::Binary)
            .count(),
        limits::MAX_SELECTED_FILES
    );
    assert_eq!(
        selection
            .skipped
            .iter()
            .filter(|skipped| skipped.reason
                == SkipReason::SelectionFull {
                    limit: limits::MAX_SELECTED_FILES
                })
            .count(),
        REQUESTED - limits::MAX_SELECTED_FILES
    );
}

#[tokio::test]
async fn a_repeated_path_is_requested_once() {
    // A caller's list is not guaranteed to be a set. Without deduplication the
    // same file could be paid for as many times as it appears, and its bytes
    // charged to the budget again on each.
    let server = MockServer::start().await;
    mount_repository(&server).await;
    let client = client(&server);

    let tree = client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");

    let selection = client
        .collect_blobs(
            &coordinate(),
            &tree,
            &[
                "README.md".to_owned(),
                "README.md".to_owned(),
                "README.md".to_owned(),
            ],
        )
        .await
        .expect("a repeated path is not a failure");

    assert_eq!(selection.retrieved.len(), 1);
    assert!(selection.skipped.is_empty());

    let blob_requests = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().contains("/git/blobs/"))
        .count();
    assert_eq!(blob_requests, 1);
}

#[tokio::test]
async fn a_file_github_did_not_measure_cannot_overshoot_the_analysis_budget() {
    // The case the previous implementation admitted to: GitHub omits `size` for
    // entries it does not weigh, so nothing could be charged before the request
    // and the body was read against the per-file cap instead of what was left of
    // the analysis. The budget could then be exceeded by a whole file.
    const BODY_BYTES: u64 = 700 * 1024;

    let fits = usize::try_from(limits::MAX_TOTAL_FILE_BYTES / BODY_BYTES).expect("a small count");
    let requested_count = fits + 1;

    let server = MockServer::start().await;
    let entries: Vec<serde_json::Value> = (0..requested_count)
        .map(|index| {
            // No `size`: this is what GitHub returns for an entry it did not
            // measure, and the whole point of the case.
            json!({
                "path": format!("bulk/unmeasured_{index}.rs"),
                "sha": format!("{:040x}", 0xb0 + index),
                "type": "blob",
            })
        })
        .collect();

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/git/trees/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": TREE_SHA,
            "truncated": false,
            "tree": entries,
        })))
        .mount(&server)
        .await;

    let body = vec![b'a'; usize::try_from(BODY_BYTES).expect("a file that fits in memory")];
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/repos/tadoEng/repolens/git/blobs/[0-9a-f]{40}$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;

    let client = client(&server);
    let tree = client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");
    let requested: Vec<String> = tree
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();

    let selection = client
        .collect_blobs(&coordinate(), &tree, &requested)
        .await
        .expect("an exhausted budget is a limitation, not a failure");

    let retrieved_bytes: u64 = selection
        .retrieved
        .iter()
        .map(|blob| u64::try_from(blob.bytes.len()).expect("a bounded length"))
        .sum();
    assert!(
        retrieved_bytes <= limits::MAX_TOTAL_FILE_BYTES,
        "retrieved {retrieved_bytes} bytes against a ceiling of {}",
        limits::MAX_TOTAL_FILE_BYTES
    );
    assert_eq!(selection.retrieved.len(), fits);

    // The last file is under the per-file cap and would have been read in full
    // had the cap been the only bound. It is refused against what was left of
    // the analysis instead, and says so: this file is not too large, the budget
    // is spent.
    assert_eq!(
        selection.skipped,
        vec![repolens_github::SkippedPath {
            path: format!("bulk/unmeasured_{fits}.rs"),
            reason: SkipReason::BudgetSpent {
                limit_bytes: limits::MAX_TOTAL_FILE_BYTES,
            },
        }]
    );
}

#[tokio::test]
async fn a_headerless_too_many_requests_is_retriable() {
    // GitHub documents the secondary rate limit as `403` or `429`, and defines
    // what to do when it sends no timing hint at all: wait at least a minute.
    // Classifying that as permanent discards an analysis that would have
    // succeeded, without ever retrying it.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("a rate limit is a failure");

    assert!(
        error.is_retryable(),
        "a 429 without headers must still be retriable, got {error:?}"
    );
    match error {
        GitHubSourceError::RateLimited {
            retry_after_seconds,
            reset_at,
        } => {
            // Absent rather than invented. The caller's own minimum wait is the
            // honest answer when GitHub named no instant.
            assert_eq!(retry_after_seconds, None);
            assert_eq!(reset_at, None);
        }
        other => panic!("expected a typed rate limit, got {other:?}"),
    }
}

#[tokio::test]
async fn an_insecure_base_is_refused_before_any_request() {
    // The credential is attached to every request to the configured origin, so
    // a plain-HTTP origin means the analysis token in cleartext. Refused at
    // construction: a deployment mistake should stop a process at startup, not
    // be discovered from a packet capture.
    let config = GitHubClientConfig::new()
        .with_api_base(Url::parse("http://api.example.invalid/").expect("literal"))
        .with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()));

    assert!(matches!(
        GitHubRestClient::new(config),
        Err(GitHubSourceError::InvalidApiBase { .. })
    ));
}

#[tokio::test]
async fn a_redirect_onto_a_public_cleartext_host_is_refused() {
    // The development option is scoped to loopback on every hop, not only on
    // the base. Its production twin is the HTTPS to HTTP downgrade, which the
    // same rule refuses and which no local mock can stage.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", "http://codeload.example.invalid/repos"),
        )
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("a cleartext hop off the machine is refused");

    assert!(matches!(error, GitHubSourceError::InsecureRedirect));
    assert!(!error.is_retryable());

    // Refused before it was followed: the second request was never sent, and
    // nothing was resolved against a host that could rewrite the answer.
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 1);
}

#[tokio::test]
async fn every_unread_file_is_recorded_with_a_reason() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    let client = client(&server);

    let tree = client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");

    let requested = [
        "README.md".to_owned(),
        "generated.rs".to_owned(),
        "logo.png".to_owned(),
        "src".to_owned(),
        "web/vendored".to_owned(),
        "never-existed.md".to_owned(),
    ];

    let selection = client
        .collect_blobs(&coordinate(), &tree, &requested)
        .await
        .expect("skipping is never a failure");

    assert_eq!(selection.retrieved.len(), 1);
    assert_eq!(selection.retrieved[0].path, "README.md");
    assert_eq!(selection.retrieved[0].bytes, README_BYTES);

    let reasons: Vec<(&str, &SkipReason)> = selection
        .skipped
        .iter()
        .map(|skipped| (skipped.path.as_str(), &skipped.reason))
        .collect();

    assert_eq!(
        reasons,
        vec![
            (
                "generated.rs",
                &SkipReason::TooLarge {
                    size_bytes: limits::MAX_FILE_BYTES + 1,
                    limit_bytes: limits::MAX_FILE_BYTES,
                }
            ),
            ("logo.png", &SkipReason::Binary),
            // A directory.
            ("src", &SkipReason::NotAFile),
            // A submodule: its contents belong to another repository.
            ("web/vendored", &SkipReason::NotAFile),
            ("never-existed.md", &SkipReason::NotInTree),
        ]
    );

    // The oversized file cost no request: its size was known from the tree.
    let fetched: Vec<String> = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .map(|request| request.url.path().to_owned())
        .filter(|path| path.contains("/git/blobs/"))
        .collect();
    assert_eq!(
        fetched,
        vec![
            format!("/repos/tadoEng/repolens/git/blobs/{README_BLOB_SHA}"),
            format!("/repos/tadoEng/repolens/git/blobs/{IMAGE_BLOB_SHA}"),
        ]
    );
}

#[tokio::test]
async fn identical_content_yields_an_identical_digest() {
    // Two paths, one content. If the digest varied with anything but the bytes,
    // "the same commit produced the same evidence" would be unverifiable.
    let server = MockServer::start().await;
    mount_repository(&server).await;
    let client = client(&server);

    let tree = client
        .fetch_tree(&coordinate(), &commit())
        .await
        .expect("the tree is listed");

    let first = client
        .collect_blobs(
            &coordinate(),
            &tree,
            &["README.md".to_owned(), "docs/README.md".to_owned()],
        )
        .await
        .expect("both are readable");

    assert_eq!(
        first.retrieved[0].content_digest,
        first.retrieved[1].content_digest
    );
    // Different Git objects, same content: the two digests answer different
    // questions and are both worth keeping.
    assert_ne!(first.retrieved[0].sha, first.retrieved[1].sha);

    let second = client
        .collect_blobs(&coordinate(), &tree, &["README.md".to_owned()])
        .await
        .expect("still readable");
    assert_eq!(
        first.retrieved[0].content_digest, second.retrieved[0].content_digest,
        "a second ingestion at the same commit must yield the same evidence identity"
    );
}

#[tokio::test]
async fn a_cross_host_redirect_never_carries_the_credential() {
    // The control section 3 of the amendment calls load-bearing. `reqwest`
    // strips sensitive headers cross-origin today; this test is what would fail
    // if a future version stopped, instead of the token quietly reaching a host
    // that never needed it.
    let api = MockServer::start().await;
    let codeload = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repolens/tarball"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x1f, 0x8b, 0x08, 0x00]))
        .mount(&codeload)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/tarball/{COMMIT_SHA}"
        )))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/repolens/tarball", codeload.uri())),
        )
        .mount(&api)
        .await;

    let archive = scratch_path("cross-host");
    let download = client(&api)
        .download_archive(&coordinate(), &commit(), 4096, &archive)
        .await
        .expect("the redirect is followed");

    assert_eq!(download.compressed_bytes, 4);

    let credentialed = api.received_requests().await.expect("recorded");
    assert!(
        credentialed[0].headers.contains_key("authorization"),
        "the credentialed origin must still be credentialed"
    );

    let crossed = codeload.received_requests().await.expect("recorded");
    assert_eq!(crossed.len(), 1);
    assert!(
        !crossed[0].headers.contains_key("authorization"),
        "the archive host received the analysis token"
    );
    // The version header is not a credential and is not origin-scoped.
    assert!(crossed[0].headers.contains_key("x-github-api-version"));

    drop(std::fs::remove_file(&archive));
}

#[tokio::test]
async fn a_same_host_redirect_keeps_the_credential() {
    // The other half of the policy. Stripping unconditionally would break a
    // renamed repository, which GitHub answers with a `301` to itself — so
    // "strip everything" is not a safe simplification.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/repos/tadoEng/renamed", server.uri()),
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/renamed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name": "tadoEng/renamed",
            "default_branch": "main",
            "archived": true,
            "size": 10,
            "private": false,
        })))
        .mount(&server)
        .await;

    let resolved = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect("a rename is followed");

    // Recorded under the name it now has, taken from the response rather than
    // from the request.
    assert_eq!(resolved.coordinate.name, "renamed");
    assert!(resolved.archived);

    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert!(
            request.headers.contains_key("authorization"),
            "{} lost the credential on a same-origin hop",
            request.url.path()
        );
    }
}

#[tokio::test]
async fn a_redirect_loop_costs_a_bounded_number_of_requests() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            format!("{}/repos/tadoEng/repolens", server.uri()),
        ))
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("a loop cannot resolve");

    assert!(matches!(
        error,
        GitHubSourceError::LimitExceeded {
            limit_name: "redirect hops",
            ..
        }
    ));

    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(
        requests.len(),
        usize::from(limits::MAX_REDIRECT_HOPS) + 1,
        "a redirect loop must cost a bounded number of requests, not a rate-limit window"
    );
}

#[tokio::test]
async fn an_oversized_archive_leaves_nothing_behind() {
    // A truncated tarball is not partial evidence, it is a file the extractor
    // would open and fail on somewhere with less context than here.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/tarball/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 4096]))
        .mount(&server)
        .await;

    let archive = scratch_path("oversized");
    let error = client(&server)
        .download_archive(&coordinate(), &commit(), 128, &archive)
        .await
        .expect_err("the cap is enforced");

    match error {
        GitHubSourceError::LimitExceeded {
            limit_name,
            limit,
            observed,
        } => {
            assert_eq!(limit_name, "archive compressed bytes");
            // The caller's budget, not the boundary's ceiling: the smaller of
            // the two wins.
            assert_eq!(limit, 128);
            assert_eq!(observed, 4096);
        }
        other => panic!("expected a limit breach, got {other:?}"),
    }

    assert!(
        !archive.exists(),
        "a rejected archive must not be left on disk"
    );
    assert!(
        siblings_of(&archive).is_empty(),
        "the temporary file the download wrote to must not survive it"
    );
}

#[tokio::test]
async fn a_declared_oversize_archive_never_touches_a_file_it_did_not_create() {
    // The destination is a caller-chosen path, and a caller that reuses one is
    // not doing anything wrong. Opening it for writing truncates it, and the
    // cleanup that follows any failure would then delete a file this analysis
    // never created — losing data on a path that only ever had permission to
    // add one.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/tarball/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 4096]))
        .mount(&server)
        .await;

    let archive = scratch_path("pre-existing-declared");
    let precious = b"not this analysis' file to delete\n";
    std::fs::write(&archive, precious).expect("the canary is written");

    let error = client(&server)
        .download_archive(&coordinate(), &commit(), 128, &archive)
        .await
        .expect_err("the cap is enforced");
    assert!(matches!(error, GitHubSourceError::LimitExceeded { .. }));

    assert_eq!(
        std::fs::read(&archive).expect("the canary still exists"),
        precious,
        "a failed download deleted or truncated a file it did not create"
    );
    assert!(siblings_of(&archive).is_empty());

    drop(std::fs::remove_file(&archive));
}

#[tokio::test]
async fn a_mid_stream_failure_never_touches_a_file_it_did_not_create() {
    // The other half, and the worse one: here the bytes really do start
    // arriving, so a download writing straight to the destination would have
    // truncated it before discovering that the body did not fit.
    let base = undeclared_length_server(vec![0x1f; 64], 8).await;

    let archive = scratch_path("pre-existing-stream");
    let precious = b"not this analysis' file to delete\n";
    std::fs::write(&archive, precious).expect("the canary is written");

    let error = client_at(&base)
        .download_archive(&coordinate(), &commit(), 128, &archive)
        .await
        .expect_err("a body that outgrows the cap is refused");

    match error {
        GitHubSourceError::LimitExceeded {
            limit_name,
            limit,
            observed,
        } => {
            assert_eq!(limit_name, "archive compressed bytes");
            assert_eq!(limit, 128);
            // Cut at the first chunk that crossed the cap, rather than after
            // the whole body had been written and measured.
            assert_eq!(observed, 192);
        }
        other => panic!("expected a limit breach, got {other:?}"),
    }

    assert_eq!(
        std::fs::read(&archive).expect("the canary still exists"),
        precious,
        "a failed download deleted or truncated a file it did not create"
    );
    assert!(
        siblings_of(&archive).is_empty(),
        "the partial download must not be left beside the destination"
    );

    drop(std::fs::remove_file(&archive));
}

#[tokio::test]
async fn a_successful_archive_replaces_the_destination() {
    // The counterpart of the two canaries: writing elsewhere first must still
    // leave the bytes at the path the caller asked for, and leave nothing else.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/tadoEng/repolens/tarball/{COMMIT_SHA}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x1f, 0x8b, 0x08, 0x00]))
        .mount(&server)
        .await;

    let archive = scratch_path("replaced");
    std::fs::write(&archive, b"an older download").expect("the previous file is written");

    let download = client(&server)
        .download_archive(&coordinate(), &commit(), 4096, &archive)
        .await
        .expect("the archive downloads");

    assert_eq!(download.compressed_bytes, 4);
    assert_eq!(
        std::fs::read(&archive).expect("the archive is on disk"),
        vec![0x1f, 0x8b, 0x08, 0x00]
    );
    assert!(siblings_of(&archive).is_empty());

    drop(std::fs::remove_file(&archive));
}

#[tokio::test]
async fn a_missing_repository_is_reported_without_echoing_the_request() {
    let server = MockServer::start().await;
    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("the mock answers 404 for anything unmounted");

    assert!(matches!(
        error,
        GitHubSourceError::RepositoryNotFound(ref found) if found == &coordinate()
    ));

    let rendered = error.to_string();
    assert!(rendered.contains("tadoEng/repolens"));
    assert!(!rendered.contains(EXAMPLE_TOKEN), "{rendered}");
    assert!(!rendered.contains("http"), "{rendered}");
}

#[tokio::test]
async fn a_private_repository_is_indistinguishable_from_an_absent_one() {
    // Otherwise the answer would depend on whether a token happened to be
    // configured, and would confirm the existence of a repository the submitter
    // is not entitled to know about.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name": "tadoEng/repolens",
            "default_branch": "main",
            "archived": false,
            "size": 10,
            "private": true,
        })))
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("a private repository is not analyzable");

    assert!(matches!(error, GitHubSourceError::RepositoryNotFound(_)));
}

#[tokio::test]
async fn a_repository_too_large_to_analyze_is_refused_before_any_download() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name": "tadoEng/repolens",
            "default_branch": "main",
            "archived": false,
            "size": limits::MAX_REPOSITORY_KILOBYTES + 1,
            "private": false,
        })))
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_repository(&coordinate())
        .await
        .expect_err("the ceiling is enforced");

    assert!(matches!(
        error,
        GitHubSourceError::LimitExceeded {
            limit_name: "repository kilobytes",
            observed,
            ..
        } if observed == limits::MAX_REPOSITORY_KILOBYTES + 1
    ));
}

#[tokio::test]
async fn a_reference_resolves_to_an_exact_commit_and_its_tree() {
    // Never analyze a moving branch name without recording what it resolved to.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens/commits/master"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit_body()))
        .mount(&server)
        .await;

    let resolved = client(&server)
        .resolve_commit(&coordinate(), "master")
        .await
        .expect("the branch resolves");

    assert_eq!(resolved.sha.as_str(), COMMIT_SHA);
    assert_eq!(resolved.tree_sha, TREE_SHA);
    // 2026-08-04T19:58:17Z, the timestamp the fixture carries.
    assert_eq!(resolved.committed_at.unix_timestamp(), 1_785_873_497);
}

#[tokio::test]
async fn a_commit_response_that_is_not_a_digest_is_refused() {
    // The commit SHA becomes the analysis' identity. Accepting whatever arrived
    // would let a malformed response choose it.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/tadoEng/repolens/commits/master"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": "HEAD",
            "commit": {
                "tree": { "sha": TREE_SHA },
                "committer": { "date": "2026-08-04T19:58:17Z" },
            },
        })))
        .mount(&server)
        .await;

    let error = client(&server)
        .resolve_commit(&coordinate(), "master")
        .await
        .expect_err("an abbreviated or symbolic name is not an identity");

    assert!(matches!(
        error,
        GitHubSourceError::MalformedResponse { resource: "commit" }
    ));
}
