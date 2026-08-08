//! Counting a tree, and what the number is allowed to mean.
//!
//! Trees are built here rather than committed, so each test states the exact
//! repository it is a claim about.

use std::path::Path;

use repolens_core::{CompositionOutcome, RepositoryCompositionCounter};
use repolens_server::infrastructure::composition::counter::{TOKEI_VERSION, TokeiCounter};
use repolens_server::infrastructure::composition::exclusion::EXCLUSION_POLICY_VERSION;

/// Writes `files` into a fresh directory and returns it.
fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a scratch directory");
    for (path, contents) in files {
        let full = directory.path().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("the parent is creatable");
        }
        std::fs::write(&full, contents).expect("the file is writable");
    }
    directory
}

const RUST: &str = "// a comment\nfn main() {\n    println!(\"hi\");\n}\n\n";
const TYPESCRIPT: &str = "// a comment\nexport const x = 1;\n\n";

#[test]
fn languages_are_counted_and_attributed() {
    let repository = tree(&[
        ("src/main.rs", RUST),
        ("web/src/app.ts", TYPESCRIPT),
        ("README.md", "# hello\n"),
    ]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    let names: Vec<&str> = counted
        .composition
        .languages
        .iter()
        .map(|language| language.language.as_str())
        .collect();
    assert!(names.contains(&"Rust"), "{names:?}");
    assert!(names.contains(&"TypeScript"), "{names:?}");

    let rust = counted
        .composition
        .languages
        .iter()
        .find(|language| language.language == "Rust")
        .expect("Rust is counted");
    assert_eq!(rust.files, 1);
    assert!(rust.code > 0, "{rust:?}");
    assert!(rust.comments > 0, "the comment line is counted as one");
    assert!(rust.blanks > 0, "the blank line is counted as one");
}

#[test]
fn the_result_carries_the_versions_that_decided_it() {
    // Issue #12: the persisted result names the Tokei release and the exclusion
    // policy, and never the tarball hash. Two runs that disagree about a
    // repository's size have to be able to say which of the three inputs moved.
    let repository = tree(&[("src/main.rs", RUST)]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    assert_eq!(counted.tokei_version, TOKEI_VERSION);
    assert_eq!(counted.exclusion_policy_version, EXCLUSION_POLICY_VERSION);
    assert!(
        !TOKEI_VERSION.is_empty() && TOKEI_VERSION.chars().next().is_some_and(char::is_numeric),
        "the version has to be a version: {TOKEI_VERSION}"
    );
}

#[test]
fn two_runs_over_one_tree_produce_identical_results() {
    // The property the whole feature rests on. A filesystem walk promises no
    // order, so without the sorting in the counter this passes by luck on a
    // small tree and fails on a real one.
    let repository = tree(&[
        ("z/last.rs", RUST),
        ("a/first.rs", RUST),
        ("m/middle.ts", TYPESCRIPT),
        ("web/node_modules/dep/index.js", "module.exports = 1;\n"),
        ("Cargo.lock", "# generated\nname = \"x\"\n"),
    ]);

    let first = TokeiCounter.count(repository.path()).expect("counting");
    let second = TokeiCounter.count(repository.path()).expect("counting");

    assert_eq!(first.composition, second.composition);
    assert_eq!(first.manifest, second.manifest);
}

#[test]
fn the_manifest_names_every_counted_file_in_a_fixed_order() {
    // A total nobody can decompose is a total nobody can check. The manifest is
    // what lets two disagreeing runs be diffed to the file that differs.
    let repository = tree(&[
        ("z/last.rs", RUST),
        ("a/first.rs", RUST),
        ("m/middle.ts", TYPESCRIPT),
    ]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    let paths: Vec<&str> = counted
        .manifest
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, vec!["a/first.rs", "m/middle.ts", "z/last.rs"]);
    assert_eq!(counted.composition.counted_files, 3);

    // Forward slashes on every platform: the manifest is persisted, and a
    // report produced on Windows must not differ from one produced on Linux
    // over something as incidental as a separator.
    for path in paths {
        assert!(!path.contains('\\'), "{path}");
    }
}

#[test]
fn excluded_directories_are_recorded_rather_than_silently_dropped() {
    /*
     * The failure LOC reporting is most prone to. A repository whose
     * `node_modules` is counted is not a large repository, it is a repository
     * with dependencies — and a smaller number with no explanation is exactly
     * as untrustworthy in the other direction.
     */
    let repository = tree(&[
        ("src/main.rs", RUST),
        ("node_modules/left-pad/index.js", "module.exports = 1;\n"),
        ("node_modules/left-pad/extra.js", "module.exports = 2;\n"),
        ("web/node_modules/other/index.js", "module.exports = 3;\n"),
    ]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    // Nothing under an excluded directory reached the count.
    for file in &counted.manifest {
        assert!(!file.path.contains("node_modules"), "{}", file.path);
    }

    let exclusion = counted
        .composition
        .exclusions
        .iter()
        .find(|entry| entry.matched_rule == "vendored.node_modules")
        .expect("the exclusion is published");
    assert_eq!(
        exclusion.file_count, 3,
        "every excluded file is counted, at any depth"
    );
    assert!(exclusion.bytes > 0);
    assert!(!exclusion.reason.is_empty());
}

#[test]
fn a_lock_file_is_excluded_and_says_so() {
    // The single largest distortion available to a headline number: a lock file
    // can be tens of thousands of machine-written lines.
    let repository = tree(&[
        ("src/main.rs", RUST),
        ("Cargo.lock", "# generated\nname = \"x\"\nversion = \"1\"\n"),
        ("web/pnpm-lock.yaml", "lockfileVersion: 9\n"),
    ]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    for file in &counted.manifest {
        assert!(!file.path.contains("Cargo.lock"), "{}", file.path);
        assert!(!file.path.contains("pnpm-lock.yaml"), "{}", file.path);
    }
    let exclusion = counted
        .composition
        .exclusions
        .iter()
        .find(|entry| entry.matched_rule == "generated.lockfile")
        .expect("the exclusion is published");
    assert_eq!(exclusion.file_count, 2);
}

#[test]
fn a_repositorys_own_gitignore_cannot_shrink_its_line_count() {
    /*
     * Tokei honours `.gitignore` while traversing, and a `.gitignore` is
     * written by the repository under analysis — so a project naming `src/` in
     * one could report itself as empty, and the report would have no way to say
     * why.
     *
     * The guarantee comes from the counter doing its own walk and handing Tokei
     * explicit file paths, never a directory: there is no traversal for an
     * ignore file to influence. Every exclusion comes from the versioned policy
     * instead, which nothing outside this binary can reach.
     *
     * Stated that way because it was originally credited to Tokei's
     * `no_ignore` flags, and a tamper that removed them changed nothing —
     * they are inert while the walk is ours.
     */
    let repository = tree(&[
        ("src/main.rs", RUST),
        ("src/other.rs", RUST),
        (".gitignore", "src/\n*.rs\n"),
    ]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    let rust = counted
        .composition
        .languages
        .iter()
        .find(|language| language.language == "Rust")
        .expect("the Rust files are still counted");
    assert_eq!(
        rust.files, 2,
        "a repository's own ignore file must not decide its size"
    );
}

#[test]
fn the_domain_contract_yields_only_the_normalized_result() {
    // A rule may depend on counts and on nothing else. The trait returns the
    // composition alone — no manifest, no versions, no Tokei types — so a rule
    // cannot accidentally acquire a dependency on how counting happens.
    let repository = tree(&[("src/main.rs", RUST)]);

    let outcome = TokeiCounter
        .count_composition(repository.path())
        .expect("counting");

    let CompositionOutcome::Counted(composition) = outcome else {
        panic!("a countable tree is counted");
    };
    assert_eq!(composition.counted_files, 1);
}

#[test]
fn a_repository_of_documentation_is_counted_as_documentation() {
    /*
     * Tokei counts Markdown, and this reports **composition** rather than
     * source lines only — so a README is part of what a repository is made of
     * and appears under its own language. A reader sees it separately and can
     * weigh it; folding it into one "lines of code" figure is what would
     * mislead.
     *
     * Written first as an assertion that a README-only tree counts zero, which
     * was a guess about Tokei rather than a decision about the report.
     */
    let repository = tree(&[("README.md", "# hello\n")]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    assert_eq!(counted.composition.counted_files, 1);
    assert_eq!(
        counted
            .composition
            .languages
            .iter()
            .map(|language| language.language.as_str())
            .collect::<Vec<_>>(),
        vec!["Markdown"]
    );
}

#[test]
fn a_repository_with_nothing_in_it_is_counted_as_nothing() {
    // Zero is an answer. An empty tree is not an error, and reporting one would
    // make an honest edge case look like a fault.
    let repository = tree(&[]);

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    assert_eq!(counted.composition.counted_files, 0);
    assert!(counted.manifest.is_empty());
    assert!(counted.composition.languages.is_empty());
}

#[test]
fn a_symlink_is_never_followed_out_of_the_tree() {
    // The extractor refuses to write links, but this walk is over a directory
    // and does not get to assume who filled it. Following one would count files
    // that are not in the repository at all.
    let repository = tree(&[("src/main.rs", RUST)]);
    let outside = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(outside.path().join("elsewhere.rs"), RUST).expect("writable");

    let link = repository.path().join("linked");
    let made = symlink_dir(outside.path(), &link);
    if !made {
        // Windows needs Developer Mode or elevation to create a directory
        // symlink. Skipping is stated rather than silent: the assertion below
        // is the point of the test, and pretending it ran would be worse than
        // saying it did not.
        eprintln!("skipped: this platform did not permit creating a directory symlink");
        return;
    }

    let counted = TokeiCounter.count(repository.path()).expect("counting");

    assert_eq!(counted.composition.counted_files, 1);
    for file in &counted.manifest {
        assert!(!file.path.contains("linked"), "{}", file.path);
    }
}

/// Creates a directory symlink, reporting whether the platform allowed it.
fn symlink_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
}
