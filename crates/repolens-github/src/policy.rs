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

/// Version of this selection policy, part of the reproducibility key.
///
/// Bump on any change to *which* files are chosen or *in what order*: the named
/// patterns, the nested-manifest rules, the source extensions, the excluded
/// directories, the per-file and total budgets, and the ordering that decides
/// who gets the last slot when a budget runs out.
///
/// The module documentation above has always said that two analyses of one
/// commit must choose the same files "or the reproducibility key becomes a
/// claim the collector cannot honour". Until this constant existed, that
/// sentence described an intention rather than a mechanism — selection could
/// change, every version in the key could stay put, and two reports drawn from
/// different evidence would claim to be comparable. The key's own membership
/// test is *does changing this value change the report?*, and changing which
/// files are read plainly does.
///
/// Two gates hold this number honest, and they catch different edits.
/// `POLICY_SNAPSHOT` pins the *values* selection is decided from — the arrays
/// and the budgets. `BEHAVIOUR_SNAPSHOT` pins what [`select_paths`] does with
/// them, because the ranking, the precedence between passes and the rules about
/// what a dropped file gets recorded as are policy that lives in code rather
/// than in a constant, and a value snapshot cannot see them change.
pub const SELECTION_POLICY_VERSION: &str = "1";

/// The selection policy, rendered from the values that decide it.
///
/// The same device the exclusion and classification policies use, and for the
/// same reason: a version constant asserting "bump me when the policy changes"
/// is a request, not a mechanism. Nothing stopped a new source extension from
/// changing which files are read on a real repository while
/// [`SELECTION_POLICY_VERSION`] stayed at `1`, every test stayed green, and the
/// reproducibility key stayed identical — two reports drawn from different
/// evidence, claiming to be comparable.
///
/// Rendered from the arrays and budgets themselves rather than restated, so it
/// cannot describe a policy other than the one applied. Order is included
/// because it is policy here: it decides who gets the last slot when the
/// selection budget runs out.
#[must_use]
pub fn describe_selection_policy() -> String {
    use std::fmt::Write as _;

    let mut rendered = String::new();
    let _ = writeln!(rendered, "selection-policy {SELECTION_POLICY_VERSION}");
    // Writing to a `String` cannot fail; results are discarded rather than
    // unwrapped so this stays infallible.
    let _ = writeln!(rendered, "named {}", NAMED_FILE_PATTERNS.join(" "));
    let _ = writeln!(rendered, "manifests {}", MANIFEST_FILENAMES.join(" "));
    let _ = writeln!(rendered, "extensions {}", SOURCE_EXTENSIONS.join(" "));
    let _ = writeln!(rendered, "excluded-dirs {}", EXCLUDED_DIRECTORIES.join(" "));
    let _ = writeln!(rendered, "max-selected {}", limits::MAX_SELECTED_FILES);
    let _ = writeln!(rendered, "max-file-bytes {}", limits::MAX_FILE_BYTES);
    let _ = writeln!(rendered, "max-total-bytes {}", limits::MAX_TOTAL_FILE_BYTES);
    rendered
}

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

/// Manifests, wherever in the tree they sit.
///
/// Matched on the final path component at any depth, unlike
/// [`NAMED_FILE_PATTERNS`], whose `*` deliberately stops at a `/`.
///
/// The root patterns are not enough, and RepoLens is the proof: its root
/// `Cargo.toml` is a bare `[workspace]` table and its root `package.json` is
/// four scripts. Every dependency this repository actually has is declared in
/// `crates/*/Cargo.toml`, `web/package.json` or
/// `packages/repolens-api-client/package.json`. Selecting only the root ones
/// left every content rule answering `UNABLE_TO_VERIFY` on the one repository
/// we can check by hand — honest, and useless. That is the shape of most
/// monorepos, not a quirk of this one.
///
/// Bounded the same way everything else here is: excluded directories are
/// skipped, and the selection ceiling still applies. A `node_modules` tree
/// holds thousands of these.
const MANIFEST_FILENAMES: &[&str] = &["Cargo.toml", "package.json"];

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
    /// Retrieved, but the bytes are not valid UTF-8.
    ///
    /// Deliberately distinct from [`SkipReason::Binary`]: a latin-1 source file
    /// carries no `NUL`, passes the sniff, and still cannot become the `&str` a
    /// rule matches on. Distinct from every *other* variant too, because the
    /// request was spent and the bytes did arrive — saying they were never
    /// retrieved would be false.
    Undecodable,
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
            Self::Undecodable => "FILE_SKIPPED_UNDECODABLE",
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
            Self::Undecodable => {
                "A candidate file was retrieved but is not valid UTF-8, so there is no text in it \
                 for a rule to match. The file exists and its bytes arrived; they could not be \
                 read as source."
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
    /// Dropped manifests worth naming before the count stands in for the rest.
    const MAX_RECORDED_DROPS: usize = 8;

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

    // Manifests below the root, before implementation files.
    //
    // A nested manifest answers "what is this built with" outright, which no
    // number of source files does, so it outranks them for the remaining
    // budget. Shallow-first for the same reason as below: `web/package.json`
    // describes the frontend, `web/vendor/thing/package.json` describes
    // somebody else's.
    let mut manifests: Vec<&crate::TreeEntry> = tree
        .entries
        .iter()
        .filter(|entry| is_nested_manifest(&entry.path))
        .collect();
    manifests.sort_by(|left, right| {
        depth(&left.path)
            .cmp(&depth(&right.path))
            .then_with(|| left.path.cmp(&right.path))
    });

    // A manifest that did not fit is worth recording, unlike the 65th source
    // file: every dependency finding rests on one, so "we did not read it" is
    // the gap a reader needs told. Bounded all the same — a repository with a
    // thousand packages would otherwise put a thousand near-identical entries
    // into a report that has already said the selection filled up.
    let mut recorded_drops = 0;

    for entry in manifests {
        if chosen.contains(&entry.path.as_str()) {
            continue;
        }
        if chosen.len() >= limits::MAX_SELECTED_FILES {
            if recorded_drops >= MAX_RECORDED_DROPS {
                break;
            }
            recorded_drops += 1;
        }
        admit(&mut selection, &mut chosen, entry, true);
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

/// A manifest below the repository root, outside vendored directories.
///
/// Root manifests are left to [`NAMED_FILE_PATTERNS`], which ranks them among
/// the other files a repository uses to describe itself. This is only the ones
/// that pass would never reach.
fn is_nested_manifest(path: &str) -> bool {
    if depth(path) == 0 {
        return false;
    }
    if path
        .split('/')
        .any(|segment| EXCLUDED_DIRECTORIES.contains(&segment))
    {
        return false;
    }
    MANIFEST_FILENAMES.contains(&path.rsplit('/').next().unwrap_or(path))
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
    /// The selection policy exactly as version 1 defines it.
    ///
    /// Regenerate deliberately, never by pasting whatever the test printed:
    /// a changed policy under an unchanged version is precisely the defect this
    /// gate exists to catch, and updating the snapshot without bumping
    /// [`SELECTION_POLICY_VERSION`] reintroduces it while turning the test
    /// green.
    const POLICY_SNAPSHOT: &str = "selection-policy 1
named README* LICENSE* Cargo.toml Cargo.lock package.json pnpm-workspace.yaml svelte.config.* vite.config.* Dockerfile* diesel.toml .github/workflows/* docs/ARCHITECTURE* AGENTS.md CONTRIBUTING* SECURITY*
manifests Cargo.toml package.json
extensions c cc cpp cs go h hpp java js jsx kt php py rb rs sql svelte swift ts tsx
excluded-dirs .git dist generated node_modules target third_party vendor
max-selected 64
max-file-bytes 1048576
max-total-bytes 8388608
";

    #[test]
    fn the_selection_policy_matches_the_version_it_is_published_under() {
        assert_eq!(
            super::describe_selection_policy(),
            POLICY_SNAPSHOT,
            "\nthe selection policy changed. Update POLICY_SNAPSHOT *and* bump \
             SELECTION_POLICY_VERSION (currently {}) — selection decides which files every \
             finding is drawn from, so a changed policy under an unchanged version lets two runs \
             of one commit report different evidence while claiming to be comparable.\n",
            super::SELECTION_POLICY_VERSION
        );
    }

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

    fn directory(path: &str) -> TreeEntry {
        TreeEntry {
            path: path.to_owned(),
            sha: "0".repeat(40),
            kind: TreeEntryKind::Tree,
            size_bytes: None,
        }
    }

    /// A tree built so that every ranking decision is visible in one result.
    ///
    /// Small enough to stay under every budget, because what it is for is the
    /// *order*: which pass runs first, how each pass ranks its candidates, and
    /// which rejections are worth a record. Several entries are here to be
    /// absent from the output — a rule that stops matching is as much a change
    /// as one that starts.
    fn ordering_fixture() -> RepositoryTree {
        tree(vec![
            // Named files, taken in the issue's order rather than the tree's,
            // and lexicographically within a single pattern.
            blob("README.md", 10),
            blob("README.rst", 10),
            blob("LICENSE", 10),
            blob("AGENTS.md", 10),
            blob("SECURITY.md", 10),
            blob(".github/workflows/ci.yml", 10),
            // Root manifests belong to the named pass, ahead of every nested
            // one, and ahead of the source files that describe the same thing
            // less directly.
            blob("Cargo.toml", 10),
            blob("package.json", 10),
            // Named candidates that are recorded rather than read. Their
            // absence from the evidence is a fact about the analysis.
            blob("Cargo.lock", limits::MAX_FILE_BYTES + 1),
            directory("docs/ARCHITECTURE"),
            // Never matched at all: `*` stops at a `/`.
            blob("docs/README.md", 10),
            blob(".github/workflows/nested/deep.yml", 10),
            // Nested manifests: shallowest first, then lexicographic, and ahead
            // of every implementation file.
            blob("web/package.json", 10),
            blob("a/package.json", 10),
            blob("crates/api/Cargo.toml", 10),
            blob("apps/web/package.json", 10),
            // Vendored trees are nobody's evidence about this repository, at
            // any depth and whatever the filename.
            blob("node_modules/left-pad/package.json", 10),
            blob("web/vendor/x/Cargo.toml", 10),
            blob("target/debug/gen.rs", 10),
            // Implementation files: shallowest first, then lexicographic.
            blob("src/util/text/pad.rs", 10),
            blob("src/lib.rs", 10),
            blob("build.rs", 10),
            blob("src/app.rs", 10),
            blob("web/src/main.ts", 10),
            // Rejected in the implementation pass, which records nothing: a
            // real repository has thousands of these and a report that listed
            // them would bury the handful of limitations that matter.
            blob("src/big.rs", limits::MAX_FILE_BYTES + 1),
            // Not source. No extension at all, and a leading dot is not one.
            blob("Makefile", 10),
            blob(".eslintrc", 10),
        ])
    }

    /// A tree that fills the selection part-way through the manifest pass.
    ///
    /// The ordering fixture cannot also do this: once the ceiling is reached
    /// the implementation pass stops without admitting anything, so a single
    /// tree shows either how the passes rank their candidates or what happens
    /// when the budget runs out, never both.
    ///
    /// What this one pins is the second: how many files fit, who gets the last
    /// slot, that a dropped manifest is recorded where a dropped source file is
    /// not, and that the record stops after a bounded number of them.
    fn exhaustion_fixture() -> RepositoryTree {
        let mut entries = vec![
            blob("README.md", 10),
            blob("LICENSE", 10),
            blob("Cargo.toml", 10),
            blob("package.json", 10),
        ];
        // Exactly enough depth-1 manifests to spend what the named files left.
        for index in 0..limits::MAX_SELECTED_FILES - 4 {
            entries.push(blob(&format!("p{index:02}/package.json"), 10));
        }
        // Deeper, so they rank last and meet a full selection. More than the
        // recording bound, so the bound itself shows in the result.
        for index in 0..10 {
            entries.push(blob(&format!("q{index:02}/nested/package.json"), 10));
        }
        // Source files, which meet the same full selection and say nothing.
        entries.push(blob("src/a.rs", 10));
        entries.push(blob("deep/nest/b.rs", 10));
        tree(entries)
    }

    /// What [`select_paths`] actually did, rendered from its own output.
    ///
    /// Paired with [`describe_selection_policy`], not a substitute for it: that
    /// one publishes the values selection is decided from, this one the
    /// decisions themselves.
    ///
    /// [`describe_selection_policy`]: super::describe_selection_policy
    fn describe_selection_behaviour() -> String {
        use std::fmt::Write as _;

        let mut rendered = String::new();
        // Writing to a `String` cannot fail; results are discarded rather than
        // unwrapped so this stays infallible.
        let _ = writeln!(
            rendered,
            "selection-behaviour {}",
            super::SELECTION_POLICY_VERSION
        );

        for (name, fixture) in [
            ("ordering", ordering_fixture()),
            ("exhaustion", exhaustion_fixture()),
        ] {
            let selection = select_paths(&fixture);
            let _ = writeln!(rendered, "{name} selected {}", selection.paths.len());
            for path in &selection.paths {
                let _ = writeln!(rendered, "  {path}");
            }
            let _ = writeln!(rendered, "{name} skipped {}", selection.skipped.len());
            for skipped in &selection.skipped {
                let _ = writeln!(rendered, "  {} {}", skipped.reason.code(), skipped.path);
            }
        }
        rendered
    }

    /// What version 1 of the policy *does*, as opposed to what it is made of.
    ///
    /// `POLICY_SNAPSHOT` above pins the arrays and the budgets. It cannot see
    /// the rest of the policy, which lives in `select_paths` as code: the
    /// precedence between the three passes, the ranking inside each of them,
    /// and the rule deciding whether a file that did not fit is worth naming.
    /// Every one of those changes which evidence a report is drawn from.
    ///
    /// Without this, the drift the version exists to prevent stays available in
    /// one hop: swap `depth().cmp().then_with(path)` for another ranking,
    /// update the behavioural tests below to whatever the new ranking produces,
    /// and ship it under `SELECTION_POLICY_VERSION = "1"` with green CI. Those
    /// tests state the intended behaviour, so they move with the intent; this
    /// states the *published* behaviour, so it does not.
    ///
    /// Regenerate deliberately, never by pasting whatever the test printed. The
    /// version is the first line so that a diff puts the bump and the changed
    /// behaviour on screen together.
    const BEHAVIOUR_SNAPSHOT: &str = "\
selection-behaviour 1
ordering selected 17
  README.md
  README.rst
  LICENSE
  Cargo.toml
  package.json
  .github/workflows/ci.yml
  AGENTS.md
  SECURITY.md
  a/package.json
  web/package.json
  apps/web/package.json
  crates/api/Cargo.toml
  build.rs
  src/app.rs
  src/lib.rs
  web/src/main.ts
  src/util/text/pad.rs
ordering skipped 2
  FILE_SKIPPED_TOO_LARGE Cargo.lock
  FILE_SKIPPED_NOT_A_FILE docs/ARCHITECTURE
exhaustion selected 64
  README.md
  LICENSE
  Cargo.toml
  package.json
  p00/package.json
  p01/package.json
  p02/package.json
  p03/package.json
  p04/package.json
  p05/package.json
  p06/package.json
  p07/package.json
  p08/package.json
  p09/package.json
  p10/package.json
  p11/package.json
  p12/package.json
  p13/package.json
  p14/package.json
  p15/package.json
  p16/package.json
  p17/package.json
  p18/package.json
  p19/package.json
  p20/package.json
  p21/package.json
  p22/package.json
  p23/package.json
  p24/package.json
  p25/package.json
  p26/package.json
  p27/package.json
  p28/package.json
  p29/package.json
  p30/package.json
  p31/package.json
  p32/package.json
  p33/package.json
  p34/package.json
  p35/package.json
  p36/package.json
  p37/package.json
  p38/package.json
  p39/package.json
  p40/package.json
  p41/package.json
  p42/package.json
  p43/package.json
  p44/package.json
  p45/package.json
  p46/package.json
  p47/package.json
  p48/package.json
  p49/package.json
  p50/package.json
  p51/package.json
  p52/package.json
  p53/package.json
  p54/package.json
  p55/package.json
  p56/package.json
  p57/package.json
  p58/package.json
  p59/package.json
exhaustion skipped 8
  FILE_SKIPPED_SELECTION_FULL q00/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q01/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q02/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q03/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q04/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q05/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q06/nested/package.json
  FILE_SKIPPED_SELECTION_FULL q07/nested/package.json
";

    #[test]
    fn the_selection_behaviour_matches_the_version_it_is_published_under() {
        assert_eq!(
            describe_selection_behaviour(),
            BEHAVIOUR_SNAPSHOT,
            "\nselection behaves differently than version {} published. Update \
             BEHAVIOUR_SNAPSHOT *and* bump SELECTION_POLICY_VERSION — which files a report draws \
             its findings from is decided here as much as by the patterns and budgets, so a \
             changed ranking under an unchanged version lets two runs of one commit report \
             different evidence while claiming to be comparable.\n",
            super::SELECTION_POLICY_VERSION
        );
    }

    #[test]
    fn a_monorepo_manifest_below_the_root_is_selected() {
        // The gap that made every dependency rule useless on a monorepo: the
        // root patterns stop at a `/`, so `web/package.json` was never read and
        // "which framework" answered UNABLE_TO_VERIFY on a repository whose
        // frontend framework is written down.
        let tree = tree(vec![
            blob("Cargo.toml", 10),
            blob("package.json", 10),
            blob("web/package.json", 10),
            blob("crates/server/Cargo.toml", 10),
            blob("packages/api-client/package.json", 10),
        ]);

        let selection = select_paths(&tree);

        for path in [
            "web/package.json",
            "crates/server/Cargo.toml",
            "packages/api-client/package.json",
        ] {
            assert!(
                selection.paths.iter().any(|chosen| chosen == path),
                "{path} was not selected: {:?}",
                selection.paths
            );
        }
    }

    #[test]
    fn a_vendored_manifest_is_not_selected() {
        // `node_modules` alone holds thousands, and none of them is a fact
        // about this repository.
        let tree = tree(vec![
            blob("package.json", 10),
            blob("node_modules/left-pad/package.json", 10),
            blob("web/node_modules/thing/package.json", 10),
            blob("vendor/lib/Cargo.toml", 10),
            blob("target/package/x/Cargo.toml", 10),
        ]);

        let selection = select_paths(&tree);

        assert_eq!(selection.paths, vec!["package.json".to_owned()]);
    }

    #[test]
    fn manifests_are_selected_shallowest_first() {
        // Deterministic, and ranked: `web/package.json` describes the frontend,
        // `web/vendor-ish/deep/package.json` describes something nested inside
        // it. When the budget runs out the shallow one has to be the survivor.
        let tree = tree(vec![
            blob("a/b/c/package.json", 10),
            blob("z/package.json", 10),
            blob("a/package.json", 10),
        ]);

        let selection = select_paths(&tree);

        assert_eq!(
            selection.paths,
            vec![
                "a/package.json".to_owned(),
                "z/package.json".to_owned(),
                "a/b/c/package.json".to_owned(),
            ]
        );
    }

    #[test]
    fn a_manifest_that_did_not_fit_is_recorded() {
        // Every dependency finding rests on a manifest, so one that was dropped
        // is exactly the gap a reader needs told — unlike the 65th source file.
        let mut entries = vec![blob("package.json", 10)];
        for index in 0..limits::MAX_SELECTED_FILES {
            entries.push(blob(&format!("pkg{index:03}/package.json"), 10));
        }
        let tree = tree(entries);

        let selection = select_paths(&tree);

        assert_eq!(selection.paths.len(), limits::MAX_SELECTED_FILES);
        assert!(
            selection
                .skipped
                .iter()
                .any(|skipped| matches!(skipped.reason, SkipReason::SelectionFull { .. })),
            "a dropped manifest must be recorded: {:?}",
            selection.skipped
        );
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
        // `zz.rs` is what makes this assertion discriminate. Every other path
        // here sorts the same way under both rules, so without a shallow file
        // that loses on name — or a deep one that wins on it — this test would
        // pass just as happily against a plain lexicographic sort and prove
        // only the tie-break.
        let selection = selection_of(vec![
            blob("src/util/text/pad.rs", 10),
            blob("src/lib.rs", 10),
            blob("build.rs", 10),
            blob("zz.rs", 10),
            blob("src/app.rs", 10),
        ]);
        assert_eq!(
            selection.paths,
            vec![
                "build.rs",
                "zz.rs",
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
