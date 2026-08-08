//! The production composition path, end to end, against a real archive.
//!
//! Everything below the pipeline is already covered on its own: extraction has
//! its ceilings tested, counting has its languages and its exclusion ledger,
//! and the projection has its orderings. What none of them prove is that the
//! *path* exists — that a commit archive becomes a `composition` section on a
//! report, and that a ceiling becomes a limitation instead of a silent null.
//!
//! The archives here are built with the same `tar` and `flate2` the extractor
//! reads with, rather than checked in as binaries. A committed fixture archive
//! would be a second definition of what extraction accepts, and it would drift
//! without anything failing.

use std::io::Write as _;
use std::path::Path;

use repolens_core::{CommitSha, RepositoryCoordinate};
use repolens_github::{
    ArchiveDownload, BlobSelection, GitHubRepositorySource, GitHubSourceError, RepositoryTree,
    ResolvedCommit, ResolvedRepository,
};
use repolens_server::infrastructure::composition::{self, Composed};

/// GitHub wraps an archive in a single top-level directory. Extraction strips
/// it, and a fixture that omitted it would be testing a shape GitHub never
/// sends.
const PREFIX: &str = "owner-repo-0584a2d";

fn commit() -> CommitSha {
    CommitSha::parse("0584a2df65968a4e9e6859ef46bbed430408a3f1").expect("a literal digest")
}

/// Writes a gzip'd tar of `files` to `destination`.
fn write_archive(destination: &Path, files: &[(&str, &str)]) {
    let file = std::fs::File::create(destination).expect("a scratch archive");
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);

    for (path, contents) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("{PREFIX}/{path}"), contents.as_bytes())
            .expect("appending to a scratch archive");
    }

    builder
        .into_inner()
        .expect("finishing the tar")
        .finish()
        .expect("finishing the gzip");
}

/// A source that serves one archive and nothing else.
///
/// Only `download_archive` is reachable from `compose`; the rest of the trait
/// is unreachable here on purpose, so a change that made composition fetch
/// something else fails loudly rather than silently passing.
struct ArchiveSource {
    files: Vec<(&'static str, &'static str)>,
    /// Bytes the caller is allowed to write, so the compressed ceiling can be
    /// driven from the test rather than by building a 4 MiB fixture.
    ceiling: Option<u64>,
}

impl GitHubRepositorySource for ArchiveSource {
    async fn resolve_repository(
        &self,
        _coordinate: &RepositoryCoordinate,
    ) -> Result<ResolvedRepository, GitHubSourceError> {
        unreachable!("composition resolves nothing")
    }

    async fn resolve_commit(
        &self,
        _coordinate: &RepositoryCoordinate,
        _reference: &str,
    ) -> Result<ResolvedCommit, GitHubSourceError> {
        unreachable!("composition resolves nothing")
    }

    async fn fetch_tree(
        &self,
        _coordinate: &RepositoryCoordinate,
        _commit: &CommitSha,
    ) -> Result<RepositoryTree, GitHubSourceError> {
        unreachable!("composition reads an archive, never a tree")
    }

    async fn collect_selected_blobs(
        &self,
        _coordinate: &RepositoryCoordinate,
        _tree: &RepositoryTree,
        _paths: &[String],
    ) -> Result<BlobSelection, GitHubSourceError> {
        unreachable!("composition reads an archive, never a blob")
    }

    async fn download_archive(
        &self,
        _coordinate: &RepositoryCoordinate,
        _commit: &CommitSha,
        max_compressed_bytes: u64,
        destination: &Path,
    ) -> Result<ArchiveDownload, GitHubSourceError> {
        write_archive(destination, &self.files);
        let written = std::fs::metadata(destination)
            .expect("the archive exists")
            .len();

        // The real client refuses mid-stream; refusing here after writing is
        // the same outcome for the caller and keeps the fixture honest about
        // the numbers it reports.
        let ceiling = self.ceiling.unwrap_or(max_compressed_bytes);
        if written > ceiling {
            return Err(GitHubSourceError::LimitExceeded {
                limit_name: "ARCHIVE_COMPRESSED_LIMIT",
                limit: ceiling,
                observed: written,
            });
        }

        Ok(ArchiveDownload {
            compressed_bytes: written,
        })
    }
}

async fn compose_over(files: Vec<(&'static str, &'static str)>, ceiling: Option<u64>) -> Composed {
    let scratch = tempfile::tempdir().expect("a scratch parent");
    composition::compose(
        &ArchiveSource { files, ceiling },
        &RepositoryCoordinate::new("owner", "repo"),
        &commit(),
        scratch.path(),
    )
    .await
}

const MAIN_RS: &str = "// entry point\nfn main() {\n    println!(\"hi\");\n}\n";
const LIB_RS: &str = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
const VENDORED: &str = "module.exports = 1;\n";

#[tokio::test]
async fn a_commit_archive_becomes_a_counted_repository() {
    // The path this whole slice exists to create: bytes from the archive
    // endpoint, through a bounded extraction, into counts a report can publish.
    let composed = compose_over(
        vec![
            ("src/main.rs", MAIN_RS),
            ("crates/a/src/lib.rs", LIB_RS),
            ("node_modules/left-pad/index.js", VENDORED),
        ],
        None,
    )
    .await;

    let Composed::Counted(counted) = composed else {
        panic!("a well-formed archive must count: {composed:?}");
    };

    // The vendored file is excluded rather than counted, and says so. A count
    // that quietly included it would describe somebody else's code.
    assert_eq!(counted.composition.counted_files, 2);
    assert!(
        counted
            .composition
            .exclusions
            .iter()
            .any(|exclusion| exclusion.matched_rule.contains("node_modules")),
        "the ledger must name the rule that removed the vendored file: {:?}",
        counted.composition.exclusions
    );

    let paths: Vec<&str> = counted
        .manifest
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, ["crates/a/src/lib.rs", "src/main.rs"]);
}

#[tokio::test]
async fn two_runs_over_one_archive_produce_the_same_counts() {
    // Determinism is the property the whole report rests on, and extraction
    // plus a filesystem walk is where an accidental dependence on order would
    // enter.
    let files = vec![("src/main.rs", MAIN_RS), ("crates/a/src/lib.rs", LIB_RS)];

    let (Composed::Counted(first), Composed::Counted(second)) = (
        compose_over(files.clone(), None).await,
        compose_over(files, None).await,
    ) else {
        panic!("both runs must count");
    };

    assert_eq!(first.manifest, second.manifest);
    assert_eq!(first.composition, second.composition);
}

#[tokio::test]
async fn a_refused_download_is_a_breach_carrying_both_numbers() {
    // "We could not count this" is far less useful than the ceiling and the
    // observed value, and the report has a place for both.
    let composed = compose_over(vec![("src/main.rs", MAIN_RS)], Some(1)).await;

    let Composed::Limited(breach) = composed else {
        panic!("an over-ceiling archive must be a breach: {composed:?}");
    };
    assert_eq!(breach.limit_name, "ARCHIVE_COMPRESSED_LIMIT");
    assert_eq!(breach.limit_value, 1);
    assert!(
        breach.observed_value > breach.limit_value,
        "the observed value has to exceed the ceiling it tripped: {breach:?}"
    );
}

#[tokio::test]
async fn an_archive_that_cannot_be_read_costs_the_counts_and_not_the_analysis() {
    // A truncated or corrupt archive is not a fact about the repository, so it
    // has no limit to publish — and it must not fail the analysis, whose
    // findings were computed before composition ran.
    struct Corrupt;

    impl GitHubRepositorySource for Corrupt {
        async fn resolve_repository(
            &self,
            _coordinate: &RepositoryCoordinate,
        ) -> Result<ResolvedRepository, GitHubSourceError> {
            unreachable!()
        }
        async fn resolve_commit(
            &self,
            _coordinate: &RepositoryCoordinate,
            _reference: &str,
        ) -> Result<ResolvedCommit, GitHubSourceError> {
            unreachable!()
        }
        async fn fetch_tree(
            &self,
            _coordinate: &RepositoryCoordinate,
            _commit: &CommitSha,
        ) -> Result<RepositoryTree, GitHubSourceError> {
            unreachable!()
        }
        async fn collect_selected_blobs(
            &self,
            _coordinate: &RepositoryCoordinate,
            _tree: &RepositoryTree,
            _paths: &[String],
        ) -> Result<BlobSelection, GitHubSourceError> {
            unreachable!()
        }
        async fn download_archive(
            &self,
            _coordinate: &RepositoryCoordinate,
            _commit: &CommitSha,
            _max_compressed_bytes: u64,
            destination: &Path,
        ) -> Result<ArchiveDownload, GitHubSourceError> {
            let mut file = std::fs::File::create(destination).expect("a scratch file");
            file.write_all(b"this is not a gzip stream")
                .expect("writing the corrupt archive");
            Ok(ArchiveDownload {
                compressed_bytes: 25,
            })
        }
    }

    let scratch = tempfile::tempdir().expect("a scratch parent");
    let composed = composition::compose(
        &Corrupt,
        &RepositoryCoordinate::new("owner", "repo"),
        &commit(),
        scratch.path(),
    )
    .await;

    assert!(
        matches!(composed, Composed::Unavailable),
        "an unreadable archive has no ceiling to name: {composed:?}"
    );
}

#[tokio::test]
async fn nothing_of_the_archive_survives_the_count() {
    // The archive is transport. Its bytes are never evidence and never
    // persisted, so the scratch directory must be empty again once the count
    // returns — otherwise a long-running worker accumulates repositories on a
    // volume sized for one.
    let scratch = tempfile::tempdir().expect("a scratch parent");
    let composed = composition::compose(
        &ArchiveSource {
            files: vec![("src/main.rs", MAIN_RS)],
            ceiling: None,
        },
        &RepositoryCoordinate::new("owner", "repo"),
        &commit(),
        scratch.path(),
    )
    .await;

    assert!(matches!(composed, Composed::Counted(_)));
    let leftovers: Vec<_> = std::fs::read_dir(scratch.path())
        .expect("the scratch parent is readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("its entries are readable");
    assert!(
        leftovers.is_empty(),
        "the archive and its extraction must not outlive the count: {:?}",
        leftovers
            .iter()
            .map(std::fs::DirEntry::path)
            .collect::<Vec<_>>()
    );
}
