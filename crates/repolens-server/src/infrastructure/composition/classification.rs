//! What a counted file *is*, and where it lives.
//!
//! Two questions the report asks of every counted file:
//!
//! * which **role** does it play — production, test, generated, tooling;
//! * which **area** of the repository does it sit in.
//!
//! Both are published, so both are policy rather than convenience, and both are
//! versioned for the same reason the exclusion policy is: two runs that
//! disagree about how much of a repository is test code must be able to say
//! whether the repository changed or the classifier did.
//!
//! # Role is the claim most easily misread
//!
//! A large *generated* file at the top of the "largest files" list is not the
//! same fact as a large hand-written one, and reporting the second when the
//! first is true overstates effort. That is the single most common way a
//! file-size list misleads, which is why the role travels with every row.
//!
//! # This classifies by path, and says so
//!
//! Nothing here opens a file. A path is weak evidence — `src/parser.rs` might
//! be nothing but test helpers — and the honest response is to keep the rules
//! conventional, narrow, and legible rather than to guess harder. Where a
//! convention is genuinely ambiguous the file stays [`CodeRole::Production`],
//! because that is the claim that adds nothing: it is the residual category,
//! not a positive finding.
//!
//! In particular there is **no content sniffing and no heuristic scoring**. A
//! classifier that is right 90% of the time produces a number nobody can check,
//! and an unverifiable number is what this product exists not to publish.

use std::fmt::Write as _;
use std::path::Path;

use crate::contract::report::CodeRole;

/// Version of this policy, persisted with every composition result.
///
/// Bump on any change to how a path is classified. Two reports produced under
/// one version may be compared directly; two under different versions may not.
pub const CLASSIFICATION_POLICY_VERSION: &str = "1";

/// How a role rule decides.
///
/// The authority, exactly as in [`super::exclusion`]: what a reader is told
/// about a rule is rendered from this, so a published description cannot
/// disagree with the match that was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Match {
    /// A whole path segment, at any depth — `tests/`, `benches/`.
    Segment(&'static str),
    /// An infix in the file name — `.test.`, `.spec.`.
    NameContains(&'static str),
    /// An exact repository-relative path.
    ExactPath(&'static str),
}

impl Match {
    fn matches(self, path: &Path) -> bool {
        match self {
            Self::Segment(name) => path.iter().any(|part| part.to_string_lossy() == name),
            Self::NameContains(fragment) => path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains(fragment)),
            Self::ExactPath(exact) => normalise(path) == exact,
        }
    }

    fn expression(self) -> String {
        match self {
            Self::Segment(name) => format!("**/{name}/**"),
            Self::NameContains(fragment) => format!("*{fragment}*"),
            Self::ExactPath(exact) => exact.to_owned(),
        }
    }
}

/// One classification rule.
struct Rule {
    role: CodeRole,
    matcher: Match,
}

/// The rules, in the order they are tried.
///
/// **Order is policy.** `crates/repolens-server/tests/archive_extraction.rs`
/// matches both the `tests` segment and nothing else, but a file under
/// `.github/workflows/` that happened to be named `x.test.yml` would match two
/// rules — first wins, and the list is ordered most-specific first so that a
/// file's most informative label is the one it gets.
const RULES: &[Rule] = &[
    // Generated first: a generated test fixture is more usefully described as
    // generated, because the point of the label is "nobody wrote this by hand".
    Rule {
        role: CodeRole::Generated,
        matcher: Match::ExactPath("contracts/openapi.json"),
    },
    Rule {
        role: CodeRole::Generated,
        matcher: Match::Segment("generated"),
    },
    // Tests. `tests/` is the Cargo and Playwright convention; `__tests__` is the
    // JavaScript one; the name infixes cover co-located tests, which is the
    // dominant TypeScript style and would otherwise all count as production.
    Rule {
        role: CodeRole::Test,
        matcher: Match::Segment("tests"),
    },
    Rule {
        role: CodeRole::Test,
        matcher: Match::Segment("__tests__"),
    },
    Rule {
        role: CodeRole::Test,
        matcher: Match::Segment("e2e"),
    },
    Rule {
        role: CodeRole::Test,
        matcher: Match::NameContains(".test."),
    },
    Rule {
        role: CodeRole::Test,
        matcher: Match::NameContains(".spec."),
    },
    // Tooling: real work, not the product. Kept separate from production so
    // "how much of this repository is the thing it ships" stays answerable.
    Rule {
        role: CodeRole::Tooling,
        matcher: Match::Segment("scripts"),
    },
    Rule {
        role: CodeRole::Tooling,
        matcher: Match::Segment("examples"),
    },
    Rule {
        role: CodeRole::Tooling,
        matcher: Match::Segment("benches"),
    },
    Rule {
        role: CodeRole::Tooling,
        matcher: Match::Segment(".github"),
    },
];

/// The role this path plays.
///
/// [`CodeRole::Production`] is the residual: it is what a path is called when
/// no rule claims it, not a positive finding about the file.
#[must_use]
pub fn role_of(path: &Path) -> CodeRole {
    RULES
        .iter()
        .find(|rule| rule.matcher.matches(path))
        .map_or(CodeRole::Production, |rule| rule.role)
}

/// The area a path belongs to — its first segment, or the repository root.
///
/// Deliberately the *first* segment and nothing cleverer. It is the division a
/// reader already has in their head (`crates/` against `web/`), it needs no
/// per-repository configuration, and it degrades honestly on a flat repository
/// by putting everything in one bucket rather than inventing structure.
#[must_use]
pub fn area_of(path: &Path) -> String {
    let mut parts = path.iter();
    match (parts.next(), parts.next()) {
        // A first segment with something after it is a directory.
        (Some(first), Some(_)) => format!("{}/", first.to_string_lossy()),
        // One component: a file sitting at the repository root.
        _ => "(root)".to_owned(),
    }
}

/// The policy's semantics, rendered for the drift gate.
#[must_use]
pub fn describe_policy() -> String {
    let mut rendered = format!("classification-policy {CLASSIFICATION_POLICY_VERSION}\n");
    for rule in RULES {
        let _ = writeln!(rendered, "{:?} {}", rule.role, rule.matcher.expression());
    }
    rendered
}

/// A repository-relative path with forward slashes, whatever the platform.
fn normalise(path: &Path) -> String {
    path.iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The policy as it stands, pinned.
    ///
    /// **If this fails, you changed how files are classified.** That is allowed
    /// and takes two edits: update this snapshot *and* bump
    /// `CLASSIFICATION_POLICY_VERSION`. The version is the first line so a diff
    /// shows the bump beside the change.
    ///
    /// Same reasoning as the exclusion policy's gate: the version is persisted
    /// as report metadata, and a changed classifier under an unchanged version
    /// lets two runs of one commit disagree about how much of a repository is
    /// test code.
    const POLICY_SNAPSHOT: &str = "\
classification-policy 1
Generated contracts/openapi.json
Generated **/generated/**
Test **/tests/**
Test **/__tests__/**
Test **/e2e/**
Test *.test.*
Test *.spec.*
Tooling **/scripts/**
Tooling **/examples/**
Tooling **/benches/**
Tooling **/.github/**
";

    #[test]
    fn the_policy_matches_the_version_it_is_published_under() {
        assert_eq!(
            describe_policy(),
            POLICY_SNAPSHOT,
            "\nthe classification policy changed. Update POLICY_SNAPSHOT *and* bump \
             CLASSIFICATION_POLICY_VERSION (currently {CLASSIFICATION_POLICY_VERSION}).\n"
        );
    }

    #[test]
    fn this_repositorys_own_files_are_classified_as_a_reader_would() {
        // Ground truth from RepoLens itself, which is the corpus we can check
        // entirely by hand.
        let cases = [
            ("crates/repolens-core/src/ruleset.rs", CodeRole::Production),
            ("web/src/lib/auth/session.svelte.ts", CodeRole::Production),
            (
                "crates/repolens-server/tests/archive_extraction.rs",
                CodeRole::Test,
            ),
            ("web/src/tests/report.svelte.test.ts", CodeRole::Test),
            ("web/e2e/report.spec.ts", CodeRole::Test),
            ("scripts/check-agent-contracts.mjs", CodeRole::Tooling),
            (".github/workflows/ci.yml", CodeRole::Tooling),
            ("contracts/openapi.json", CodeRole::Generated),
        ];
        for (path, expected) in cases {
            assert_eq!(role_of(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn a_name_that_merely_contains_a_keyword_is_still_production() {
        // The false positives a looser matcher would produce. `contest.rs` and
        // `latest.ts` both contain "test"; neither is a test. This is why the
        // name rules match `.test.` with its dots rather than the bare word.
        for path in [
            "src/contest.rs",
            "web/src/lib/latest.ts",
            "src/protester.rs",
            "web/src/lib/spectrum.ts",
            "src/attestation.rs",
        ] {
            assert_eq!(
                role_of(Path::new(path)),
                CodeRole::Production,
                "{path} was misclassified"
            );
        }
    }

    #[test]
    fn a_directory_that_merely_contains_a_keyword_is_still_production() {
        // Segment matching, not substring: `testing-library` and `scriptorium`
        // are ordinary directory names.
        for path in [
            "testing-library/src/index.ts",
            "src/scriptorium/main.rs",
            "examples-archive/old.rs",
        ] {
            assert_eq!(
                role_of(Path::new(path)),
                CodeRole::Production,
                "{path} was misclassified"
            );
        }
    }

    #[test]
    fn generated_outranks_test_because_the_stronger_claim_is_that_nobody_wrote_it() {
        // Order is policy. A generated file that also sits under `tests/` is
        // more usefully labelled generated — the point of the label is that it
        // was not hand-written, which is the fact a reader needs when the file
        // appears near the top of a size list.
        assert_eq!(
            role_of(Path::new("crates/x/tests/generated/schema.rs")),
            CodeRole::Generated
        );
    }

    #[test]
    fn area_is_the_first_segment_and_root_files_say_so() {
        assert_eq!(
            area_of(Path::new("crates/repolens-core/src/lib.rs")),
            "crates/"
        );
        assert_eq!(area_of(Path::new("web/src/app.html")), "web/");
        assert_eq!(area_of(Path::new("Cargo.toml")), "(root)");
        assert_eq!(area_of(Path::new("README.md")), "(root)");
    }

    #[test]
    fn production_is_the_residual_rather_than_a_finding() {
        // A path nothing recognises is production. That is deliberate: it is
        // the label that asserts least, so an unrecognised layout degrades to
        // "ordinary code" instead of to a confident wrong category.
        assert_eq!(
            role_of(Path::new("some/unfamiliar/layout/thing.kt")),
            CodeRole::Production
        );
    }
}
