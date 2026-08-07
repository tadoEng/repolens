//! What is deliberately left out of a line count, and under which rule.
//!
//! LOC is the easiest number in a report to misread, and it is usually wrong
//! because of what was silently left out. A repository whose `node_modules` is
//! counted is not a large repository; it is a repository with dependencies. So
//! every exclusion here is *named*, carries the rule that matched, and reaches
//! the report as data rather than as a footnote nobody wrote.
//!
//! # One source for what a rule does
//!
//! A [`Rule`] carries a [`Matcher`], and that matcher is the only authority.
//! [`Rule::expression`] — the human-readable pattern the report publishes — is
//! *derived* from it rather than written alongside it.
//!
//! The first version of this file wrote them separately, and the hazard is not
//! theoretical: the report could say "excluded by `**/node_modules/**`" while
//! the code matched something else entirely, and every test would still pass
//! because each half was self-consistent. An exclusion ledger whose stated
//! reason is not the applied reason is worse than no ledger, because it is
//! believed.
//!
//! # This is a version, not a configuration
//!
//! [`EXCLUSION_POLICY_VERSION`] is persisted with every result and forms part
//! of the reproducibility key alongside the commit SHA and the Tokei version.
//! Two runs that disagree about a repository's size must be able to say which
//! of the three changed. That is only true if this list never varies at run
//! time — there is no environment variable, no per-request override, and no
//! repository-supplied configuration that can reach it.
//!
//! A repository being able to configure its own exclusions would be worse than
//! a wrong number: it would let a repository decide how large it appears.
//!
//! Changing *what* is excluded without changing the version breaks the same
//! promise from the other direction, so [`describe_policy`] renders the
//! semantics and a test pins that rendering. Editing a matcher fails the test,
//! and the failure names the version bump.
//!
//! # Exclusion is not the same as refusal
//!
//! [`super::entry::Refusal`] is the extractor declining to *write* something —
//! a symlink, a traversal path. That is a safety decision about hostile input.
//! This is an analytical decision about relevance, made over a tree that is
//! already safely on disk, and the two are kept apart because a reader needs to
//! know which one happened.

use std::fmt::Write as _;
use std::path::Path;

/// Version of this policy, persisted with every composition result.
///
/// Bump on any change to what is excluded. A report counted under `1` and a
/// report counted under `2` are allowed to disagree about the same commit;
/// two under the same version are not.
pub const EXCLUSION_POLICY_VERSION: &str = "1";

/// How a rule decides.
///
/// The authority. Everything a reader is shown about a rule is rendered from
/// this, so a published pattern cannot describe a match the code does not
/// perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matcher {
    /// A whole path segment, at any depth.
    ///
    /// Segment-wise is the only matching that behaves: a substring test would
    /// exclude `my-vendor-lib/`, and a prefix test would miss
    /// `web/node_modules/`.
    Directory(&'static str),
    /// Exact file names, wherever they sit.
    FileNames(&'static [&'static str]),
    /// File-name suffixes.
    Suffixes(&'static [&'static str]),
}

impl Matcher {
    /// Whether this matcher covers `path`.
    fn matches(self, path: &Path) -> bool {
        match self {
            Self::Directory(name) => path.iter().any(|segment| segment.to_string_lossy() == name),
            Self::FileNames(names) => path
                .file_name()
                .is_some_and(|actual| names.contains(&actual.to_string_lossy().as_ref())),
            Self::Suffixes(suffixes) => path.file_name().is_some_and(|actual| {
                let actual = actual.to_string_lossy();
                suffixes.iter().any(|suffix| actual.ends_with(suffix))
            }),
        }
    }

    /// The pattern as a reader would write it.
    fn expression(self) -> String {
        match self {
            Self::Directory(name) => format!("**/{name}/**"),
            Self::FileNames(names) => names.join(", "),
            Self::Suffixes(suffixes) => suffixes
                .iter()
                .map(|suffix| format!("*{suffix}"))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    /// A short, stable word for the kind of match, for [`describe_policy`].
    fn kind(self) -> &'static str {
        match self {
            Self::Directory(_) => "directory",
            Self::FileNames(_) => "filename",
            Self::Suffixes(_) => "suffix",
        }
    }
}

/// One rule in the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// Stable identifier, carried into the report so a decision is traceable.
    pub id: &'static str,
    /// What the rule actually matches. The authority for everything below.
    pub matcher: Matcher,
    /// Why this is not the repository's own code.
    pub reason: &'static str,
}

impl Rule {
    /// The pattern the report publishes, rendered from [`Rule::matcher`].
    #[must_use]
    pub fn expression(&self) -> String {
        self.matcher.expression()
    }
}

/// Every rule, in the order they are tried.
///
/// The directory rules deliberately mirror `repolens_github::policy`'s
/// exclusions for *evidence* selection. The two lists answer different
/// questions — "is this architectural evidence" and "is this the repository's
/// own code" — and happen to agree today. They are not shared, because binding
/// them would mean a change to one silently changing the other's version.
const RULES: &[Rule] = &[
    Rule {
        id: "vendored.node_modules",
        matcher: Matcher::Directory("node_modules"),
        reason: "Installed npm dependencies. Counting them measures the ecosystem, not this \
                 repository.",
    },
    Rule {
        id: "vendored.vendor",
        matcher: Matcher::Directory("vendor"),
        reason: "Vendored third-party source. It is committed here, but it was not written here.",
    },
    Rule {
        id: "vendored.third_party",
        matcher: Matcher::Directory("third_party"),
        reason: "Third-party source by convention of its directory name.",
    },
    Rule {
        id: "build.target",
        matcher: Matcher::Directory("target"),
        reason: "Cargo build output.",
    },
    Rule {
        id: "build.dist",
        matcher: Matcher::Directory("dist"),
        reason: "Build output. Counting it counts the same source twice, once written and once \
                 compiled.",
    },
    Rule {
        id: "build.build",
        matcher: Matcher::Directory("build"),
        reason: "Build output by convention of its directory name.",
    },
    Rule {
        id: "generated.generated",
        matcher: Matcher::Directory("generated"),
        reason: "Generated code. It is real code and somebody maintains the generator, but it is \
                 not work this repository's authors did line by line.",
    },
    Rule {
        id: "vcs.git",
        matcher: Matcher::Directory(".git"),
        reason: "Version-control metadata.",
    },
    Rule {
        id: "generated.lockfile",
        matcher: Matcher::FileNames(&[
            "Cargo.lock",
            "Gemfile.lock",
            "composer.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
            "poetry.lock",
            "yarn.lock",
        ]),
        reason: "A dependency lock file. Machine-written, often tens of thousands of lines, and \
                 not a measure of anything anybody wrote.",
    },
    Rule {
        id: "generated.minified",
        matcher: Matcher::Suffixes(&[".min.css", ".min.js"]),
        reason: "Minified output. One line can hold an entire library, which makes a line count \
                 of it meaningless in both directions.",
    },
];

/// Every rule in the policy, for a report that wants to publish the policy
/// itself rather than only the matches.
#[must_use]
pub fn rules() -> &'static [Rule] {
    RULES
}

/// The rule that excludes `path`, if any.
///
/// `path` is relative to the extraction root, so it is shaped like the
/// repository — which is what lets a rule be written as `**/node_modules/**`
/// and mean what a reader thinks it means.
#[must_use]
pub fn excluded_by(path: &Path) -> Option<Rule> {
    RULES
        .iter()
        .find(|rule| rule.matcher.matches(path))
        .copied()
}

/// The policy's semantics, rendered for the drift gate.
///
/// Everything that changes which files are excluded, and nothing that does not:
/// the reasons are prose and may be reworded freely, so they are absent. Two
/// policies with the same rendering exclude the same files.
#[must_use]
pub fn describe_policy() -> String {
    let mut rendered = format!("exclusion-policy {EXCLUSION_POLICY_VERSION}\n");
    for rule in RULES {
        // Writing to a `String` cannot fail; the result is discarded rather
        // than unwrapped so this stays infallible.
        let _ = writeln!(
            rendered,
            "{} {} {}",
            rule.id,
            rule.matcher.kind(),
            rule.expression()
        );
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The policy as it stands, pinned.
    ///
    /// **If this test fails, you changed what the analyzer excludes.** That is
    /// allowed, and it takes two edits rather than one: update this snapshot
    /// *and* bump `EXCLUSION_POLICY_VERSION`.
    ///
    /// The version is the snapshot's first line deliberately, so a diff puts
    /// the bump and the change on adjacent lines and a reviewer cannot see one
    /// without the other.
    ///
    /// Why this exists: `EXCLUSION_POLICY_VERSION` is persisted as report
    /// metadata and is part of the reproducibility key. Without a gate, a
    /// contributor can widen a matcher and leave the version alone — and two
    /// runs recorded under the same policy version would then legitimately
    /// disagree about the same commit, which is exactly what the version exists
    /// to make impossible.
    const POLICY_SNAPSHOT: &str = "\
exclusion-policy 1
vendored.node_modules directory **/node_modules/**
vendored.vendor directory **/vendor/**
vendored.third_party directory **/third_party/**
build.target directory **/target/**
build.dist directory **/dist/**
build.build directory **/build/**
generated.generated directory **/generated/**
vcs.git directory **/.git/**
generated.lockfile filename Cargo.lock, Gemfile.lock, composer.lock, package-lock.json, pnpm-lock.yaml, poetry.lock, yarn.lock
generated.minified suffix *.min.css, *.min.js
";

    #[test]
    fn the_policy_matches_the_version_it_is_published_under() {
        assert_eq!(
            describe_policy(),
            POLICY_SNAPSHOT,
            "\nthe exclusion policy changed. Update POLICY_SNAPSHOT *and* bump \
             EXCLUSION_POLICY_VERSION (currently {EXCLUSION_POLICY_VERSION}) — a changed policy \
             under an unchanged version lets two runs of one commit disagree.\n"
        );
    }

    #[test]
    fn the_published_expression_describes_the_match_that_was_applied() {
        // The duplication this file was restructured to remove. The matcher
        // and the published pattern used to be written separately, so a report
        // could name one glob while the code matched another — and both halves
        // would test green, because each was self-consistent.
        //
        // The expression is now rendered from the matcher, and this proves the
        // rendering faithful by matching a path built from the published
        // pattern.
        //
        // Line comments rather than a block: a glob like `**/name/**` contains
        // `*/`, which closes a block comment early. That is a compile error
        // here and would be a silent truncation in prose.
        for rule in rules() {
            match rule.matcher {
                Matcher::Directory(name) => {
                    assert_eq!(rule.expression(), format!("**/{name}/**"), "{}", rule.id);
                    let path = format!("a/{name}/b.rs");
                    assert_eq!(
                        excluded_by(Path::new(&path)).map(|matched| matched.id),
                        Some(rule.id),
                        "{} publishes a pattern it does not match",
                        rule.id
                    );
                }
                Matcher::FileNames(names) => {
                    for name in names {
                        assert!(
                            rule.expression().contains(name),
                            "{} does not publish {name}",
                            rule.id
                        );
                        let path = format!("a/b/{name}");
                        assert_eq!(
                            excluded_by(Path::new(&path)).map(|matched| matched.id),
                            Some(rule.id),
                            "{} publishes {name} and does not match it",
                            rule.id
                        );
                    }
                }
                Matcher::Suffixes(suffixes) => {
                    for suffix in suffixes {
                        assert!(
                            rule.expression().contains(suffix),
                            "{} does not publish {suffix}",
                            rule.id
                        );
                        let path = format!("a/b/thing{suffix}");
                        assert_eq!(
                            excluded_by(Path::new(&path)).map(|matched| matched.id),
                            Some(rule.id),
                            "{} publishes {suffix} and does not match it",
                            rule.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_vendored_directory_is_excluded_at_any_depth() {
        for path in [
            "node_modules/left-pad/index.js",
            "web/node_modules/thing/index.js",
            "a/b/c/vendor/lib.rs",
            "target/debug/build.rs",
        ] {
            assert!(excluded_by(Path::new(path)).is_some(), "{path} was counted");
        }
    }

    #[test]
    fn a_directory_that_merely_contains_the_word_is_kept() {
        // The failure a substring match would produce, and it is not
        // hypothetical: `vendor-portal` and `my-dist-tool` are ordinary names
        // for real source directories, and excluding them would delete a
        // repository's own code from its own line count.
        for path in [
            "my-vendor-lib/src/main.rs",
            "vendor-portal/app.ts",
            "distribution/index.js",
            "src/generated_code_notes.md",
            "buildings/model.rs",
        ] {
            assert_eq!(
                excluded_by(Path::new(path)),
                None,
                "{path} was excluded by a rule that should not match it"
            );
        }
    }

    #[test]
    fn lock_files_are_excluded_wherever_they_sit() {
        // The case with the largest effect on a headline number: a lock file
        // can be tens of thousands of lines and is written by a machine.
        for path in [
            "Cargo.lock",
            "web/pnpm-lock.yaml",
            "packages/api/package-lock.json",
        ] {
            let rule = excluded_by(Path::new(path)).expect("a lock file is excluded");
            assert_eq!(rule.id, "generated.lockfile", "{path}");
        }
    }

    #[test]
    fn minified_output_is_excluded_and_ordinary_javascript_is_not() {
        assert!(excluded_by(Path::new("web/static/app.min.js")).is_some());
        assert!(excluded_by(Path::new("web/static/app.min.css")).is_some());
        assert_eq!(excluded_by(Path::new("web/src/app.js")), None);
        // `.min.` in the middle of a name is not minified output.
        assert_eq!(excluded_by(Path::new("web/src/app.min.ts")), None);
    }

    #[test]
    fn every_rule_has_a_distinct_id_and_says_why() {
        // The report publishes `matched_rule`, so a duplicate id would make two
        // different decisions indistinguishable to a reader.
        let mut ids: Vec<&str> = rules().iter().map(|rule| rule.id).collect();
        ids.sort_unstable();
        let mut unique = ids.clone();
        unique.dedup();
        assert_eq!(ids, unique, "duplicate rule id");

        for rule in rules() {
            assert!(!rule.reason.is_empty(), "{} has no reason", rule.id);
            assert!(!rule.expression().is_empty(), "{} has no pattern", rule.id);
            assert!(
                !rule.reason.contains("  "),
                "a published sentence carries a run of spaces, which is a lost line \
                 continuation rather than typography: {:?}",
                rule.reason
            );
        }
    }
}
