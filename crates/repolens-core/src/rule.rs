//! The analyzer rule contract.
//!
//! A rule reads evidence and returns findings. It has no I/O, no clock, and no
//! configuration beyond what it is handed, which is what makes a report
//! reproducible from the key in [`crate::reproducibility`]. Concretely: a rule
//! may not depend on `axum` or `sqlx`, and this crate's dependency list is what
//! enforces that.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::evidence_input::{ContentVerdict, FileContent, Unverifiable};
use crate::finding::Finding;
use crate::repository::{CommitSha, RepositoryCoordinate};

/// Stable identifier for a rule, carried on every finding it produces.
///
/// Stable across releases: renaming one is a ruleset-version change, because
/// it changes what a report means.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    /// Names a rule.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Everything a rule is allowed to look at.
///
/// No longer provisional. It carried only repository identity while the
/// analyzer read nothing else; issue #5 added the tree paths and the bounded
/// set of file contents the ingestion boundary selected, which is what the
/// original note reserved this type for. The composition result joins it with
/// issue #12.
///
/// What a rule may conclude from *not* finding something depends on which of
/// these fields is empty and why, so that decision is made once in
/// [`content_verdict`](RuleInput::content_verdict) rather than by each rule.
#[derive(Debug, Clone, Copy)]
pub struct RuleInput<'a> {
    /// Which repository is being analyzed.
    pub repository: &'a RepositoryCoordinate,
    /// The exact commit the evidence was read from.
    pub commit: &'a CommitSha,
    /// Every blob path in the tree listing.
    pub paths: &'a [String],
    /// The bounded set of files whose contents were retrieved.
    pub files: &'a [FileContent],
    /// `true` when GitHub could not return the whole tree.
    pub tree_truncated: bool,
    /// `true` when content collection ran for this analysis.
    ///
    /// Distinct from `files.is_empty()`: a run that collected nothing because
    /// the repository has no interesting files is not the same as a run where
    /// collection never happened, and only the second makes *every* content
    /// rule unverifiable.
    pub contents_collected: bool,
}

impl<'a> RuleInput<'a> {
    /// Files this rule wants, among those actually retrieved.
    pub fn matching(
        &self,
        wants: impl Fn(&str) -> bool + 'a,
    ) -> impl Iterator<Item = &'a FileContent> {
        self.files.iter().filter(move |file| wants(&file.path))
    }

    /// Whether any path in the tree matches, retrieved or not.
    pub fn path_exists(&self, wants: impl Fn(&str) -> bool) -> bool {
        self.paths.iter().any(|path| wants(path))
    }

    /// What a content rule may conclude, having matched nothing.
    ///
    /// The four-way distinction from [`crate::evidence_input`], decided once.
    /// A rule author calls this instead of reasoning about it, because the
    /// reasoning is subtle and getting it wrong fails silently — as a
    /// confident `MISSING` for a file nobody opened.
    #[must_use]
    pub fn content_verdict(&self, wants: impl Fn(&str) -> bool + Copy) -> ContentVerdict {
        if !self.contents_collected {
            return ContentVerdict::Unverifiable(Unverifiable::ContentsNotCollected);
        }

        let mut saw_file = false;
        for file in self.matching(wants) {
            saw_file = true;
            if file.truncated {
                // Read, but not all of it. Absence in the part we saw is not
                // absence in the file.
                return ContentVerdict::Unverifiable(Unverifiable::FileTruncated);
            }
        }

        if saw_file {
            // Read in full and the thing is not there. The only branch that is
            // actually knowledge.
            return ContentVerdict::ReadAndAbsent;
        }

        if self.path_exists(wants) {
            // The tree listed it; selection or the byte budget did not retrieve
            // it. We know it exists and nothing about what is inside.
            return ContentVerdict::Unverifiable(Unverifiable::NotRetrieved);
        }

        if self.tree_truncated {
            // Not among the paths we have — but we do not have all of them.
            return ContentVerdict::Unverifiable(Unverifiable::TreeTruncated);
        }

        // A complete tree with no such path: there was nothing to read.
        ContentVerdict::ReadAndAbsent
    }
}

/// A single analyzer rule.
pub trait AnalyzerRule {
    /// Identifier reported on every finding this rule emits.
    fn id(&self) -> RuleId;

    /// Evaluates the rule.
    ///
    /// Returning an empty vector means "this rule found nothing", which is not
    /// the same as "this rule could not run" — a rule that cannot run reports a
    /// limitation instead of silently returning nothing.
    fn evaluate(&self, input: &RuleInput<'_>) -> Vec<Finding>;
}

#[cfg(test)]
mod verdict_tests {
    use super::RuleInput;
    use crate::ContentDigest;
    use crate::evidence_input::{ContentVerdict, FileContent, Unverifiable};
    use crate::repository::{CommitSha, RepositoryCoordinate};

    fn digest() -> ContentDigest {
        ContentDigest::from_sha256([0x11; 32])
    }

    fn file(path: &str, text: &str, truncated: bool) -> FileContent {
        FileContent {
            path: path.to_owned(),
            text: text.to_owned(),
            digest: digest(),
            truncated,
        }
    }

    fn coordinate() -> RepositoryCoordinate {
        RepositoryCoordinate::new("owner", "name")
    }

    fn commit() -> CommitSha {
        CommitSha::parse(&"a".repeat(40)).expect("a literal digest")
    }

    fn wants_cargo(path: &str) -> bool {
        path == "Cargo.toml"
    }

    #[test]
    fn a_run_that_collected_nothing_can_conclude_nothing_from_content() {
        let paths = vec!["Cargo.toml".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            tree_truncated: false,
            contents_collected: false,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::ContentsNotCollected),
            "a run without content collection must not report absence"
        );
    }

    #[test]
    fn a_file_read_in_full_makes_absence_knowledge() {
        let paths = vec!["Cargo.toml".to_owned()];
        let files = vec![file("Cargo.toml", "[package]\nname = \"x\"\n", false)];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &files,
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::ReadAndAbsent
        );
    }

    #[test]
    fn a_truncated_file_cannot_establish_absence() {
        // The trap this exists to close: the rule looked, found nothing, and
        // the part it did not see is exactly where the answer might be.
        let paths = vec!["Cargo.toml".to_owned()];
        let files = vec![file("Cargo.toml", "[package]", true)];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &files,
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::FileTruncated)
        );
    }

    #[test]
    fn a_path_that_exists_but_was_not_read_is_unverifiable() {
        // Selection is bounded, so a file can be present and unread. Reporting
        // MISSING here would be claiming knowledge of a file nobody opened.
        let paths = vec!["Cargo.toml".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::NotRetrieved)
        );
    }

    #[test]
    fn a_truncated_tree_cannot_establish_that_a_file_is_absent() {
        let paths = vec!["README.md".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            tree_truncated: true,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::TreeTruncated)
        );
    }

    #[test]
    fn a_complete_tree_without_the_path_is_genuine_absence() {
        let paths = vec!["README.md".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::ReadAndAbsent
        );
    }
}
