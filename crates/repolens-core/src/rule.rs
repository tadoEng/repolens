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
    /// Paths whose bytes arrived but could not be read as text.
    ///
    /// Absent from `files` like a file nobody fetched, and not the same thing
    /// at all: the request was spent and the bytes are there. Carried so the
    /// rule can say which of the two happened instead of publishing the more
    /// flattering one.
    pub undecodable: &'a [String],
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

    /// Whether this exact path is among the files whose bytes were retrieved.
    fn was_read(&self, path: &str) -> bool {
        self.files.iter().any(|file| file.path == path)
    }

    /// Whether this exact path arrived as bytes that would not decode.
    fn is_undecodable(&self, path: &str) -> bool {
        self.undecodable.iter().any(|other| other == path)
    }

    /// What a content rule may conclude, having matched nothing.
    ///
    /// The four-way distinction from [`crate::evidence_input`], decided once.
    /// A rule author calls this instead of reasoning about it, because the
    /// reasoning is subtle and getting it wrong fails silently — as a
    /// confident `MISSING` for a file nobody opened.
    #[must_use]
    pub fn content_verdict(&self, wants: impl Fn(&str) -> bool + Copy) -> ContentVerdict {
        // Applicability first, because it does not depend on what was read.
        //
        // A repository with no `package.json` anywhere is not a repository that
        // is *missing* SvelteKit, and it is not one where SvelteKit could not be
        // verified either — the question was never its to answer. Asking this
        // before anything about collection is what keeps a Rust-only repository
        // from reporting five npm rules as unverified.
        //
        // Over a truncated tree the claim is not available: `NOT_APPLICABLE`
        // asserts that no such file exists, and a listing we know to be partial
        // cannot support that.
        if !self.path_exists(wants) {
            return if self.tree_truncated {
                ContentVerdict::Unverifiable(Unverifiable::TreeTruncated)
            } else {
                ContentVerdict::NotApplicable
            };
        }

        if !self.contents_collected {
            return ContentVerdict::Unverifiable(Unverifiable::ContentsNotCollected);
        }

        if self.matching(wants).any(|file| file.truncated) {
            // Read, but not all of it. Absence in the part we saw is not
            // absence in the file.
            return ContentVerdict::Unverifiable(Unverifiable::FileTruncated);
        }

        // *Every* candidate the tree listed, not merely one of them.
        //
        // Reading one matching file and stopping is the mistake that makes this
        // product lie, and RepoLens is its own counterexample: the root
        // `package.json` declares no SvelteKit and `web/package.json` does. A
        // rule that concluded from the first file it happened to read would
        // report this repository as having no frontend framework — with the
        // confident `MISSING` that means "we looked, it is not there", not the
        // `UNABLE_TO_VERIFY` that means "we did not open the file that would
        // have said so".
        for path in self.paths.iter().filter(|path| wants(path)) {
            if self.was_read(path) {
                continue;
            }
            // Both are silences about a file that exists, and they are not the
            // same silence. Bytes that arrived and would not decode are not
            // bytes nobody fetched.
            return ContentVerdict::Unverifiable(if self.is_undecodable(path) {
                Unverifiable::NotDecodable
            } else {
                Unverifiable::NotRetrieved
            });
        }

        if self.tree_truncated {
            // Every candidate we know of was read in full — but we do not have
            // all the paths, so another may exist that was never listed. This
            // outranks a full read for the same reason: the unseen part of the
            // repository is exactly where the answer might be.
            return ContentVerdict::Unverifiable(Unverifiable::TreeTruncated);
        }

        // A complete tree, every candidate read in full, and the thing is not
        // there. The only branch that is actually knowledge.
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
            undecodable: &[],
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
            undecodable: &[],
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
            undecodable: &[],
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
            undecodable: &[],
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::NotRetrieved)
        );
    }

    #[test]
    fn one_file_read_does_not_settle_a_question_the_others_could_answer() {
        // The monorepo case, in miniature. Both manifests are candidates; only
        // one was retrieved. Concluding from it would report a fact about the
        // repository drawn from a file chosen by the byte budget.
        let paths = vec!["package.json".to_owned(), "web/package.json".to_owned()];
        let files = vec![file("package.json", "{\"dependencies\":{}}", false)];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &files,
            undecodable: &[],
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(|path| path.ends_with("package.json")),
            ContentVerdict::Unverifiable(Unverifiable::NotRetrieved),
            "an unread candidate makes absence unverifiable, however many were read"
        );
    }

    #[test]
    fn every_candidate_read_in_full_is_what_makes_absence_knowledge() {
        // The positive half of the rule above: the distinction is between
        // "some were read" and "all were", not between "none" and "some".
        let paths = vec!["package.json".to_owned(), "web/package.json".to_owned()];
        let files = vec![
            file("package.json", "{\"dependencies\":{}}", false),
            file("web/package.json", "{\"dependencies\":{}}", false),
        ];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &files,
            undecodable: &[],
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(|path| path.ends_with("package.json")),
            ContentVerdict::ReadAndAbsent
        );
    }

    #[test]
    fn a_truncated_tree_outranks_a_file_that_was_read_in_full() {
        // Reading `Cargo.toml` completely says nothing about a workspace member
        // manifest the truncated listing never mentioned.
        let paths = vec!["Cargo.toml".to_owned()];
        let files = vec![file("Cargo.toml", "[package]\nname = \"x\"\n", false)];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &files,
            undecodable: &[],
            tree_truncated: true,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::TreeTruncated)
        );
    }

    #[test]
    fn bytes_that_arrived_and_would_not_decode_are_not_bytes_nobody_fetched() {
        // Both silences are about a file that exists, and they send a reader
        // somewhere different: `FILE_NOT_RETRIEVED` says the budget or the
        // selection stopped us, which invites "raise the budget". Here the
        // request was spent and the bytes arrived — nothing about a larger
        // budget would help.
        let paths = vec!["Cargo.toml".to_owned()];
        let undecodable = vec!["Cargo.toml".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            undecodable: &undecodable,
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::NotDecodable)
        );
    }

    #[test]
    fn an_undecodable_file_elsewhere_does_not_change_this_rule() {
        // The list is consulted per path, not as a global mood.
        let paths = vec!["Cargo.toml".to_owned()];
        let undecodable = vec!["src/latin1.rs".to_owned()];
        let files = vec![file("Cargo.toml", "[package]\nname = \"x\"\n", false)];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &files,
            undecodable: &undecodable,
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::ReadAndAbsent
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
            undecodable: &[],
            tree_truncated: true,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::TreeTruncated)
        );
    }

    #[test]
    fn a_complete_tree_without_the_path_makes_the_rule_inapplicable() {
        // This used to be `ReadAndAbsent`, which the ruleset turned into
        // MISSING — so a Python repository was reported as lacking every Rust
        // dependency the set knows about. Nothing was lacking; the question was
        // never that repository's to answer.

        let paths = vec!["README.md".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            undecodable: &[],
            tree_truncated: false,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::NotApplicable
        );
    }

    #[test]
    fn a_truncated_tree_cannot_call_a_rule_inapplicable() {
        // `NOT_APPLICABLE` asserts that no such file exists anywhere, and a
        // listing we know to be partial cannot support that. The manifest may
        // be in the part nobody saw.
        let paths = vec!["README.md".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            undecodable: &[],
            tree_truncated: true,
            contents_collected: true,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::Unverifiable(Unverifiable::TreeTruncated)
        );
    }

    #[test]
    fn a_rule_with_no_file_to_read_is_inapplicable_even_before_collection() {
        // Applicability does not depend on what was read. A run that collected
        // nothing at all still knows a Rust rule has no question to answer in a
        // repository with no Cargo manifest, and reporting it as unverified
        // would be a limitation nobody can act on.
        let paths = vec!["README.md".to_owned()];
        let repository = coordinate();
        let commit = commit();
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &paths,
            files: &[],
            undecodable: &[],
            tree_truncated: false,
            contents_collected: false,
        };

        assert_eq!(
            input.content_verdict(wants_cargo),
            ContentVerdict::NotApplicable
        );
    }
}
