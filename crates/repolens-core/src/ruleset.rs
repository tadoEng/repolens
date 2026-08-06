//! The seed ruleset.
//!
//! Deliberately small. Six rules is enough to prove the evidence contract end
//! to end — a report that cites real paths at a real commit — without turning
//! the demo into a rules project. Issue #5 grows this; the shape it grows into
//! is fixed here.
//!
//! Every rule is a pure function of the evidence it is handed. No I/O, no
//! clock, no configuration it did not receive. That is what makes two runs over
//! the same commit produce the same report, which is the property the whole
//! product rests on.
//!
//! These rules read **paths only**, not file contents. A path-based rule can
//! honestly claim `DETECTED` for presence and cite the path as evidence, which
//! is a weaker claim than reading the file but a true one — and it needs no
//! blob fetches, so a report costs one tree request rather than dozens.
//! Content-reading rules arrive with #5, and will carry higher confidence
//! because they can show an excerpt.

use serde::{Deserialize, Serialize};

/// Version of this ruleset, part of the reproducibility key.
///
/// Bump on any change that could alter a report: a new rule, a changed
/// threshold, a different evidence path. Two reports with the same commit and
/// different ruleset versions are allowed to disagree; two with the same
/// version are not.
pub const RULESET_VERSION: &str = "1";

/// What a rule concluded.
///
/// Mirrors the wire `FindingState` without depending on it — the contract lives
/// in `repolens-server`, and a domain crate that imported it would invert the
/// dependency the architecture forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Observed directly.
    Detected,
    /// Looked for and genuinely absent.
    Missing,
    /// Could not be established from the evidence collected.
    UnableToVerify,
}

/// One rule's verdict, with the paths that justify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleOutcome {
    /// Stable rule identifier, e.g. `rust.workspace`.
    pub rule_id: &'static str,
    /// What the rule concluded.
    pub outcome: Outcome,
    /// Paths supporting the conclusion. Empty for `UNABLE_TO_VERIFY`, which is
    /// precisely the case where there is nothing to show.
    pub evidence_paths: Vec<String>,
}

/// A rule: a name, and a predicate over repository paths.
struct PathRule {
    rule_id: &'static str,
    /// Matches a path that would satisfy the rule.
    matches: fn(&str) -> bool,
}

/// The rules, in report order.
///
/// Order is fixed here rather than sorted later, because the report promises a
/// server-decided order and a stable one is the cheapest way to keep that
/// promise honest.
const RULES: &[PathRule] = &[
    PathRule {
        rule_id: "rust.workspace",
        matches: |path| path == "Cargo.toml",
    },
    PathRule {
        rule_id: "ci.workflows",
        matches: |path| path.starts_with(".github/workflows/"),
    },
    PathRule {
        rule_id: "docs.architecture",
        matches: |path| {
            let lower = path.to_ascii_lowercase();
            lower.starts_with("docs/architecture") || lower == "architecture.md"
        },
    },
    PathRule {
        rule_id: "contract.openapi",
        matches: |path| path.ends_with("openapi.json") || path.ends_with("openapi.yaml"),
    },
    PathRule {
        rule_id: "database.migrations",
        // Extension compared case-insensitively: `.SQL` is the same migration to
        // every tool that will run it, and treating it as a different file would
        // report a schema as absent because of how somebody named it.
        matches: |path| {
            path.starts_with("migrations/")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sql"))
        },
    },
    PathRule {
        rule_id: "tests.present",
        matches: |path| {
            path.starts_with("tests/") || path.contains("/tests/") || path.ends_with(".test.ts")
        },
    },
];

/// Evaluates every rule against a repository's paths.
///
/// `tree_truncated` is not a detail — it changes what a negative result *means*.
/// When the collector could not see the whole tree, "no architecture document
/// among the paths we have" is `UNABLE_TO_VERIFY`, not `MISSING`. Reporting the
/// second would be claiming knowledge the evidence does not support, which is
/// the single failure this product exists to avoid.
#[must_use]
pub fn evaluate(paths: &[String], tree_truncated: bool) -> Vec<RuleOutcome> {
    RULES
        .iter()
        .map(|rule| {
            let matched: Vec<String> = paths
                .iter()
                .filter(|path| (rule.matches)(path))
                .cloned()
                .collect();

            let outcome = if matched.is_empty() {
                if tree_truncated {
                    Outcome::UnableToVerify
                } else {
                    Outcome::Missing
                }
            } else {
                Outcome::Detected
            };

            RuleOutcome {
                rule_id: rule.rule_id,
                outcome,
                // Bounded: a repository with 4,000 test files should cite a few,
                // not send all of them to a browser.
                evidence_paths: matched.into_iter().take(3).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn detects_what_is_present_and_cites_it() {
        let outcomes = evaluate(&paths(&["Cargo.toml", ".github/workflows/ci.yml"]), false);

        let workspace = outcomes
            .iter()
            .find(|o| o.rule_id == "rust.workspace")
            .unwrap();
        assert_eq!(workspace.outcome, Outcome::Detected);
        assert_eq!(workspace.evidence_paths, vec!["Cargo.toml".to_owned()]);
    }

    #[test]
    fn a_complete_tree_makes_absence_reportable() {
        let outcomes = evaluate(&paths(&["README.md"]), false);
        let docs = outcomes
            .iter()
            .find(|o| o.rule_id == "docs.architecture")
            .unwrap();
        assert_eq!(docs.outcome, Outcome::Missing);
        assert!(docs.evidence_paths.is_empty());
    }

    #[test]
    fn a_truncated_tree_turns_absence_into_unable_to_verify() {
        // The distinction the whole product rests on: we did not see it is not
        // it is not there.
        let outcomes = evaluate(&paths(&["README.md"]), true);
        let docs = outcomes
            .iter()
            .find(|o| o.rule_id == "docs.architecture")
            .unwrap();
        assert_eq!(docs.outcome, Outcome::UnableToVerify);
    }

    #[test]
    fn truncation_does_not_weaken_a_positive_result() {
        // Seeing a file proves it exists regardless of what else was missed.
        let outcomes = evaluate(&paths(&["Cargo.toml"]), true);
        let workspace = outcomes
            .iter()
            .find(|o| o.rule_id == "rust.workspace")
            .unwrap();
        assert_eq!(workspace.outcome, Outcome::Detected);
    }

    #[test]
    fn evidence_is_bounded() {
        let many: Vec<String> = (0..4_000).map(|i| format!("tests/case_{i}.rs")).collect();
        let outcomes = evaluate(&many, false);
        let tests = outcomes
            .iter()
            .find(|o| o.rule_id == "tests.present")
            .unwrap();
        assert!(tests.evidence_paths.len() <= 3);
    }

    #[test]
    fn the_order_is_stable() {
        let first: Vec<_> = evaluate(&paths(&["Cargo.toml"]), false)
            .into_iter()
            .map(|o| o.rule_id)
            .collect();
        let second: Vec<_> = evaluate(&paths(&["Cargo.toml"]), false)
            .into_iter()
            .map(|o| o.rule_id)
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn every_rule_has_a_distinct_id() {
        let mut ids: Vec<_> = RULES.iter().map(|r| r.rule_id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two rules share an id");
    }
}
