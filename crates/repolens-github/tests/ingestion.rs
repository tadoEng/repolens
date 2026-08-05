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

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use repolens_core::{CommitSha, RepositoryCoordinate};
use repolens_github::{
    GITHUB_REST_API_VERSION, GitHubClientConfig, GitHubRepositorySource, GitHubRestClient,
    GitHubSourceError, SkipReason, limits,
};
use secrecy::SecretString;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
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
    let config = GitHubClientConfig::new()
        .with_api_base(Url::parse(&server.uri()).expect("the mock server reports a valid URL"))
        .with_token(SecretString::from(EXAMPLE_TOKEN.to_owned()));
    GitHubRestClient::new(config).expect("a http base is usable")
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
