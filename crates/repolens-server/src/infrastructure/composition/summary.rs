//! Turning a count into what the report publishes.
//!
//! The counter answers *how many lines, of what language*. A reader is asking
//! something slightly different — where the code lives, what kind of code it
//! is, and which files are worth opening first — and every one of those answers
//! is a projection of the same manifest through the classification policy.
//!
//! Derived here rather than in [`counter`](super::counter) on purpose. The
//! counter's output is what Tokei saw; this is what RepoLens says about it, and
//! the two versions in the reproducibility key are different numbers precisely
//! because they can change independently. A classifier that moves the
//! production share has changed no count at all.
//!
//! # Every order here is decided, not inherited
//!
//! The contract says these lists are server-ordered, and a filesystem walk
//! promises nothing. Two runs of one commit must publish byte-identical
//! sections, so each ordering below is total: a primary key that is useful to a
//! reader, and a tie-break that is arbitrary but fixed.

use std::collections::BTreeMap;
use std::path::Path;

use super::classification::{self, CLASSIFICATION_POLICY_VERSION};
use super::counter::{COUNTER_NAME, CountedRepository};
use crate::contract::report::{
    AreaLineCount, CodeRole, CompositionExclusion, LanguageLineCount, LargestSourceFile,
    LargestSourceFiles, LineCountSummary, RoleLineCount,
};

/// Where a role sits in the order a report lists them.
///
/// Fixed rather than derived from the data, and that is the point: a reader
/// comparing two reports should find the same rows in the same places, which an
/// ordering by size would not give them. Production first, because it is the row
/// every other one is read against.
///
/// A `match` rather than a position in a list. The exhaustiveness is the whole
/// value — a role added to the contract fails to compile here, instead of
/// landing in an arbitrary place or panicking on the first repository that has
/// one.
const fn rank_of(role: CodeRole) -> usize {
    match role {
        CodeRole::Production => 0,
        CodeRole::Test => 1,
        CodeRole::Generated => 2,
        CodeRole::Tooling => 3,
        CodeRole::Unclassified => 4,
    }
}

/// What one counted repository publishes as its composition section.
///
/// Total counts come from the language table rather than from the manifest,
/// because the manifest carries code lines only. Comment and blank lines are
/// real parts of a file and a total that silently omitted them would be a
/// smaller, wronger number wearing the same name.
#[must_use]
pub fn summarize(counted: &CountedRepository) -> LineCountSummary {
    let languages = language_rows(counted);

    let code_lines = languages.iter().map(|row| row.code_lines).sum();
    let comment_lines = languages.iter().map(|row| row.comment_lines).sum();
    let blank_lines = languages.iter().map(|row| row.blank_lines).sum();

    LineCountSummary {
        counter: COUNTER_NAME.to_owned(),
        counter_version: counted.tokei_version.to_owned(),
        exclusion_policy_version: counted.exclusion_policy_version.to_owned(),
        // Not carried on `CountedRepository`, because counting never consults
        // it. Everything below that needs a role or an area is decided here, so
        // this is where the version that decided it comes from.
        classification_policy_version: CLASSIFICATION_POLICY_VERSION.to_owned(),
        total_files: counted.composition.counted_files,
        // Physical lines: what a reader sees on opening the file.
        total_lines: code_lines + comment_lines + blank_lines,
        code_lines,
        comment_lines,
        blank_lines,
        languages,
        areas: area_rows(counted),
        exclusions: exclusion_rows(counted),
        roles: role_rows(counted),
        largest_files: largest_files(counted),
        unclassified_files: counted
            .manifest
            .iter()
            .filter(|file| role_of(&file.path) == CodeRole::Unclassified)
            .count() as u64,
    }
}

/// Per-language rows, largest first.
///
/// The counter hands these over in name order, which is a fine way to store
/// them and a poor way to read them: the question is what this repository is
/// mostly written in, and the answer should be the first row.
fn language_rows(counted: &CountedRepository) -> Vec<LanguageLineCount> {
    let mut rows: Vec<LanguageLineCount> = counted
        .composition
        .languages
        .iter()
        .map(|language| LanguageLineCount {
            language: language.language.clone(),
            files: language.files,
            code_lines: language.code,
            comment_lines: language.comments,
            blank_lines: language.blanks,
        })
        .collect();

    rows.sort_by(|left, right| {
        right
            .code_lines
            .cmp(&left.code_lines)
            .then_with(|| left.language.cmp(&right.language))
    });
    rows
}

/// Per-area rows, largest first.
///
/// Areas are accumulated from the manifest rather than asked of the counter,
/// which has no idea what an area is. Code lines only, because that is the
/// number the section compares across areas and summing comments into it would
/// make a heavily documented area look like a larger one.
fn area_rows(counted: &CountedRepository) -> Vec<AreaLineCount> {
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for file in &counted.manifest {
        *totals
            .entry(classification::area_of(Path::new(&file.path)))
            .or_default() += file.code;
    }

    let mut rows: Vec<AreaLineCount> = totals
        .into_iter()
        .map(|(area, code_lines)| AreaLineCount { area, code_lines })
        .collect();

    rows.sort_by(|left, right| {
        right
            .code_lines
            .cmp(&left.code_lines)
            .then_with(|| left.area.cmp(&right.area))
    });
    rows
}

/// Per-role rows, in [`rank_of`] order, omitting roles nothing matched.
///
/// A role with no files is left out rather than published as a row of zeros: a
/// zero reads as a measurement, and "this repository has no generated code" is
/// better said by the row's absence than by a number a reader has to interpret.
///
/// Keyed by rank rather than by role, so the `BTreeMap` sorts the output and
/// there is no second place where the order is written down.
fn role_rows(counted: &CountedRepository) -> Vec<RoleLineCount> {
    let mut totals: BTreeMap<usize, RoleLineCount> = BTreeMap::new();
    for file in &counted.manifest {
        let role = role_of(&file.path);
        let row = totals.entry(rank_of(role)).or_insert(RoleLineCount {
            role,
            files: 0,
            code_lines: 0,
        });
        row.files += 1;
        row.code_lines += file.code;
    }

    totals.into_values().collect()
}

/// The largest files by code, descending, bounded by the contract's ceiling.
///
/// Ties broken by path so the list is total. Without it two files of equal size
/// could swap places between runs of one commit, which is a difference a
/// determinism comparison would report as a defect.
fn largest_files(counted: &CountedRepository) -> LargestSourceFiles {
    let mut rows: Vec<LargestSourceFile> = counted
        .manifest
        .iter()
        .map(|file| LargestSourceFile {
            path: file.path.clone(),
            language: file.language.clone(),
            code_lines: file.code,
            // Carried so a large generated file is not read as a large
            // hand-written one, which is the most common way this list misleads.
            role: role_of(&file.path),
        })
        .collect();

    rows.sort_by(|left, right| {
        right
            .code_lines
            .cmp(&left.code_lines)
            .then_with(|| left.path.cmp(&right.path))
    });

    // `truncated_from` rather than `new`: the caller has ranked every file and
    // is asking for the head of that ranking, which is explicit truncation
    // rather than the silent shortening `new` refuses to do.
    LargestSourceFiles::truncated_from(rows)
}

/// The exclusion ledger, in the order the policy produced it.
///
/// Not re-sorted. The counter builds this while partitioning, and the ledger's
/// job is to be complete rather than ranked — reordering it here would add a
/// second ordering rule to maintain for no reader's benefit.
fn exclusion_rows(counted: &CountedRepository) -> Vec<CompositionExclusion> {
    counted
        .composition
        .exclusions
        .iter()
        .map(|exclusion| CompositionExclusion {
            path_or_rule: exclusion.path_or_rule.clone(),
            reason: exclusion.reason.clone(),
            matched_rule: exclusion.matched_rule.clone(),
            file_count: exclusion.file_count,
            bytes: exclusion.bytes,
        })
        .collect()
}

/// The role of one manifest path.
fn role_of(path: &str) -> CodeRole {
    classification::role_of(Path::new(path))
}

#[cfg(test)]
mod tests {
    use repolens_core::{
        CompositionExclusion as DomainExclusion, LanguageComposition, RepositoryComposition,
    };

    use super::super::counter::{COUNTER_NAME, CountedFile, TOKEI_VERSION};
    use super::*;
    use crate::contract::report::MAX_LARGEST_FILES;

    fn counted(
        manifest: Vec<CountedFile>,
        languages: Vec<LanguageComposition>,
    ) -> CountedRepository {
        let counted_files = manifest.len() as u64;
        CountedRepository {
            composition: RepositoryComposition {
                languages,
                counted_files,
                exclusions: Vec::new(),
            },
            manifest,
            tokei_version: TOKEI_VERSION,
            exclusion_policy_version: "1",
        }
    }

    fn file(path: &str, language: &str, code: u64) -> CountedFile {
        CountedFile {
            path: path.to_owned(),
            language: language.to_owned(),
            code,
        }
    }

    fn language(
        name: &str,
        files: u64,
        code: u64,
        comments: u64,
        blanks: u64,
    ) -> LanguageComposition {
        LanguageComposition {
            language: name.to_owned(),
            files,
            code,
            comments,
            blanks,
        }
    }

    #[test]
    fn totals_count_comments_and_blanks_as_lines_of_the_file() {
        // The number answering "how big is this repository" is what a reader
        // would see on opening the files. A total that quietly meant "code
        // only" would be a smaller, wronger number wearing the same name, and
        // `code_lines` already answers the other question.
        let summary = summarize(&counted(
            vec![file("src/main.rs", "Rust", 60)],
            vec![language("Rust", 1, 60, 30, 10)],
        ));

        assert_eq!(summary.code_lines, 60);
        assert_eq!(summary.comment_lines, 30);
        assert_eq!(summary.blank_lines, 10);
        assert_eq!(summary.total_lines, 100);
    }

    #[test]
    fn languages_and_areas_lead_with_the_largest() {
        // The counter stores languages in name order, which is a fine way to
        // hold them and a poor way to read them: the question this section
        // answers is what the repository is mostly made of.
        let summary = summarize(&counted(
            vec![
                file("web/app.ts", "TypeScript", 900),
                file("crates/a/src/lib.rs", "Rust", 100),
            ],
            vec![
                language("Rust", 1, 100, 0, 0),
                language("TypeScript", 1, 900, 0, 0),
            ],
        ));

        assert_eq!(
            summary
                .languages
                .iter()
                .map(|row| row.language.as_str())
                .collect::<Vec<_>>(),
            ["TypeScript", "Rust"],
            "name order is the counter's; size order is the report's"
        );
        assert_eq!(
            summary
                .areas
                .iter()
                .map(|row| row.area.as_str())
                .collect::<Vec<_>>(),
            ["web/", "crates/"]
        );
    }

    #[test]
    fn roles_keep_a_fixed_order_rather_than_a_size_order() {
        // Two reports must put the same rows in the same places, or comparing
        // them means reading both legends first. Production leads even when it
        // is by far the smallest.
        let summary = summarize(&counted(
            vec![
                file("web/src/generated/schema.ts", "TypeScript", 5_000),
                file("crates/a/tests/it.rs", "Rust", 400),
                file("crates/a/src/lib.rs", "Rust", 10),
            ],
            vec![
                language("Rust", 2, 410, 0, 0),
                language("TypeScript", 1, 5_000, 0, 0),
            ],
        ));

        let roles: Vec<CodeRole> = summary.roles.iter().map(|row| row.role).collect();
        assert_eq!(
            roles,
            [CodeRole::Production, CodeRole::Test, CodeRole::Generated]
        );
        // A role nothing matched is absent rather than a row of zeros: a zero
        // reads as a measurement, and there was nothing to measure.
        assert!(!roles.contains(&CodeRole::Tooling));
    }

    #[test]
    fn the_largest_files_list_is_bounded_and_totally_ordered() {
        // Ties broken by path. Without it two files of equal size could swap
        // places between runs of one commit — a difference a determinism
        // comparison reports as a defect in a report that is in fact correct.
        let mut manifest: Vec<CountedFile> = (0..MAX_LARGEST_FILES + 5)
            .map(|index| file(&format!("src/f{index:02}.rs"), "Rust", 100))
            .collect();
        manifest.push(file("src/biggest.rs", "Rust", 999));

        let summary = summarize(&counted(manifest, vec![language("Rust", 16, 2_499, 0, 0)]));
        let rows = summary.largest_files.as_slice();

        assert_eq!(
            rows.len(),
            MAX_LARGEST_FILES,
            "the contract's ceiling holds"
        );
        assert_eq!(rows[0].path, "src/biggest.rs");
        // Equal sizes below the head, so the tie-break alone decides — and it
        // must decide the same way every time.
        assert_eq!(rows[1].path, "src/f00.rs");
        assert_eq!(rows[2].path, "src/f01.rs");
    }

    #[test]
    fn a_large_generated_file_is_labelled_rather_than_left_to_look_hand_written() {
        // The most common way this list misleads: the biggest file in a
        // repository is very often one nobody wrote.
        let summary = summarize(&counted(
            vec![file("web/src/generated/schema.ts", "TypeScript", 5_000)],
            vec![language("TypeScript", 1, 5_000, 0, 0)],
        ));

        assert_eq!(
            summary.largest_files.as_slice()[0].role,
            CodeRole::Generated
        );
    }

    #[test]
    fn an_empty_repository_summarizes_to_zero_rather_than_to_nothing() {
        // A repository that is entirely excluded content is a real outcome and
        // its answer is zero. Nothing here may divide by a total or index a
        // first row.
        let summary = summarize(&counted(Vec::new(), Vec::new()));

        assert_eq!(summary.total_files, 0);
        assert_eq!(summary.total_lines, 0);
        assert!(summary.languages.is_empty());
        assert!(summary.areas.is_empty());
        assert!(summary.roles.is_empty());
        assert!(summary.largest_files.is_empty());
    }

    #[test]
    fn the_versions_published_are_the_ones_that_decided_the_numbers() {
        // Three policies decide this section and they move independently. The
        // classification version comes from here rather than from the count,
        // because counting never consults it.
        let summary = summarize(&counted(
            vec![file("src/main.rs", "Rust", 1)],
            vec![language("Rust", 1, 1, 0, 0)],
        ));

        assert_eq!(summary.counter, COUNTER_NAME);
        assert_eq!(summary.counter_version, TOKEI_VERSION);
        assert_eq!(
            summary.classification_policy_version,
            CLASSIFICATION_POLICY_VERSION
        );
    }

    #[test]
    fn the_exclusion_ledger_is_carried_whole() {
        // The ledger is what makes a LOC number readable at all. A dropped row
        // here would make the counts look like the whole repository.
        let mut repository = counted(Vec::new(), Vec::new());
        repository.composition.exclusions = vec![DomainExclusion {
            path_or_rule: "**/node_modules/**".to_owned(),
            reason: "Vendored dependencies are not this repository's code.".to_owned(),
            matched_rule: "vendored.node_modules".to_owned(),
            file_count: 126,
            bytes: 4 * 1024 * 1024,
        }];

        let summary = summarize(&repository);

        assert_eq!(summary.exclusions.len(), 1);
        assert_eq!(summary.exclusions[0].matched_rule, "vendored.node_modules");
        assert_eq!(summary.exclusions[0].file_count, 126);
    }
}
