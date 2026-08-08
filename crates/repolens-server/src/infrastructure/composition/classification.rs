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
//! be nothing but test helpers — and the honest response is to narrow the claim
//! rather than to guess harder.
//!
//! So every role is claimed **positively**, including `Production`, which is
//! recognised by a `src/` segment rather than left over. Where no rule
//! recognises a layout the file is [`CodeRole::Unclassified`], and that is the
//! label which adds nothing.
//!
//! The distinction is the whole point. `Production` means *ordinary
//! implementation code* — a claim about the file. `Unclassified` means *this
//! policy does not recognise the layout* — a statement about the policy. An
//! earlier revision collapsed them by making `Production` the residual, which
//! made the published production share read as far more certain than the
//! evidence supports: every unrecognised path silently voted for it.
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
    // Production, claimed positively and last.
    //
    // This rule exists because the residual became `Unclassified`. Without it
    // every ordinary implementation file falls through to "we recognised
    // nothing", and a report that classifies almost nothing is useless in the
    // opposite direction from one that over-claims.
    //
    // `src/` is the convention Cargo and every JavaScript bundler share, and it
    // is the one directory whose meaning is near-universal. It sits last so the
    // narrower claims win: `web/src/tests/x.ts` is a test that happens to live
    // under `src/`, and `crates/x/src/generated/y.rs` is generated.
    //
    // Anything outside a `src/` tree — a root manifest, a migration, a
    // workflow — stays `Unclassified`, which is accurate. Those files are real
    // and counted; what the policy declines to say is which of the contract's
    // four roles they play.
    Rule {
        role: CodeRole::Production,
        matcher: Match::Segment("src"),
    },
];

/// The role this path plays.
///
/// The residual is [`CodeRole::Unclassified`], **not** `Production`.
///
/// An earlier revision returned `Production` here and called it "the label that
/// asserts least". It is not: the contract defines `Production` as *ordinary
/// implementation code*, which is a claim about the file, while the residual
/// case is a statement about the analyzer — the policy recognised nothing.
/// Collapsing the two makes the published production share read as far more
/// certain than the classifier is, and the report is about to draw a
/// percentage from it.
///
/// This is the same conflation the ruleset refuses one level up, where
/// `MISSING` means "looked for and absent" and never "nobody opened the file".
#[must_use]
pub fn role_of(path: &Path) -> CodeRole {
    RULES
        .iter()
        .find(|rule| rule.matcher.matches(path))
        .map_or(RESIDUAL_ROLE, |rule| rule.role)
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
        _ => ROOT_AREA.to_owned(),
    }
}

/// The policy's semantics, rendered for the drift gate.
///
/// Covers the **area** rule and the **residual** as well as the role rules.
/// An earlier revision rendered only the rule list, which left two decisions
/// that change published numbers outside the gate: `area_of` could switch from
/// the first path segment to something else, and the residual could move
/// between roles, both without failing a test or bumping the version.
#[must_use]
pub fn describe_policy() -> String {
    let mut rendered = format!("classification-policy {CLASSIFICATION_POLICY_VERSION}\n");
    for rule in RULES {
        let _ = writeln!(
            rendered,
            "role {:?} {}",
            rule.role,
            rule.matcher.expression()
        );
    }
    let _ = writeln!(rendered, "residual {RESIDUAL_ROLE:?}");
    let _ = writeln!(rendered, "area {AREA_RULE}");
    let _ = writeln!(rendered, "area-root {ROOT_AREA}");
    rendered
}

/// What a path is called when no rule claims it.
///
/// Named rather than inlined so [`describe_policy`] can state it and the drift
/// gate can catch it moving.
const RESIDUAL_ROLE: CodeRole = CodeRole::Unclassified;

/// How [`area_of`] decides, in one line, for the drift gate.
const AREA_RULE: &str = "first path segment";

/// The area a file directly at the repository root belongs to.
const ROOT_AREA: &str = "(root)";

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
role Generated contracts/openapi.json
role Generated **/generated/**
role Test **/tests/**
role Test **/__tests__/**
role Test **/e2e/**
role Test *.test.*
role Test *.spec.*
role Tooling **/scripts/**
role Tooling **/examples/**
role Tooling **/benches/**
role Tooling **/.github/**
role Production **/src/**
residual Unclassified
area first path segment
area-root (root)
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
        for path in ["testing-library/src/index.ts", "src/scriptorium/main.rs"] {
            assert_eq!(
                role_of(Path::new(path)),
                CodeRole::Production,
                "{path} was misclassified"
            );
        }
        // Outside a `src/` tree there is no positive claim to make, so the
        // honest answer is that the policy does not know — not that this is a
        // tooling directory because its name resembles one.
        assert_eq!(
            role_of(Path::new("examples-archive/old.rs")),
            CodeRole::Unclassified
        );
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
    fn an_unrecognised_layout_is_unclassified_rather_than_production() {
        /*
         * The correction that reshaped this policy.
         *
         * An earlier revision returned `Production` here and called it "the
         * label that asserts least". It is not: the contract defines
         * `Production` as *ordinary implementation code*, a claim about the
         * file, while this case is a statement about the analyzer — no rule
         * recognised the path. Folding the second into the first makes the
         * published production share read as far more certain than the
         * classifier is, and the report draws a percentage from it.
         *
         * Same conflation the ruleset refuses one level up, where `MISSING`
         * means "looked for and absent" and never "nobody opened the file".
         */
        for path in [
            "some/unfamiliar/layout/thing.kt",
            "migrations/0002_analyses.sql",
            "Cargo.toml",
        ] {
            assert_eq!(
                role_of(Path::new(path)),
                CodeRole::Unclassified,
                "{path} was given a role the policy cannot justify"
            );
        }
    }

    #[test]
    fn production_is_claimed_positively_rather_than_left_over() {
        // The other half of the same change. Making the residual
        // `Unclassified` without this rule would leave every ordinary
        // implementation file unclassified too — useless in the opposite
        // direction from over-claiming.
        for path in [
            "crates/repolens-core/src/ruleset.rs",
            "web/src/lib/auth/session.svelte.ts",
        ] {
            assert_eq!(role_of(Path::new(path)), CodeRole::Production, "{path}");
        }
    }
}
