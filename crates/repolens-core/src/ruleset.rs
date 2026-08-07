//! The analyzer ruleset.
//!
//! Every rule is a pure function of the evidence it is handed. No I/O, no
//! clock, no configuration it did not receive. That is what makes two runs over
//! the same commit produce the same report, which is the property the whole
//! product rests on.
//!
//! # Two kinds of rule, and why the distinction is kept
//!
//! A **path rule** observes that a file exists. It is a weaker claim than
//! reading the file, but a true one, and it costs no blob fetches — a report
//! that used only these costs one tree request rather than dozens.
//!
//! A **content rule** reads a bounded set of files the ingestion boundary
//! selected, and can quote the line it matched. It carries higher confidence
//! because a reader can check it.
//!
//! They are separate variants rather than one general rule because they fail
//! differently. A path rule that matches nothing over a complete tree knows the
//! file is absent. A content rule that matches nothing usually knows far less —
//! the file may not have been retrieved, or may have been cut short by the byte
//! cap — and [`RuleInput::content_verdict`] is what decides which. Merging the
//! two would put that decision inside every rule body.

use serde::{Deserialize, Serialize};

use crate::evidence_input::{ContentVerdict, FileContent, RuleEvidence, Unverifiable};
use crate::rule::RuleInput;

/// Version of this ruleset, part of the reproducibility key.
///
/// Bump on any change that could alter a report: a new rule, a changed
/// threshold, a different evidence path. Two reports with the same commit and
/// different ruleset versions are allowed to disagree; two with the same
/// version are not.
///
/// `2` renamed `contract.openapi` to `contract.openapi.committed` and narrowed
/// its title to the filename it actually tests. A rule id is how a stored
/// report is read back, so the rename alone would make version `1` reports
/// disagree with version `2` reports about a rule that never changed its
/// verdict — which is precisely what the version exists to explain.
///
/// `3` added the content rules and, with them, changed what every existing
/// finding may carry: evidence can now quote a line and cite a digest. Both
/// halves qualify on their own — three new rule ids, and a different evidence
/// shape — so a version `2` report and a version `3` report of the same commit
/// are expected to differ, and the key says why.
pub const RULESET_VERSION: &str = "3";

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

/// One rule's verdict, with the evidence that justifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleOutcome {
    /// Stable rule identifier, e.g. `rust.workspace`.
    pub rule_id: &'static str,
    /// What the rule concluded.
    pub outcome: Outcome,
    /// Evidence supporting the conclusion.
    ///
    /// Empty for `UNABLE_TO_VERIFY`, which is precisely the case where there is
    /// nothing to show — and where inventing something would be the failure
    /// this product exists to avoid.
    pub evidence: Vec<RuleEvidence>,
    /// Why the rule could not conclude, when it could not.
    ///
    /// Carried so the report can say *which* limitation applied rather than a
    /// generic "unable to verify". `None` whenever `outcome` is not
    /// `UnableToVerify`.
    pub unverifiable: Option<Unverifiable>,
}

/// How a rule reaches its verdict.
enum RuleKind {
    /// Observes that a path exists. Cites the path, quotes nothing.
    Path {
        /// Matches a path that would satisfy the rule.
        matches: fn(&str) -> bool,
    },
    /// Reads the files it selects and quotes the line it matched.
    Content {
        /// Which files this rule needs read.
        wants: fn(&str) -> bool,
        /// Finds the supporting line in a file it was handed.
        find: fn(&FileContent) -> Option<(u32, String)>,
    },
}

/// A rule: a stable name and the way it decides.
struct Rule {
    rule_id: &'static str,
    kind: RuleKind,
}

/// Ceiling on evidence items per finding.
///
/// A repository with four thousand test files should cite a few, not send all
/// of them to a browser.
const MAX_EVIDENCE: usize = 3;

/// The rules, in report order.
///
/// Order is fixed here rather than sorted later, because the report promises a
/// server-decided order and a stable one is the cheapest way to keep that
/// promise honest.
const RULES: &[Rule] = &[
    Rule {
        rule_id: "rust.workspace",
        kind: RuleKind::Path {
            matches: |path| path == "Cargo.toml",
        },
    },
    Rule {
        rule_id: "ci.workflows",
        kind: RuleKind::Path {
            matches: |path| path.starts_with(".github/workflows/"),
        },
    },
    Rule {
        rule_id: "docs.architecture",
        kind: RuleKind::Path {
            matches: |path| {
                let lower = path.to_ascii_lowercase();
                lower.starts_with("docs/architecture") || lower == "architecture.md"
            },
        },
    },
    Rule {
        // Named for the filename convention it tests, not for the property a
        // reader wishes it tested. "This repository publishes an OpenAPI
        // contract" is a strictly broader claim than "a file with this name is
        // committed", and only the second is decidable from a path.
        //
        // `rust-lang/crates.io` at 7bef82ce is the case that settles it: it
        // generates its document with utoipa and commits it, as an insta
        // snapshot under a name no path rule can recognise. So this reports
        // MISSING over a complete tree — correct for the narrow claim, and a
        // false negative for the broad one. The general case needs a content
        // or dependency rule.
        rule_id: "contract.openapi.committed",
        kind: RuleKind::Path {
            // The **final path component**, compared exactly. `ends_with` was
            // wrong in the direction that matters: `notopenapi.json`,
            // `my-openapi.yaml` and `vendor/legacy-openapi.json` all end with
            // the string and none of them is the conventional artifact.
            matches: |path| {
                matches!(
                    path.rsplit('/').next(),
                    Some("openapi.json" | "openapi.yaml")
                )
            },
        },
    },
    Rule {
        rule_id: "database.migrations",
        kind: RuleKind::Path {
            // Extension compared case-insensitively: `.SQL` is the same
            // migration to every tool that will run it, and treating it as a
            // different file would report a schema as absent because of how
            // somebody named it.
            matches: |path| {
                path.starts_with("migrations/")
                    && std::path::Path::new(path)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("sql"))
            },
        },
    },
    Rule {
        rule_id: "tests.present",
        kind: RuleKind::Path {
            matches: |path| {
                path.starts_with("tests/") || path.contains("/tests/") || path.ends_with(".test.ts")
            },
        },
    },
    // ----------------------------------------------------------------------
    // Content rules. Each quotes the line it matched, so a reader can check
    // the claim rather than take it on trust.
    // ----------------------------------------------------------------------
    Rule {
        // A dependency entry, not a guess from a directory name. `src/main.rs`
        // existing says a program is here; the manifest naming `axum` says what
        // it is built with.
        rule_id: "framework.axum",
        kind: RuleKind::Content {
            wants: is_cargo_manifest,
            find: |file| dependency_line(file, "axum"),
        },
    },
    Rule {
        rule_id: "framework.sveltekit",
        kind: RuleKind::Content {
            wants: is_node_manifest,
            find: |file| dependency_line(file, "@sveltejs/kit"),
        },
    },
    Rule {
        // Which database layer, not merely that migrations exist. A repository
        // can carry SQL and reach it through anything.
        rule_id: "database.sqlx",
        kind: RuleKind::Content {
            wants: is_cargo_manifest,
            find: |file| dependency_line(file, "sqlx"),
        },
    },
];

/// Any Cargo manifest, workspace root or member.
fn is_cargo_manifest(path: &str) -> bool {
    path == "Cargo.toml" || path.ends_with("/Cargo.toml")
}

/// Any npm manifest.
fn is_node_manifest(path: &str) -> bool {
    path == "package.json" || path.ends_with("/package.json")
}

/// The first line declaring `name` as a dependency.
///
/// Deliberately crude, and honest about it. This matches a line that *starts* a
/// declaration of the name — `axum = "0.8"`, `"@sveltejs/kit": "^2"`,
/// `sqlx = { workspace = true }` — without parsing TOML or JSON. A name inside
/// a comment at the start of a line could match.
///
/// That imprecision is why these findings quote the line rather than asserting
/// on their own authority: a reader can dismiss a wrong one in a glance, which
/// a bare `DETECTED` would not allow. A real parser is the right answer as soon
/// as a rule needs a *version* or has to separate dev-dependencies; nothing
/// here does yet, and a TOML dependency would be the first infrastructural one
/// this crate has ever taken.
fn dependency_line(file: &FileContent, name: &str) -> Option<(u32, String)> {
    let quoted = format!("\"{name}\"");
    file.find_line(|line| {
        let trimmed = line.trim_start();
        let declares = |rest: &str| rest.trim_start().starts_with(['=', ':']);

        trimmed.strip_prefix(name).is_some_and(declares)
            || trimmed.strip_prefix(quoted.as_str()).is_some_and(declares)
    })
    .map(|(number, text)| (number, text.to_owned()))
}

/// Evaluates every rule against the evidence collected for one commit.
///
/// The outcome of *not* matching is the subtle part, and it differs by kind:
///
/// * a **path rule** over a complete tree knows the file is absent, and over a
///   truncated one knows nothing;
/// * a **content rule** defers to [`RuleInput::content_verdict`], because "I
///   read the file and it is not there" and "I never read the file" are
///   different claims that look identical from inside a rule body.
///
/// Reporting the wrong one is the single failure this product exists to avoid.
#[must_use]
pub fn evaluate(input: &RuleInput<'_>) -> Vec<RuleOutcome> {
    RULES
        .iter()
        .map(|rule| evaluate_rule(rule, input))
        .collect()
}

fn evaluate_rule(rule: &Rule, input: &RuleInput<'_>) -> RuleOutcome {
    match rule.kind {
        RuleKind::Path { matches } => {
            let evidence: Vec<RuleEvidence> = input
                .paths
                .iter()
                .filter(|path| matches(path))
                .take(MAX_EVIDENCE)
                .map(RuleEvidence::path_only)
                .collect();

            if evidence.is_empty() {
                // Truncation changes what absence means, and nothing else does.
                let (outcome, unverifiable) = if input.tree_truncated {
                    (Outcome::UnableToVerify, Some(Unverifiable::TreeTruncated))
                } else {
                    (Outcome::Missing, None)
                };
                return RuleOutcome {
                    rule_id: rule.rule_id,
                    outcome,
                    evidence: Vec::new(),
                    unverifiable,
                };
            }

            RuleOutcome {
                rule_id: rule.rule_id,
                outcome: Outcome::Detected,
                evidence,
                unverifiable: None,
            }
        }
        RuleKind::Content { wants, find } => {
            let evidence: Vec<RuleEvidence> = input
                .matching(wants)
                .filter_map(|file| {
                    find(file).map(|(number, text)| RuleEvidence::line(file, number, &text))
                })
                .take(MAX_EVIDENCE)
                .collect();

            if !evidence.is_empty() {
                return RuleOutcome {
                    rule_id: rule.rule_id,
                    outcome: Outcome::Detected,
                    evidence,
                    unverifiable: None,
                };
            }

            match input.content_verdict(wants) {
                ContentVerdict::ReadAndAbsent => RuleOutcome {
                    rule_id: rule.rule_id,
                    outcome: Outcome::Missing,
                    evidence: Vec::new(),
                    unverifiable: None,
                },
                ContentVerdict::Unverifiable(reason) => RuleOutcome {
                    rule_id: rule.rule_id,
                    outcome: Outcome::UnableToVerify,
                    evidence: Vec::new(),
                    unverifiable: Some(reason),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContentDigest;
    use crate::repository::{CommitSha, RepositoryCoordinate};

    fn paths(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn coordinate() -> RepositoryCoordinate {
        RepositoryCoordinate::new("owner", "name")
    }

    fn commit() -> CommitSha {
        CommitSha::parse(&"a".repeat(40)).expect("a literal digest")
    }

    fn file(path: &str, text: &str) -> FileContent {
        FileContent {
            path: path.to_owned(),
            text: text.to_owned(),
            digest: ContentDigest::from_sha256([0x22; 32]),
            truncated: false,
        }
    }

    /// Everything a path-rule test needs: paths, and whether the tree was whole.
    ///
    /// `contents_collected` is true because a run that read *some* files is the
    /// normal case; a path rule does not consult contents either way.
    fn seen(
        repository: &RepositoryCoordinate,
        commit: &CommitSha,
        paths: &[String],
        files: &[FileContent],
        tree_truncated: bool,
    ) -> RuleInput<'static> {
        // Leaked deliberately: a test input outlives the assertion that reads
        // it, and threading lifetimes through every case buys nothing here.
        RuleInput {
            repository: Box::leak(Box::new(repository.clone())),
            commit: Box::leak(Box::new(commit.clone())),
            paths: Box::leak(paths.to_vec().into_boxed_slice()),
            files: Box::leak(files.to_vec().into_boxed_slice()),
            tree_truncated,
            contents_collected: true,
        }
    }

    /// A path-only run: nothing was read.
    fn from_paths(items: &[&str], tree_truncated: bool) -> RuleInput<'static> {
        seen(&coordinate(), &commit(), &paths(items), &[], tree_truncated)
    }

    fn outcome_of(outcomes: &[RuleOutcome], rule_id: &str) -> RuleOutcome {
        outcomes
            .iter()
            .find(|o| o.rule_id == rule_id)
            .unwrap_or_else(|| panic!("{rule_id} is not in the ruleset"))
            .clone()
    }

    #[test]
    fn detects_what_is_present_and_cites_it() {
        let outcomes = evaluate(&from_paths(
            &["Cargo.toml", ".github/workflows/ci.yml"],
            false,
        ));

        let workspace = outcomes
            .iter()
            .find(|o| o.rule_id == "rust.workspace")
            .unwrap();
        assert_eq!(workspace.outcome, Outcome::Detected);
        assert_eq!(
            workspace.evidence,
            vec![RuleEvidence::path_only("Cargo.toml")]
        );
    }

    #[test]
    fn a_complete_tree_makes_absence_reportable() {
        let outcomes = evaluate(&from_paths(&["README.md"], false));
        let docs = outcomes
            .iter()
            .find(|o| o.rule_id == "docs.architecture")
            .unwrap();
        assert_eq!(docs.outcome, Outcome::Missing);
        assert!(docs.evidence.is_empty());
    }

    #[test]
    fn a_truncated_tree_turns_absence_into_unable_to_verify() {
        // The distinction the whole product rests on: we did not see it is not
        // it is not there.
        let outcomes = evaluate(&from_paths(&["README.md"], true));
        let docs = outcomes
            .iter()
            .find(|o| o.rule_id == "docs.architecture")
            .unwrap();
        assert_eq!(docs.outcome, Outcome::UnableToVerify);
    }

    #[test]
    fn truncation_does_not_weaken_a_positive_result() {
        // Seeing a file proves it exists regardless of what else was missed.
        let outcomes = evaluate(&from_paths(&["Cargo.toml"], true));
        let workspace = outcomes
            .iter()
            .find(|o| o.rule_id == "rust.workspace")
            .unwrap();
        assert_eq!(workspace.outcome, Outcome::Detected);
    }

    #[test]
    fn evidence_is_bounded() {
        let many: Vec<String> = (0..4_000).map(|i| format!("tests/case_{i}.rs")).collect();
        let outcomes = evaluate(&from_paths(
            &many.iter().map(String::as_str).collect::<Vec<_>>(),
            false,
        ));
        let tests = outcomes
            .iter()
            .find(|o| o.rule_id == "tests.present")
            .unwrap();
        assert!(tests.evidence.len() <= 3);
    }

    #[test]
    fn the_order_is_stable() {
        let first: Vec<_> = evaluate(&from_paths(&["Cargo.toml"], false))
            .into_iter()
            .map(|o| o.rule_id)
            .collect();
        let second: Vec<_> = evaluate(&from_paths(&["Cargo.toml"], false))
            .into_iter()
            .map(|o| o.rule_id)
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn the_openapi_rule_claims_only_the_filename_it_tests() {
        // Both halves matter, and the second is the one that keeps the title
        // honest.
        let committed = &evaluate(&from_paths(&["contracts/openapi.json"], false));
        let found = committed
            .iter()
            .find(|o| o.rule_id == "contract.openapi.committed")
            .unwrap();
        assert_eq!(found.outcome, Outcome::Detected);

        // `rust-lang/crates.io` at 7bef82ce: utoipa generates the document and
        // the repository commits it — as an insta snapshot, under a name this
        // rule cannot recognise. MISSING is the correct answer to "is there a
        // committed openapi.json or openapi.yaml" and the wrong answer to "does
        // this repository publish an OpenAPI contract", which is why the
        // finding's title asks only the first.
        let snapshot = evaluate(&from_paths(
            &[
                "src/openapi.rs",
                "src/tests/openapi.rs",
                "src/tests/snapshots/integration__openapi__openapi_snapshot-2.snap",
            ],
            false,
        ));
        let found = snapshot
            .iter()
            .find(|o| o.rule_id == "contract.openapi.committed")
            .unwrap();
        assert_eq!(
            found.outcome,
            Outcome::Missing,
            "a path-based rule cannot see an OpenAPI document committed under another name; \
             if this ever detects one, the title must widen to match"
        );
    }

    #[test]
    fn the_openapi_rule_compares_a_filename_rather_than_a_suffix() {
        let outcome = |items: &[&str]| {
            evaluate(&from_paths(items, false))
                .into_iter()
                .find(|o| o.rule_id == "contract.openapi.committed")
                .unwrap()
                .outcome
        };

        // The conventional artifact, at the root and nested.
        for accepted in [
            "openapi.json",
            "openapi.yaml",
            "contracts/openapi.json",
            "api/v1/openapi.yaml",
        ] {
            assert_eq!(
                outcome(&[accepted]),
                Outcome::Detected,
                "should detect {accepted}"
            );
        }

        // Near misses. Every one of these ends with the string the rule used to
        // test, and not one of them is a committed OpenAPI document — a false
        // DETECTED is worse than the false MISSING this rule was narrowed to
        // avoid, because it claims evidence that does not exist.
        for rejected in [
            "notopenapi.json",
            "my-openapi.yaml",
            "vendor/legacy-openapi.json",
            "openapi.json.bak",
            "openapi.yaml.tmpl",
            "openapi.jsonc",
            "docs/openapi.md",
            "openapi.yml",
        ] {
            assert_eq!(
                outcome(&[rejected]),
                Outcome::Missing,
                "should not accept {rejected}"
            );
        }
    }

    #[test]
    fn every_rule_has_a_distinct_id() {
        let mut ids: Vec<_> = RULES.iter().map(|r| r.rule_id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two rules share an id");
    }

    /// A run that read `files`, with `paths` listing the whole tree.
    fn read(items: &[&str], files: &[FileContent]) -> RuleInput<'static> {
        seen(&coordinate(), &commit(), &paths(items), files, false)
    }

    const CARGO_WITH_AXUM: &str = "[dependencies]\naxum = \"0.8\"\nserde = \"1\"\n";

    #[test]
    fn a_content_rule_quotes_the_line_it_matched() {
        // The difference this whole change exists for. A path rule can say
        // `Cargo.toml` exists; only reading it can say what is declared inside,
        // and the excerpt is what lets a reader check rather than trust.
        let outcomes = evaluate(&read(
            &["Cargo.toml"],
            &[file("Cargo.toml", CARGO_WITH_AXUM)],
        ));
        let axum = outcome_of(&outcomes, "framework.axum");

        assert_eq!(axum.outcome, Outcome::Detected);
        let evidence = axum.evidence.first().expect("evidence is cited");
        assert_eq!(evidence.excerpt.as_deref(), Some("axum = \"0.8\""));
        assert_eq!(evidence.line_range, Some((2, 2)));
        assert!(
            evidence.digest.is_some(),
            "a quoted excerpt must be traceable to the bytes it came from"
        );
    }

    #[test]
    fn a_file_read_in_full_can_report_a_dependency_as_absent() {
        // The one silence that is knowledge.
        let outcomes = evaluate(&read(
            &["Cargo.toml"],
            &[file("Cargo.toml", "[dependencies]\nserde = \"1\"\n")],
        ));
        let axum = outcome_of(&outcomes, "framework.axum");

        assert_eq!(axum.outcome, Outcome::Missing);
        assert!(axum.evidence.is_empty());
        assert!(axum.unverifiable.is_none());
    }

    #[test]
    fn a_manifest_that_was_never_read_is_unverified_rather_than_missing() {
        // Selection is bounded, so a manifest can be listed and unread. This is
        // the case that would silently become a confident MISSING.
        let outcomes = evaluate(&read(&["Cargo.toml"], &[]));
        let axum = outcome_of(&outcomes, "framework.axum");

        assert_eq!(axum.outcome, Outcome::UnableToVerify);
        assert_eq!(axum.unverifiable, Some(Unverifiable::NotRetrieved));
        assert!(
            axum.evidence.is_empty(),
            "there is nothing to show, and inventing something is the failure this avoids"
        );
    }

    #[test]
    fn a_run_that_collected_no_contents_leaves_every_content_rule_unverified() {
        let repository = coordinate();
        let commit = commit();
        let listed = paths(&["Cargo.toml"]);
        let input = RuleInput {
            repository: &repository,
            commit: &commit,
            paths: &listed,
            files: &[],
            tree_truncated: false,
            contents_collected: false,
        };

        let outcomes = evaluate(&input);
        for rule_id in ["framework.axum", "framework.sveltekit", "database.sqlx"] {
            let outcome = outcome_of(&outcomes, rule_id);
            assert_eq!(
                outcome.outcome,
                Outcome::UnableToVerify,
                "{rule_id} must not claim absence when nothing was read"
            );
            assert_eq!(
                outcome.unverifiable,
                Some(Unverifiable::ContentsNotCollected)
            );
        }

        // Path rules are unaffected: they never needed contents.
        assert_eq!(
            outcome_of(&outcomes, "rust.workspace").outcome,
            Outcome::Detected
        );
    }

    #[test]
    fn a_truncated_file_cannot_report_a_dependency_as_absent() {
        // The byte cap cut the manifest short, so the dependency may be in the
        // part nobody saw.
        let mut cut = file("Cargo.toml", "[dependencies]\nserde = \"1\"\n");
        cut.truncated = true;

        let outcomes = evaluate(&read(&["Cargo.toml"], &[cut]));
        let axum = outcome_of(&outcomes, "framework.axum");

        assert_eq!(axum.outcome, Outcome::UnableToVerify);
        assert_eq!(axum.unverifiable, Some(Unverifiable::FileTruncated));
    }

    #[test]
    fn a_dependency_is_matched_in_either_manifest_syntax() {
        // TOML and JSON declare the same fact differently, and a rule that
        // handled only one would report a SvelteKit app as having no framework.
        let outcomes = evaluate(&read(
            &["package.json"],
            &[file(
                "package.json",
                "{\n  \"devDependencies\": {\n    \"@sveltejs/kit\": \"^2.0.0\"\n  }\n}\n",
            )],
        ));
        let kit = outcome_of(&outcomes, "framework.sveltekit");

        assert_eq!(kit.outcome, Outcome::Detected);
        assert_eq!(
            kit.evidence.first().and_then(|e| e.excerpt.as_deref()),
            Some("\"@sveltejs/kit\": \"^2.0.0\"")
        );
    }

    #[test]
    fn a_workspace_member_manifest_counts_too() {
        // The dependency is rarely in the root manifest of a workspace.
        let outcomes = evaluate(&read(
            &["Cargo.toml", "crates/server/Cargo.toml"],
            &[file("crates/server/Cargo.toml", CARGO_WITH_AXUM)],
        ));

        assert_eq!(
            outcome_of(&outcomes, "framework.axum").outcome,
            Outcome::Detected
        );
    }

    #[test]
    fn repolens_itself_is_not_reported_as_having_no_frontend_framework() {
        /*
         * Ground truth, taken from this repository. The root `package.json`
         * exists and declares no SvelteKit; `web/package.json` declares it. If
         * only the root is retrieved, the honest answer is that we did not open
         * the file that would have said so.
         *
         * This is the case that a rule reading the first matching file it
         * happened to receive gets confidently wrong, and it gets it wrong in
         * the worst direction: MISSING reads as "we looked", so nobody
         * re-checks it.
         */
        let outcomes = evaluate(&read(
            &["package.json", "web/package.json"],
            &[file(
                "package.json",
                "{
  \"private\": true,
  \"packageManager\": \"pnpm@11.0.0\"
}
",
            )],
        ));
        let kit = outcome_of(&outcomes, "framework.sveltekit");

        assert_eq!(
            kit.outcome,
            Outcome::UnableToVerify,
            "the manifest that declares SvelteKit was never read"
        );
        assert_eq!(kit.unverifiable, Some(Unverifiable::NotRetrieved));
    }

    #[test]
    fn reading_both_manifests_finds_the_framework_in_the_workspace_package() {
        // And the same repository, analyzed properly: the declaration is in the
        // nested manifest, which is where it is in almost every real monorepo.
        let outcomes = evaluate(&read(
            &["package.json", "web/package.json"],
            &[
                file(
                    "package.json",
                    "{
  \"private\": true
}
",
                ),
                file(
                    "web/package.json",
                    "{
  \"devDependencies\": {
    \"@sveltejs/kit\": \"^2.0.0\"
  }
}
",
                ),
            ],
        ));
        let kit = outcome_of(&outcomes, "framework.sveltekit");

        assert_eq!(kit.outcome, Outcome::Detected);
        assert_eq!(
            kit.evidence.first().map(|e| e.path.as_str()),
            Some("web/package.json"),
            "the evidence must point at the manifest that actually declares it"
        );
    }

    #[test]
    fn a_name_that_merely_appears_is_not_a_declaration() {
        // `dependency_line` is crude, and this pins how crude. A mention in
        // prose must not become a DETECTED, because a wrong finding with a
        // quoted line is more convincing than one without.
        let outcomes = evaluate(&read(
            &["Cargo.toml"],
            &[file(
                "Cargo.toml",
                "# we considered axum and chose something else\n[dependencies]\nserde = \"1\"\n",
            )],
        ));

        assert_eq!(
            outcome_of(&outcomes, "framework.axum").outcome,
            Outcome::Missing,
            "a comment mentioning the crate is not a dependency on it"
        );
    }
}
