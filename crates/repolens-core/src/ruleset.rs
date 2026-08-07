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
///
/// `4` completed the ruleset issue #5 asked for and changed one existing
/// verdict. `rust.workspace` was a path rule matching any root `Cargo.toml`
/// while claiming a workspace, so every single-crate Rust repository detected
/// it — a false positive of exactly the kind narrow titles exist to prevent.
/// It now reads the manifest for a `[workspace]` table, and the weaker claim it
/// used to make has its own id, `rust.cargo_manifest`. A version `3` report and
/// a version `4` report of a single-crate repository will disagree about
/// `rust.workspace`, and the version is how a reader learns that the rule
/// changed rather than the repository.
///
/// `4` also introduced [`Outcome::NotApplicable`], so a content rule whose file
/// does not exist anywhere stops reporting `MISSING`. A Rust-only repository
/// used to accrue an absence for every npm rule in the set; it now accrues
/// none. Same commit, same evidence, different — and more honest — report.
pub const RULESET_VERSION: &str = "4";

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
    /// The rule had no question to answer here.
    ///
    /// Distinct from `Missing`, which claims the repository lacks something.
    /// A Python project does not *lack* an axum dependency; it has no Cargo
    /// manifest for the question to be about. Reporting the first as the second
    /// fills a report with absences nobody should act on.
    NotApplicable,
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
        // Weaker than it sounds, and named for what it tests. A manifest at the
        // root says Rust is built here; it says nothing about how many crates
        // there are.
        rule_id: "rust.cargo_manifest",
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
        // The one file every open-source repository is judged on and the one
        // most often absent. Root only: a `LICENSE` under `vendor/` is a
        // dependency's licence, not this repository's.
        rule_id: "docs.license",
        kind: RuleKind::Path {
            matches: |path| root_named(path, &["LICENSE", "LICENCE", "COPYING"]),
        },
    },
    Rule {
        // Root, `.github/` or `docs/` — the three places GitHub itself looks.
        //
        // Root-only matching made the rule's claim and its test disagree: a
        // conventional `.github/CONTRIBUTING.md` is the single most common
        // placement, and reporting it MISSING is a false absence about a file
        // sitting in the repository.
        rule_id: "docs.contributing",
        kind: RuleKind::Path {
            matches: |path| conventionally_placed(path, &["CONTRIBUTING"]),
        },
    },
    Rule {
        // Same three locations, and the same reason. `.github/SECURITY.md` is
        // where GitHub's own prompt puts it.
        rule_id: "docs.security",
        kind: RuleKind::Path {
            matches: |path| conventionally_placed(path, &["SECURITY"]),
        },
    },
    Rule {
        // A Dockerfile anywhere, not only at the root: a monorepo puts one per
        // deployable, and requiring the root would report a containerised
        // system as having no container.
        rule_id: "deployment.docker",
        kind: RuleKind::Path {
            matches: is_dockerfile,
        },
    },
    Rule {
        // The npm counterpart of `rust.workspace`, and decidable from a path
        // because pnpm puts the declaration in a file of its own.
        rule_id: "node.workspace",
        kind: RuleKind::Path {
            matches: |path| root_named(path, &["PNPM-WORKSPACE"]),
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
    Rule {
        rule_id: "database.diesel",
        kind: RuleKind::Content {
            wants: is_cargo_manifest,
            find: |file| dependency_line(file, "diesel"),
        },
    },
    Rule {
        rule_id: "database.seaorm",
        kind: RuleKind::Content {
            wants: is_cargo_manifest,
            find: |file| dependency_line(file, "sea-orm"),
        },
    },
    Rule {
        // The build tool, which is a different fact from the framework. A
        // SvelteKit app is built by Vite; a plain Vite app is not SvelteKit.
        rule_id: "framework.vite",
        kind: RuleKind::Content {
            wants: is_node_manifest,
            find: |file| dependency_line(file, "vite"),
        },
    },
    Rule {
        // Named for the declaration it reads, not the deployment it suggests.
        //
        // It used to be `frontend.adapter_static`, titled "Frontend builds to
        // static files" — a conclusion a dependency line cannot reach. An
        // adapter can be installed and never selected: `svelte.config.js` names
        // exactly one, and a repository that switched to `adapter-node` last
        // month may still carry this entry. The honest claim is that the
        // dependency is declared, and the title now says so.
        //
        // Proving the stronger claim means reading `svelte.config.*`, which the
        // selection reaches only at the repository root today. In a monorepo the
        // config sits beside the app — `web/svelte.config.js` here — so the
        // stronger rule needs config files selected at depth first, the way
        // manifests now are.
        rule_id: "frontend.adapter_static.declared",
        kind: RuleKind::Content {
            wants: is_node_manifest,
            find: |file| dependency_line(file, "@sveltejs/adapter-static"),
        },
    },
    Rule {
        // Named for the generator it recognises, for the same reason
        // `contract.openapi.committed` is named for a filename: "the client is
        // generated from the contract" is a broader claim than any dependency
        // line can settle, and a repository generating its client another way
        // will report MISSING here. That is a false negative for the broad
        // claim and correct for the narrow one.
        rule_id: "contract.client.openapi_typescript",
        kind: RuleKind::Content {
            wants: is_node_manifest,
            find: |file| dependency_line(file, "openapi-typescript"),
        },
    },
    Rule {
        // Reads the manifest rather than observing it exists.
        //
        // This was a path rule matching any root `Cargo.toml` under the title
        // "Rust workspace detected", so every single-crate repository in the
        // world detected it. The claim and the test now agree.
        rule_id: "rust.workspace",
        kind: RuleKind::Content {
            wants: |path| path == "Cargo.toml",
            find: |file| table_line(file, "workspace"),
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

/// A root-level file whose name is one of `names`, or one of them followed by a
/// separator.
///
/// Compared case-insensitively and anchored at both ends, which is the whole
/// point: `LICENSE`, `LICENSE.md` and `LICENSE-MIT` are the file, and
/// `LICENSED_TO.md` is not. A bare prefix test would accept the last one, and a
/// wrong DETECTED is worse here than a MISSING — nobody re-checks a box that is
/// already ticked.
///
/// Root only. A `LICENSE` under `vendor/` belongs to a dependency, and counting
/// it would let a repository inherit a claim it never made.
fn root_named(path: &str, names: &[&str]) -> bool {
    if path.contains('/') {
        return false;
    }
    let upper = path.to_ascii_uppercase();
    names.iter().any(|name| {
        upper
            .strip_prefix(name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '-', '_']))
    })
}

/// A community-health file, in any of the places GitHub looks for one.
///
/// The repository root, `.github/`, or `docs/` — GitHub's own documented search
/// order, and `.github/` is where its "add a security policy" prompt puts the
/// file. Anywhere else does not count: a `SECURITY.md` inside a subproject is
/// that subproject's policy.
///
/// Stricter about the name than [`root_named`], and deliberately so. GitHub
/// recognises `SECURITY.md`, `SECURITY.txt` or a bare `SECURITY` — the stem
/// plus an extension, nothing else. The `-` and `_` latitude that makes
/// `LICENSE-MIT` a licence would make `SECURITY_NOTES.md` a security policy,
/// and a wrong DETECTED is worse than a MISSING because nobody re-checks a
/// ticked box. This test caught that on its first run.
fn conventionally_placed(path: &str, names: &[&str]) -> bool {
    let name = path
        .strip_prefix(".github/")
        .or_else(|| path.strip_prefix("docs/"))
        .unwrap_or(path);
    if name.contains('/') {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    names.iter().any(|stem| {
        upper
            .strip_prefix(stem)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

/// A Dockerfile, by any of the conventional spellings, at any depth.
///
/// `Dockerfile`, `Dockerfile.web` and `web.Dockerfile` are all the file; a
/// `Dockerfile` inside `docs/` is still a Dockerfile. Compared on the final
/// path component so `my-Dockerfile-notes.md` is not one.
fn is_dockerfile(path: &str) -> bool {
    let Some(name) = path.rsplit('/').next() else {
        return false;
    };
    let upper = name.to_ascii_uppercase();
    upper == "DOCKERFILE" || upper.starts_with("DOCKERFILE.") || upper.ends_with(".DOCKERFILE")
}

/// The first line opening the TOML table `name`.
///
/// As crude as [`dependency_line`] and honest in the same way: it matches
/// `[workspace]` on its own line, not `[workspace.dependencies]`, and it would
/// match one inside a multi-line string. The finding quotes the line, so a
/// reader can see which it was.
fn table_line(file: &FileContent, name: &str) -> Option<(u32, String)> {
    let header = format!("[{name}]");
    file.find_line(|line| line.trim() == header)
        .map(|(number, text)| (number, text.to_owned()))
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
                ContentVerdict::NotApplicable => RuleOutcome {
                    rule_id: rule.rule_id,
                    outcome: Outcome::NotApplicable,
                    evidence: Vec::new(),
                    unverifiable: None,
                },
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
            undecodable: &[],
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
            .find(|o| o.rule_id == "rust.cargo_manifest")
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
            .find(|o| o.rule_id == "rust.cargo_manifest")
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

    /// The same, for a path list already built.
    fn read_paths(paths: &[String], files: &[FileContent]) -> RuleInput<'static> {
        seen(&coordinate(), &commit(), paths, files, false)
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
            undecodable: &[],
            tree_truncated: false,
            contents_collected: false,
        };

        let outcomes = evaluate(&input);
        // Cargo rules only. The tree holds a `Cargo.toml` and no `package.json`,
        // so the npm rules have no question to answer here — see the assertion
        // below, which is the distinction this version added.
        for rule_id in ["framework.axum", "database.sqlx", "database.diesel"] {
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
            outcome_of(&outcomes, "rust.cargo_manifest").outcome,
            Outcome::Detected
        );

        // And a rule whose file does not exist is not unverified — it is not
        // applicable. Whether contents were collected changes nothing about a
        // question this repository never posed.
        assert_eq!(
            outcome_of(&outcomes, "framework.sveltekit").outcome,
            Outcome::NotApplicable
        );
    }

    #[test]
    fn a_repository_of_one_ecosystem_reports_no_absences_from_the_other() {
        /*
         * Issue #5 asks for a NOT_APPLICABLE case, and this is what it is for.
         *
         * A Rust-only repository used to accrue a MISSING for every npm rule in
         * the set — four confident absences about a frontend it never claimed
         * to have. MISSING means "looked for, not there"; a reader acts on it.
         */
        let outcomes = evaluate(&read(
            &["Cargo.toml", "src/main.rs"],
            &[file("Cargo.toml", "[dependencies]\naxum = \"0.8\"\n")],
        ));

        for rule_id in [
            "framework.sveltekit",
            "framework.vite",
            "frontend.adapter_static.declared",
            "contract.client.openapi_typescript",
        ] {
            let outcome = outcome_of(&outcomes, rule_id);
            assert_eq!(
                outcome.outcome,
                Outcome::NotApplicable,
                "{rule_id} has no npm manifest to be about"
            );
            assert!(outcome.evidence.is_empty());
            assert!(
                outcome.unverifiable.is_none(),
                "not applicable is a conclusion, not a limitation"
            );
        }

        // The Rust half still answers properly: one detected, one genuinely
        // absent from a manifest that was read in full.
        assert_eq!(
            outcome_of(&outcomes, "framework.axum").outcome,
            Outcome::Detected
        );
        assert_eq!(
            outcome_of(&outcomes, "database.diesel").outcome,
            Outcome::Missing
        );
    }

    #[test]
    fn a_community_health_file_counts_wherever_github_looks_for_it() {
        // Root-only matching made the claim and the test disagree:
        // `.github/SECURITY.md` is where GitHub's own prompt puts the file, and
        // reporting it MISSING is a false absence about a committed document.
        let detected = |path: &str| {
            outcome_of(&evaluate(&from_paths(&[path], false)), "docs.security").outcome
        };

        for real in [
            "SECURITY.md",
            ".github/SECURITY.md",
            "docs/SECURITY.md",
            "SECURITY",
        ] {
            assert_eq!(detected(real), Outcome::Detected, "{real}");
        }
        for other in [
            // A subproject's own policy is not this repository's.
            "packages/api/SECURITY.md",
            ".github/ISSUE_TEMPLATE/SECURITY.md",
            "SECURITY_NOTES.md",
        ] {
            assert_eq!(detected(other), Outcome::Missing, "{other}");
        }

        assert_eq!(
            outcome_of(
                &evaluate(&from_paths(&[".github/CONTRIBUTING.md"], false)),
                "docs.contributing"
            )
            .outcome,
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
                "{\n  \"devDependencies\": {\n \"@sveltejs/kit\": \"^2.0.0\"\n  }\n}\n",
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

    // ------------------------------------------------------------------
    // Matcher precision. A wrong DETECTED is worse than a MISSING: nobody
    // re-checks a box that is already ticked.
    // ------------------------------------------------------------------

    #[test]
    fn a_licence_is_the_file_and_not_a_file_that_starts_like_one() {
        let detected =
            |path: &str| outcome_of(&evaluate(&from_paths(&[path], false)), "docs.license").outcome;

        for real in [
            "LICENSE",
            "LICENSE.md",
            "LICENSE-MIT",
            "licence.txt",
            "COPYING",
        ] {
            assert_eq!(detected(real), Outcome::Detected, "{real}");
        }
        for other in [
            "LICENSED_TO.md",
            "LICENSES.md",
            "vendor/thing/LICENSE",
            "docs/LICENSE.md",
        ] {
            assert_eq!(
                detected(other),
                Outcome::Missing,
                "{other} is not this repository's licence"
            );
        }
    }

    #[test]
    fn a_dockerfile_is_recognised_by_any_conventional_spelling() {
        let detected = |path: &str| {
            outcome_of(&evaluate(&from_paths(&[path], false)), "deployment.docker").outcome
        };

        for real in [
            "Dockerfile",
            "Dockerfile.web",
            "web.Dockerfile",
            // A monorepo puts one per deployable; requiring the root would
            // report a containerised system as having no container.
            "services/api/Dockerfile",
        ] {
            assert_eq!(detected(real), Outcome::Detected, "{real}");
        }
        for other in ["my-Dockerfile-notes.md", "docs/dockerfiles.md"] {
            assert_eq!(detected(other), Outcome::Missing, "{other}");
        }
    }

    #[test]
    fn a_cargo_manifest_is_not_by_itself_a_workspace() {
        // The false positive this version fixes. A single-crate repository has
        // a root manifest and no workspace, and the old path rule reported
        // "Rust workspace detected" for every one of them.
        let single = read(
            &["Cargo.toml"],
            &[file("Cargo.toml", "[package]\nname = \"one-crate\"\n")],
        );
        let outcomes = evaluate(&single);

        assert_eq!(
            outcome_of(&outcomes, "rust.cargo_manifest").outcome,
            Outcome::Detected,
            "the manifest is there, and that weaker claim is still worth making"
        );
        assert_eq!(
            outcome_of(&outcomes, "rust.workspace").outcome,
            Outcome::Missing,
            "and it is not a workspace"
        );
    }

    #[test]
    fn a_workspace_table_is_read_from_the_manifest() {
        let outcomes = evaluate(&read(
            &["Cargo.toml"],
            &[file(
                "Cargo.toml",
                "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n",
            )],
        ));
        let workspace = outcome_of(&outcomes, "rust.workspace");

        assert_eq!(workspace.outcome, Outcome::Detected);
        assert_eq!(
            workspace
                .evidence
                .first()
                .and_then(|e| e.excerpt.as_deref()),
            Some("[workspace]")
        );
    }

    #[test]
    fn a_workspace_subtable_is_not_the_workspace_table() {
        // `[workspace.dependencies]` appears in plenty of member manifests that
        // are not themselves the workspace root.
        let outcomes = evaluate(&read(
            &["Cargo.toml"],
            &[file(
                "Cargo.toml",
                "[package]\nname = \"x\"\n\n[workspace.dependencies]\nserde = \"1\"\n",
            )],
        ));

        assert_eq!(
            outcome_of(&outcomes, "rust.workspace").outcome,
            Outcome::Missing
        );
    }

    #[test]
    fn a_dependency_whose_name_extends_another_is_not_confused_with_it() {
        // `diesel_migrations` is not `diesel`; `sqlx-cli` is not `sqlx`. Both
        // are common, and both would be false positives under a `contains`.
        let outcomes = evaluate(&read(
            &["Cargo.toml"],
            &[file(
                "Cargo.toml",
                "[dependencies]\ndiesel_migrations = \"2\"\nsqlx-cli = \"0.8\"\n",
            )],
        ));

        assert_eq!(
            outcome_of(&outcomes, "database.diesel").outcome,
            Outcome::Missing
        );
        assert_eq!(
            outcome_of(&outcomes, "database.sqlx").outcome,
            Outcome::Missing
        );
    }

    #[test]
    fn a_scoped_package_containing_a_name_is_not_that_package() {
        // `@sveltejs/vite-plugin-svelte` contains "vite" and is not Vite.
        let outcomes = evaluate(&read(
            &["package.json"],
            &[file(
                "package.json",
                "{\n  \"devDependencies\": {\n    \"@sveltejs/vite-plugin-svelte\": \"^6\"\n  }\n}\n",
            )],
        ));

        assert_eq!(
            outcome_of(&outcomes, "framework.vite").outcome,
            Outcome::Missing
        );
    }

    // ------------------------------------------------------------------
    // Ground truth: RepoLens analysing itself.
    // ------------------------------------------------------------------

    #[test]
    fn repolens_reports_itself_correctly() {
        /*
         * Issue #5's definition of done, as a test rather than a spot-check:
         * every fact below was read off this working tree by hand — `git
         * ls-files`, the root `Cargo.toml`, `web/package.json` and
         * `packages/repolens-api-client/package.json`.
         *
         * It is the one repository whose answers we can verify completely, so
         * it is the one place a false positive or a false MISSING has nowhere
         * to hide. When this fixture stops matching the repository, the fix is
         * to check which of the two is wrong before changing either.
         */
        let paths = paths(&[
            "AGENTS.md",
            "Cargo.lock",
            "Cargo.toml",
            "LICENSE",
            "README.md",
            ".github/workflows/ci.yml",
            "contracts/openapi.json",
            "crates/repolens-core/Cargo.toml",
            "crates/repolens-server/Cargo.toml",
            "docs/ARCHITECTURE.md",
            "migrations/0002_analyses.sql",
            "package.json",
            "packages/repolens-api-client/package.json",
            "pnpm-workspace.yaml",
            "web/package.json",
            "crates/repolens-server/tests/openapi.rs",
        ]);
        let files = vec![
            file(
                "Cargo.toml",
                "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n",
            ),
            // Read too, and deliberately. `database.diesel` and
            // `database.seaorm` may only report MISSING once *every* Cargo
            // manifest has been read — leaving this one out is how the fixture
            // first failed, with `UNABLE_TO_VERIFY` rather than a wrong answer.
            file(
                "crates/repolens-core/Cargo.toml",
                "[dependencies]\nserde = { workspace = true }\n",
            ),
            file(
                "crates/repolens-server/Cargo.toml",
                "[dependencies]\naxum = \"0.8\"\nsqlx = { workspace = true }\n",
            ),
            file("package.json", "{\n  \"private\": true\n}\n"),
            file(
                "web/package.json",
                "{\n  \"devDependencies\": {\n    \"@sveltejs/adapter-static\": \"^3.0.10\",\n    \"@sveltejs/kit\": \"^2.70.2\",\n    \"vite\": \"^8.2.0\"\n  }\n}\n",
            ),
            file(
                "packages/repolens-api-client/package.json",
                "{\n  \"devDependencies\": {\n    \"openapi-typescript\": \"^7.13.0\"\n  }\n}\n",
            ),
        ];
        let outcomes = evaluate(&read_paths(&paths, &files));

        let expected = [
            ("rust.cargo_manifest", Outcome::Detected),
            ("rust.workspace", Outcome::Detected),
            ("node.workspace", Outcome::Detected),
            ("ci.workflows", Outcome::Detected),
            ("docs.architecture", Outcome::Detected),
            ("docs.license", Outcome::Detected),
            // Neither file is committed. Both are true absences, checked by
            // hand against the tree above.
            ("docs.contributing", Outcome::Missing),
            ("docs.security", Outcome::Missing),
            ("contract.openapi.committed", Outcome::Detected),
            ("contract.client.openapi_typescript", Outcome::Detected),
            ("database.migrations", Outcome::Detected),
            ("database.sqlx", Outcome::Detected),
            ("database.diesel", Outcome::Missing),
            ("database.seaorm", Outcome::Missing),
            ("tests.present", Outcome::Detected),
            ("framework.axum", Outcome::Detected),
            ("framework.sveltekit", Outcome::Detected),
            ("framework.vite", Outcome::Detected),
            ("frontend.adapter_static.declared", Outcome::Detected),
            // Render runs the Rust binary natively; there is no Dockerfile.
            ("deployment.docker", Outcome::Missing),
        ];

        for (rule_id, want) in expected {
            let got = outcome_of(&outcomes, rule_id);
            assert_eq!(
                got.outcome, want,
                "{rule_id}: expected {want:?}, got {:?} ({:?})",
                got.outcome, got.unverifiable
            );
        }

        assert_eq!(
            expected.len(),
            outcomes.len(),
            "every rule must be accounted for here, including any newly added one"
        );

        // Nothing is left unverified: every manifest a rule needs was read.
        // This is the half that says the *selection* is right, not only the
        // rules — the version that read only root manifests answered
        // UNABLE_TO_VERIFY to six of these.
        let unverified: Vec<&str> = outcomes
            .iter()
            .filter(|o| o.outcome == Outcome::UnableToVerify)
            .map(|o| o.rule_id)
            .collect();
        assert!(unverified.is_empty(), "unverified: {unverified:?}");

        // And every positive claim can be checked by a reader.
        for outcome in outcomes.iter().filter(|o| o.outcome == Outcome::Detected) {
            assert!(
                !outcome.evidence.is_empty(),
                "{} detected with nothing to show",
                outcome.rule_id
            );
        }
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
