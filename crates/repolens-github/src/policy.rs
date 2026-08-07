//! Which files are worth reading, and what to say about the ones that are not.
//!
//! Selection is a pure function of the tree. That is not incidental tidiness:
//! two analyses of the same commit must choose the same files, or the evidence
//! differs between runs and the reproducibility key becomes a claim the
//! collector cannot honour. Nothing here reads a clock, a socket, or a
//! configuration file.
//!
//! Every rule below is bounded and every rejection is recorded, because "we did
//! not read that file" and "that file does not exist" are different facts about
//! a repository and a report that conflates them is lying by omission.

use serde::{Deserialize, Serialize};

use crate::limits;
use crate::{BlobContent, RepositoryTree, TreeEntryKind};

/// Files named directly by issue #4, in the order the issue lists them.
///
/// Order is policy, not presentation: it decides who gets the last slot when
/// the selection budget runs out. The list runs from "what does this project
/// claim to be" (`README`, `LICENSE`) through "what does it actually build"
/// (manifests, container and bundler configuration) to "how is it worked on"
/// (workflows, contribution and security policy) — which is the order a reader
/// would want the answers in if only some were available.
///
/// `*` matches any run of characters other than `/`, so `README*` is a
/// root-level rule and does not quietly also match `vendor/README.md`.
const NAMED_FILE_PATTERNS: &[&str] = &[
    "README*",
    "LICENSE*",
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "pnpm-workspace.yaml",
    "svelte.config.*",
    "vite.config.*",
    "Dockerfile*",
    "diesel.toml",
    ".github/workflows/*",
    "docs/ARCHITECTURE*",
    "AGENTS.md",
    "CONTRIBUTING*",
    "SECURITY*",
];

/// Extensions that make a file a candidate implementation file.
///
/// A closed list rather than "anything textual". The bounded rule the issue asks
/// for has to be statable in one sentence — *source files, in shallow-first
/// order, until the budget is spent* — and "textual" is not a property of a path,
/// so deciding it would mean fetching the file to find out whether it was worth
/// fetching.
const SOURCE_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "cs", "go", "h", "hpp", "java", "js", "jsx", "kt", "php", "py", "rb", "rs",
    "sql", "svelte", "swift", "ts", "tsx",
];

/// Directory names whose contents are never implementation evidence.
///
/// Matched as a whole path segment at any depth. These hold code that the
/// project did not write and is not accountable for; counting it as
/// architectural evidence would let a repository's dependencies out-vote its own
/// design.
const EXCLUDED_DIRECTORIES: &[&str] = &[
    ".git",
    "dist",
    "generated",
    "node_modules",
    "target",
    "third_party",
    "vendor",
];

/// Why a file that was worth reading was not read.
///
/// A limitation to publish, never an error to raise. Every variant carries
/// enough to state what happened without the reader having to guess at a
/// configuration value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SkipReason {
    /// Requested, but absent from the tree at the analyzed commit.
    NotInTree,
    /// Present, but a directory or a submodule rather than a file. A submodule's
    /// contents belong to another repository and are not this analysis' to read.
    NotAFile,
    /// Larger than the per-file ceiling.
    TooLarge {
        /// Size GitHub reported for the file.
        size_bytes: u64,
        /// Ceiling it exceeded.
        limit_bytes: u64,
    },
    /// Contained a `NUL` byte within the sniff window, which is how Git itself
    /// decides a file is binary.
    Binary,
    /// The per-analysis byte budget was already spent when this file came up.
    ///
    /// Deliberately distinct from [`SkipReason::TooLarge`]: this file might be
    /// tiny, and the fix is a bigger budget or a smaller selection rather than
    /// anything about the file.
    BudgetSpent {
        /// The per-analysis ceiling that had been reached.
        limit_bytes: u64,
    },
    /// The selection was already full when this file came up.
    SelectionFull {
        /// Number of files the selection holds.
        limit: usize,
    },
}

impl SkipReason {
    /// Stable, low-cardinality code for the report.
    ///
    /// Separate from the serde representation, which carries the payload: a
    /// report groups by *why* a file was skipped, and a code that varied with
    /// the observed size would put every oversized file in its own bucket.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotInTree => "FILE_SKIPPED_NOT_IN_TREE",
            Self::NotAFile => "FILE_SKIPPED_NOT_A_FILE",
            Self::TooLarge { .. } => "FILE_SKIPPED_TOO_LARGE",
            Self::Binary => "FILE_SKIPPED_BINARY",
            Self::BudgetSpent { .. } => "FILE_SKIPPED_BUDGET_SPENT",
            Self::SelectionFull { .. } => "FILE_SKIPPED_SELECTION_FULL",
        }
    }

    /// What a reader needs to know about this class of skip.
    #[must_use]
    pub const fn explanation(&self) -> &'static str {
        match self {
            Self::NotInTree => {
                "A file the ruleset looks for was not present in the tree at this commit."
            }
            Self::NotAFile => {
                "A candidate path is a directory or a submodule rather than a file. A submodule's \
                 contents belong to another repository and are not this analysis' to read."
            }
            Self::TooLarge { .. } => {
                "A candidate file is larger than the per-file ceiling this analysis reads, so \
                 nothing was read from it."
            }
            Self::Binary => {
                "A candidate file contains NUL bytes, which is how Git itself decides a file is \
                 binary. There is no text in it for a rule to match."
            }
            Self::BudgetSpent { .. } => {
                "The per-analysis byte budget was already spent when this file came up. The file \
                 may be small; the budget is ours, not a property of the repository."
            }
            Self::SelectionFull { .. } => {
                "The bounded file selection was already full when this file came up. Which files \
                 survive is decided by a fixed ranking, not by chance."
            }
        }
    }
}

/// One file that was not read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedPath {
    /// Repository-relative path.
    pub path: String,
    /// What stopped it.
    pub reason: SkipReason,
}

/// The bounded set of paths chosen from a tree.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FileSelection {
    /// Chosen paths, in the order they should be fetched.
    pub paths: Vec<String>,
    /// Candidates that were rejected, and why.
    ///
    /// Only ever *candidates*. A file that no rule ever nominated is not
    /// "skipped" — recording it as such would bury the handful of real
    /// limitations under a list of every file in the repository.
    pub skipped: Vec<SkippedPath>,
}

/// What one retrieval attempt produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlobSelection {
    /// Files whose contents were retrieved.
    pub retrieved: Vec<BlobContent>,
    /// Files that were requested but not retrieved, and why.
    pub skipped: Vec<SkippedPath>,
}

/// Chooses a bounded, deterministic set of paths to read from `tree`.
///
/// Two passes, and the split is the whole policy:
///
/// 1. **Named files** — the list from issue #4, in its order. These are what a
///    repository says about itself, and they are cheap and almost always
///    present.
/// 2. **Implementation files** — source files outside vendored directories,
///    shallowest first and then lexicographic. Shallow first because a path near
///    the root is nearer the architecture: `src/main.rs` describes a program in a
///    way that `src/util/text/pad.rs` does not.
///
/// The ordering is total, so the same tree yields the same selection on every
/// run. Ties broken by path rather than by tree order matter more than they
/// look: GitHub's ordering is stable today and is not part of any contract.
///
/// Size is judged here from the tree's own figures, which costs no request.
/// Binary content cannot be judged from a path at all, so that check lives with
/// retrieval in [`GitHubRestClient`](crate::GitHubRestClient).
pub fn select_paths(tree: &RepositoryTree) -> FileSelection {
    let mut selection = FileSelection::default();
    let mut chosen: Vec<&str> = Vec::new();

    for pattern in NAMED_FILE_PATTERNS {
        let mut matches: Vec<&crate::TreeEntry> = tree
            .entries
            .iter()
            .filter(|entry| matches_pattern(pattern, &entry.path))
            .collect();
        matches.sort_by(|left, right| left.path.cmp(&right.path));

        for entry in matches {
            if chosen.contains(&entry.path.as_str()) {
                continue;
            }
            // A named file is always worth a record when it is dropped: its
            // absence from the evidence is a fact about the analysis, not noise.
            admit(&mut selection, &mut chosen, entry, true);
        }
    }

    let mut candidates: Vec<&crate::TreeEntry> = tree
        .entries
        .iter()
        .filter(|entry| is_implementation_candidate(&entry.path))
        .collect();
    candidates.sort_by(|left, right| {
        depth(&left.path)
            .cmp(&depth(&right.path))
            .then_with(|| left.path.cmp(&right.path))
    });

    for entry in candidates {
        if chosen.len() >= limits::MAX_SELECTED_FILES {
            // Stop scanning rather than recording thousands of near-identical
            // rejections. That the selection filled up is already visible from
            // its size; which particular source file came 65th is not a
            // limitation a reader can act on.
            break;
        }
        if chosen.contains(&entry.path.as_str()) {
            continue;
        }
        admit(&mut selection, &mut chosen, entry, false);
    }

    selection
}

/// Adds one candidate, or records why it could not be added.
///
/// `record_rejection` is false for implementation candidates, of which a large
/// repository has thousands; see [`FileSelection::skipped`].
fn admit<'tree>(
    selection: &mut FileSelection,
    chosen: &mut Vec<&'tree str>,
    entry: &'tree crate::TreeEntry,
    record_rejection: bool,
) {
    let Some(reason) = rejection(entry, chosen.len()) else {
        chosen.push(&entry.path);
        selection.paths.push(entry.path.clone());
        return;
    };

    if record_rejection {
        selection.skipped.push(SkippedPath {
            path: entry.path.clone(),
            reason,
        });
    }
}

/// Why this entry cannot join a selection that already holds `taken` files.
fn rejection(entry: &crate::TreeEntry, taken: usize) -> Option<SkipReason> {
    if entry.kind != TreeEntryKind::Blob {
        return Some(SkipReason::NotAFile);
    }
    if taken >= limits::MAX_SELECTED_FILES {
        return Some(SkipReason::SelectionFull {
            limit: limits::MAX_SELECTED_FILES,
        });
    }
    // An absent size is not a small file. GitHub omits it for entries it does
    // not weigh, and treating "unknown" as "fine" would let exactly the
    // unmeasured case through the measurement.
    match entry.size_bytes {
        Some(size_bytes) if size_bytes > limits::MAX_FILE_BYTES => Some(SkipReason::TooLarge {
            size_bytes,
            limit_bytes: limits::MAX_FILE_BYTES,
        }),
        _ => None,
    }
}

/// Whether `path` is eligible as an implementation file.
fn is_implementation_candidate(path: &str) -> bool {
    if path
        .split('/')
        .any(|segment| EXCLUDED_DIRECTORIES.contains(&segment))
    {
        return false;
    }

    // `rsplit_once` rather than `Path::extension`, so a leading-dot file such as
    // `.eslintrc` is extension-less rather than an `eslintrc` file.
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => SOURCE_EXTENSIONS.contains(&extension),
        _ => false,
    }
}

/// How many directories deep a repository-relative path sits.
fn depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count()
}

/// Matches one selection pattern against a repository-relative path.
///
/// `*` stands for any run of characters other than `/`, and there is at most one
/// per pattern. That single restriction is what makes the patterns honest:
/// without it `README*` would also match `docs/vendor/README.md`, and a rule
/// meant to find the project's own front page would instead find its
/// dependencies'.
fn matches_pattern(pattern: &str, path: &str) -> bool {
    let Some((prefix, suffix)) = pattern.split_once('*') else {
        return pattern == path;
    };
    let Some(rest) = path.strip_prefix(prefix) else {
        return false;
    };
    let Some(wildcard) = rest.strip_suffix(suffix) else {
        return false;
    };
    !wildcard.contains('/')
}

#[cfg(test)]
mod tests {
    use super::{
        FileSelection, SkipReason, is_implementation_candidate, matches_pattern, select_paths,
    };
    use crate::limits;
    use crate::{RepositoryTree, TreeEntry, TreeEntryKind};

    fn blob(path: &str, size_bytes: u64) -> TreeEntry {
        TreeEntry {
            path: path.to_owned(),
            sha: "0".repeat(40),
            kind: TreeEntryKind::Blob,
            size_bytes: Some(size_bytes),
        }
    }

    fn tree(entries: Vec<TreeEntry>) -> RepositoryTree {
        RepositoryTree {
            sha: "4b825dc642cb6eb9a060e54bf8d69288fbee4904".to_owned(),
            entries,
            truncated: false,
        }
    }

    fn selection_of(entries: Vec<TreeEntry>) -> FileSelection {
        select_paths(&tree(entries))
    }

    #[test]
    fn wildcards_never_cross_a_directory_boundary() {
        assert!(matches_pattern("README*", "README.md"));
        assert!(matches_pattern("README*", "README"));
        assert!(!matches_pattern("README*", "docs/README.md"));
        assert!(matches_pattern(
            ".github/workflows/*",
            ".github/workflows/ci.yml"
        ));
        assert!(!matches_pattern(
            ".github/workflows/*",
            ".github/workflows/nested/ci.yml"
        ));
        assert!(matches_pattern("svelte.config.*", "svelte.config.js"));
        assert!(!matches_pattern("Cargo.toml", "crates/x/Cargo.toml"));
    }

    #[test]
    fn named_files_come_first_and_in_the_issues_order() {
        // The order decides who gets the last slot, so it is asserted rather
        // than left to whatever order the tree happened to arrive in.
        let selection = selection_of(vec![
            blob("src/main.rs", 100),
            blob("LICENSE", 100),
            blob("README.md", 100),
        ]);
        assert_eq!(selection.paths, vec!["README.md", "LICENSE", "src/main.rs"]);
    }

    #[test]
    fn implementation_files_are_shallowest_first_then_lexicographic() {
        let selection = selection_of(vec![
            blob("src/util/text/pad.rs", 10),
            blob("src/lib.rs", 10),
            blob("build.rs", 10),
            blob("src/app.rs", 10),
        ]);
        assert_eq!(
            selection.paths,
            vec![
                "build.rs",
                "src/app.rs",
                "src/lib.rs",
                "src/util/text/pad.rs"
            ]
        );
    }

    #[test]
    fn selection_is_identical_for_an_identical_tree_in_a_different_order() {
        // Reproducibility is the reason the ordering is total. GitHub's tree
        // order is stable today and promised nowhere.
        let forward = vec![
            blob("README.md", 10),
            blob("src/a.rs", 10),
            blob("src/b.rs", 10),
            blob("Cargo.toml", 10),
        ];
        let mut reversed = forward.clone();
        reversed.reverse();

        assert_eq!(selection_of(forward).paths, selection_of(reversed).paths);
    }

    #[test]
    fn an_oversized_named_file_is_recorded_rather_than_dropped() {
        let selection = selection_of(vec![blob("Cargo.lock", limits::MAX_FILE_BYTES + 1)]);

        assert!(selection.paths.is_empty());
        assert_eq!(
            selection.skipped,
            vec![super::SkippedPath {
                path: "Cargo.lock".to_owned(),
                reason: SkipReason::TooLarge {
                    size_bytes: limits::MAX_FILE_BYTES + 1,
                    limit_bytes: limits::MAX_FILE_BYTES,
                },
            }]
        );
    }

    #[test]
    fn a_submodule_at_a_named_path_is_not_a_file() {
        let selection = selection_of(vec![TreeEntry {
            path: "Cargo.toml".to_owned(),
            sha: "0".repeat(40),
            kind: TreeEntryKind::Submodule,
            size_bytes: None,
        }]);

        assert!(selection.paths.is_empty());
        assert_eq!(selection.skipped[0].reason, SkipReason::NotAFile);
    }

    #[test]
    fn the_selection_stops_at_the_file_ceiling() {
        let entries = (0..limits::MAX_SELECTED_FILES * 2)
            .map(|index| blob(&format!("src/module_{index:04}.rs"), 10))
            .collect();

        let selection = selection_of(entries);
        assert_eq!(selection.paths.len(), limits::MAX_SELECTED_FILES);
        // Thousands of "did not fit" records would bury the real limitations.
        assert!(selection.skipped.is_empty());
    }

    #[test]
    fn a_named_file_that_does_not_fit_is_still_recorded() {
        let mut entries: Vec<_> = (0..limits::MAX_SELECTED_FILES)
            .map(|index| blob(&format!("README{index:04}.md"), 10))
            .collect();
        entries.push(blob("SECURITY.md", 10));

        let selection = selection_of(entries);
        assert_eq!(selection.paths.len(), limits::MAX_SELECTED_FILES);
        assert_eq!(
            selection.skipped,
            vec![super::SkippedPath {
                path: "SECURITY.md".to_owned(),
                reason: SkipReason::SelectionFull {
                    limit: limits::MAX_SELECTED_FILES,
                },
            }]
        );
    }

    #[test]
    fn vendored_code_is_never_implementation_evidence() {
        assert!(is_implementation_candidate("src/main.rs"));
        assert!(!is_implementation_candidate(
            "node_modules/left-pad/index.js"
        ));
        assert!(!is_implementation_candidate("target/debug/build/x.rs"));
        assert!(!is_implementation_candidate("web/dist/bundle.js"));
        // Extension-less and dotfile paths are not source files.
        assert!(!is_implementation_candidate("Makefile"));
        assert!(!is_implementation_candidate(".rs"));
    }
}
