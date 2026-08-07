//! What is deliberately left out of a line count, and under which rule.
//!
//! LOC is the easiest number in a report to misread, and it is usually wrong
//! because of what was silently left out. A repository whose `node_modules` is
//! counted is not a large repository; it is a repository with dependencies. So
//! every exclusion here is *named*, carries the rule that matched, and reaches
//! the report as data rather than as a footnote nobody wrote.
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
//! # Exclusion is not the same as refusal
//!
//! [`super::entry::Refusal`] is the extractor declining to *write* something —
//! a symlink, a traversal path. That is a safety decision about hostile input.
//! This is an analytical decision about relevance, made over a tree that is
//! already safely on disk, and the two are kept apart because a reader needs to
//! know which one happened.

use std::path::Path;

/// Version of this policy, persisted with every composition result.
///
/// Bump on any change to what is excluded. A report counted under `1` and a
/// report counted under `2` are allowed to disagree about the same commit;
/// two under the same version are not.
pub const EXCLUSION_POLICY_VERSION: &str = "1";

/// One rule in the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// Stable identifier, carried into the report so a decision is traceable.
    pub id: &'static str,
    /// The pattern, as a reader would write it.
    pub expression: &'static str,
    /// Why this is not the repository's own code.
    pub reason: &'static str,
}

/// Directory names whose contents are never the repository's own code.
///
/// Matched as a whole path segment at any depth, which is the only matching
/// that behaves: a substring test would exclude `my-vendor-lib/` and a
/// prefix test would miss `web/node_modules/`.
///
/// This deliberately mirrors `repolens_github::policy`'s exclusions for
/// *evidence* selection. The two lists answer different questions — "is this
/// architectural evidence" and "is this the repository's own code" — and happen
/// to agree today. They are not shared, because binding them would mean a
/// change to one silently changing the other's version.
const EXCLUDED_DIRECTORIES: &[Rule] = &[
    Rule {
        id: "vendored.node_modules",
        expression: "**/node_modules/**",
        reason: "Installed npm dependencies. Counting them measures the ecosystem, not this \
                 repository.",
    },
    Rule {
        id: "vendored.vendor",
        expression: "**/vendor/**",
        reason: "Vendored third-party source. It is committed here, but it was not written here.",
    },
    Rule {
        id: "vendored.third_party",
        expression: "**/third_party/**",
        reason: "Third-party source by convention of its directory name.",
    },
    Rule {
        id: "build.target",
        expression: "**/target/**",
        reason: "Cargo build output.",
    },
    Rule {
        id: "build.dist",
        expression: "**/dist/**",
        reason: "Build output. Counting it counts the same source twice, once written and once \
                 compiled.",
    },
    Rule {
        id: "build.build",
        expression: "**/build/**",
        reason: "Build output by convention of its directory name.",
    },
    Rule {
        id: "generated.generated",
        expression: "**/generated/**",
        reason: "Generated code. It is real code and somebody maintains the generator, but it is \
                 not work this repository's authors did line by line.",
    },
    Rule {
        id: "vcs.git",
        expression: "**/.git/**",
        reason: "Version-control metadata.",
    },
];

/// Individual file names that are generated rather than written.
///
/// Lock files are the case that matters. `Cargo.lock` and `pnpm-lock.yaml` can
/// each run to tens of thousands of lines, and a repository whose largest
/// "source file" is its lock file has not been measured, it has been counted.
const EXCLUDED_FILES: &[Rule] = &[
    Rule {
        id: "generated.lockfile",
        expression: "Cargo.lock, package-lock.json, pnpm-lock.yaml, yarn.lock, poetry.lock, \
                     Gemfile.lock, composer.lock",
        reason: "A dependency lock file. Machine-written, often tens of thousands of lines, and \
                 not a measure of anything anybody wrote.",
    },
    Rule {
        id: "generated.minified",
        expression: "*.min.js, *.min.css",
        reason: "Minified output. One line can hold an entire library, which makes a line count \
                 of it meaningless in both directions.",
    },
];

/// The name every lock-file exclusion is recorded under.
const LOCKFILE_NAMES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "poetry.lock",
    "Gemfile.lock",
    "composer.lock",
];

/// Every rule in the policy, for a report that wants to publish the policy
/// itself rather than only the matches.
#[must_use]
pub fn rules() -> Vec<Rule> {
    EXCLUDED_DIRECTORIES
        .iter()
        .chain(EXCLUDED_FILES)
        .copied()
        .collect()
}

/// The rule that excludes `path`, if any.
///
/// `path` is relative to the extraction root, so it is shaped like the
/// repository — which is what lets a rule be written as `**/node_modules/**`
/// and mean what a reader thinks it means.
#[must_use]
pub fn excluded_by(path: &Path) -> Option<Rule> {
    // Segment-wise, so `my-vendor-lib` is not `vendor` and `web/node_modules`
    // is still `node_modules`.
    for segment in path {
        let segment = segment.to_string_lossy();
        if let Some(rule) = EXCLUDED_DIRECTORIES
            .iter()
            .find(|rule| directory_of(rule.id) == segment)
        {
            return Some(*rule);
        }
    }

    let name = path.file_name()?.to_string_lossy().into_owned();
    if LOCKFILE_NAMES.contains(&name.as_str()) {
        return EXCLUDED_FILES
            .iter()
            .find(|r| r.id == "generated.lockfile")
            .copied();
    }
    if name.ends_with(".min.js") || name.ends_with(".min.css") {
        return EXCLUDED_FILES
            .iter()
            .find(|r| r.id == "generated.minified")
            .copied();
    }
    None
}

/// The directory name a `vendored.*` / `build.*` / `generated.*` rule matches.
///
/// Derived from the identifier rather than stored twice, so the two cannot
/// drift apart — the failure mode being a rule whose name says `node_modules`
/// and whose pattern says something else.
fn directory_of(rule_id: &str) -> &str {
    rule_id.rsplit('.').next().unwrap_or(rule_id)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let rules = rules();
        let mut ids: Vec<&str> = rules.iter().map(|rule| rule.id).collect();
        ids.sort_unstable();
        let mut unique = ids.clone();
        unique.dedup();
        assert_eq!(ids, unique, "duplicate rule id");

        for rule in &rules {
            assert!(!rule.reason.is_empty(), "{} has no reason", rule.id);
            assert!(!rule.expression.is_empty(), "{} has no pattern", rule.id);
            assert!(
                !rule.reason.contains("  "),
                "a published sentence carries a run of spaces, which is a lost line \
                 continuation rather than typography: {:?}",
                rule.reason
            );
        }
    }

    #[test]
    fn a_directory_rule_matches_the_directory_its_name_claims() {
        // `directory_of` derives the segment from the identifier, so this pins
        // the two together: a rule called `vendored.node_modules` that stopped
        // matching `node_modules` would be a policy that lies about itself.
        for rule in EXCLUDED_DIRECTORIES {
            let segment = directory_of(rule.id);
            let path = format!("a/{segment}/b.rs");
            assert_eq!(
                excluded_by(Path::new(&path)).map(|matched| matched.id),
                Some(rule.id),
                "{} does not match {segment}",
                rule.id
            );
        }
    }
}
