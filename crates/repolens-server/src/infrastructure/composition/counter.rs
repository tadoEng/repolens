//! Counting lines, and saying exactly what was counted.
//!
//! The analyzer depends on *counts*, never on Tokei. That is why this
//! implements [`RepositoryCompositionCounter`] — a contract in `repolens-core`
//! that mentions no library — rather than exposing Tokei's types upward. A rule
//! that reached for the counter directly would make replacing it a change to
//! the meaning of a report rather than to how one is produced.
//!
//! Tokei is used as a library. Never `Command::new("tokei")`: a binary the
//! container might not have, at a version nobody recorded, is not a
//! reproducible measurement.
//!
//! # What makes two runs agree
//!
//! Three things, and they are persisted together:
//!
//! * the commit SHA — what was counted;
//! * [`TOKEI_VERSION`] — the language definitions that decided what is Rust;
//! * [`EXCLUSION_POLICY_VERSION`] — what was deliberately left out.
//!
//! Not the tarball's hash. GitHub does not guarantee the archive for a fixed
//! commit is byte-stable over time, so recording its digest would make two
//! honest runs of the same commit disagree — breaking reproducibility while
//! looking exactly like a proof of it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use repolens_core::{
    CompositionExclusion, CompositionOutcome, LanguageComposition, RepositoryComposition,
    RepositoryCompositionCounter,
};

use super::exclusion::{self, EXCLUSION_POLICY_VERSION};

/// The Tokei release these counts were produced by.
///
/// Written here **and** pinned in `Cargo.toml`, which is a duplication and
/// therefore a place a report can start lying: bump the dependency, forget this
/// line, and every result claims counts from a version that did not produce
/// them. Tokei has no public constant to read, and a build script to synthesise
/// one is a lot of machinery for one string.
///
/// So the duplication is kept and *gated*. `tests/tokei_version.rs` reads the
/// version out of `Cargo.lock` and the requirement out of `Cargo.toml`, and
/// fails if either disagrees with this — or if the requirement stops being an
/// exact pin. The constant cannot drift silently; it can only drift through a
/// red test.
///
/// The pin matters because Tokei's language detection *is* the definition of
/// what counts as Rust: a minor release that reclassifies one extension changes
/// every report, which is exactly the kind of change a reproducibility key has
/// to be able to explain.
pub const TOKEI_VERSION: &str = "14.0.0";

/// The counter's name, as the report and the reproducibility key publish it.
///
/// Beside the version rather than spelled at the two places that publish it:
/// they are one identity, and a report naming a counter the key does not would
/// be two answers to the question of what produced the numbers.
pub const COUNTER_NAME: &str = "tokei";

/// One file that was counted, for the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountedFile {
    /// Path relative to the repository root.
    pub path: String,
    /// Language Tokei attributed it to.
    pub language: String,
    /// Lines of code.
    pub code: u64,
}

/// Everything one count produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountedRepository {
    /// The normalized result the report publishes.
    pub composition: RepositoryComposition,
    /// Every file counted, in path order.
    ///
    /// The manifest issue #12 requires: two runs that disagree about a total
    /// can be diffed to the file that differs, which a total alone never
    /// permits.
    pub manifest: Vec<CountedFile>,
    /// Tokei release used.
    pub tokei_version: &'static str,
    /// Exclusion policy applied.
    pub exclusion_policy_version: &'static str,
}

/// Counts an extracted tree with Tokei, honouring the exclusion policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokeiCounter;

/// Counting failed for a reason that is not a limit.
#[derive(Debug, thiserror::Error)]
pub enum CountError {
    /// The tree could not be walked.
    #[error("could not read the extracted tree")]
    Io(#[source] std::io::Error),
}

impl RepositoryCompositionCounter for TokeiCounter {
    type Error = CountError;

    fn count_composition(&self, root: &Path) -> Result<CompositionOutcome, Self::Error> {
        Ok(CompositionOutcome::Counted(self.count(root)?.composition))
    }
}

impl TokeiCounter {
    /// Counts `root`, returning the manifest and versions as well.
    ///
    /// [`RepositoryCompositionCounter`] returns only the normalized result,
    /// because that is all a rule may depend on. The pipeline needs the
    /// manifest and the versions to persist them, so it calls this.
    ///
    /// # Errors
    ///
    /// [`CountError::Io`] when the extracted tree cannot be walked.
    pub fn count(&self, root: &Path) -> Result<CountedRepository, CountError> {
        let (kept, excluded) = partition(root)?;

        // Tokei is not called with nothing to do.
        //
        // `get_statistics(&[], ..)` panics inside the crate — `utils/fs.rs`
        // indexes the path list without checking it — and this is reachable in
        // production, not only in a test: a repository that is entirely
        // `node_modules`, or a fork holding nothing but a lock file, leaves the
        // policy with no paths to hand over. A panic there would take the
        // worker down and strand the analysis with no diagnosis, which is the
        // exact failure the extraction limits were built to avoid.
        //
        // An empty repository has an answer, and the answer is zero.
        if kept.is_empty() {
            return Ok(CountedRepository {
                composition: RepositoryComposition {
                    languages: Vec::new(),
                    counted_files: 0,
                    exclusions: excluded,
                },
                manifest: Vec::new(),
                tokei_version: TOKEI_VERSION,
                exclusion_policy_version: EXCLUSION_POLICY_VERSION,
            });
        }

        let mut languages = tokei::Languages::new();

        /*
         * What actually keeps a repository from deciding its own size is the
         * *walk*, not this config.
         *
         * `partition` above traverses the tree itself and hands Tokei a flat
         * list of explicit file paths. Tokei consults `.gitignore` while
         * traversing directories, and it is never asked to traverse, so the
         * ignore files never get a vote. That matters because a `.gitignore` is
         * written by the repository under analysis: honouring it would let a
         * project with `src/` in its ignore file report itself as empty, and
         * the report would have no way to say why.
         *
         * The flags below are therefore belt-and-braces rather than the
         * mechanism — they would start mattering the moment somebody changed
         * `partition` to hand over a directory. They are kept for that day and
         * documented as inert today, because a comment claiming they do the
         * work is how the next reader deletes the walk and keeps the flags.
         *
         * Found by tamper-check: replacing all of this with
         * `Config::default()` — which honours ignore files — changed nothing at
         * all, and the test that was supposed to cover it passed either way.
         */
        let config = tokei::Config {
            hidden: Some(true),
            no_ignore: Some(true),
            no_ignore_parent: Some(true),
            no_ignore_dot: Some(true),
            no_ignore_vcs: Some(true),
            ..tokei::Config::default()
        };
        let paths: Vec<&Path> = kept.iter().map(PathBuf::as_path).collect();
        languages.get_statistics(&paths, &[], &config);

        let mut manifest = Vec::new();
        let mut totals: BTreeMap<String, LanguageComposition> = BTreeMap::new();

        for (language_type, language) in &languages {
            let name = language_type.name().to_owned();
            for report in &language.reports {
                manifest.push(CountedFile {
                    path: relative(root, &report.name),
                    language: name.clone(),
                    code: report.stats.code as u64,
                });
            }
            totals.insert(
                name.clone(),
                LanguageComposition {
                    language: name,
                    files: language.reports.len() as u64,
                    code: language.code as u64,
                    comments: language.comments as u64,
                    blanks: language.blanks as u64,
                },
            );
        }

        // Sorted, and not incidentally. Two runs at one commit must produce
        // byte-identical results, and a filesystem walk does not promise an
        // order — `BTreeMap` fixes the languages and this fixes the files.
        manifest.sort_by(|left, right| left.path.cmp(&right.path));

        let counted_files = manifest.len() as u64;
        Ok(CountedRepository {
            composition: RepositoryComposition {
                languages: totals.into_values().collect(),
                counted_files,
                exclusions: excluded,
            },
            manifest,
            tokei_version: TOKEI_VERSION,
            exclusion_policy_version: EXCLUSION_POLICY_VERSION,
        })
    }
}

/// Splits the tree into what is counted and what the policy excluded.
///
/// The walk happens here rather than being left to Tokei so that an exclusion
/// is *recorded* rather than merely not counted. "We left out 4,812 files under
/// `node_modules`" is a fact a reader can weigh; a smaller number with no
/// explanation is the thing that makes LOC untrustworthy.
fn partition(root: &Path) -> Result<(Vec<PathBuf>, Vec<CompositionExclusion>), CountError> {
    let mut kept = Vec::new();
    let mut ledger: BTreeMap<&'static str, (exclusion::Rule, u64, u64)> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(directory) = stack.pop() {
        let entries = std::fs::read_dir(&directory).map_err(CountError::Io)?;
        for entry in entries {
            let entry = entry.map_err(CountError::Io)?;
            let path = entry.path();
            let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

            // `symlink_metadata`, so a link is never followed. The extractor
            // refuses to write links at all, but this walk is over a directory
            // and does not get to assume who filled it.
            let metadata = std::fs::symlink_metadata(&path).map_err(CountError::Io)?;
            if metadata.is_symlink() {
                continue;
            }

            if let Some(rule) = exclusion::excluded_by(&relative_path) {
                let record = ledger.entry(rule.id).or_insert((rule, 0, 0));
                if metadata.is_dir() {
                    let (files, bytes) = measure(&path)?;
                    record.1 += files;
                    record.2 += bytes;
                } else {
                    record.1 += 1;
                    record.2 += metadata.len();
                }
                continue;
            }

            if metadata.is_dir() {
                stack.push(path);
            } else {
                kept.push(path);
            }
        }
    }

    kept.sort();
    let exclusions = ledger
        .into_values()
        .map(|(rule, file_count, bytes)| CompositionExclusion {
            path_or_rule: rule.expression(),
            reason: rule.reason.to_owned(),
            matched_rule: rule.id.to_owned(),
            file_count,
            bytes,
        })
        .collect();

    Ok((kept, exclusions))
}

/// How many files an excluded directory held, and how many bytes.
///
/// Counted rather than skipped, because the size of what was left out is the
/// number that tells a reader whether the exclusion mattered.
fn measure(directory: &Path) -> Result<(u64, u64), CountError> {
    let mut files = 0;
    let mut bytes = 0;
    let mut stack = vec![directory.to_path_buf()];

    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).map_err(CountError::Io)? {
            let entry = entry.map_err(CountError::Io)?;
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(CountError::Io)?;
            if metadata.is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                files += 1;
                bytes += metadata.len();
            }
        }
    }

    Ok((files, bytes))
}

/// A counted path, relative to the repository root, with forward slashes.
///
/// Forward slashes on every platform: the path goes into a stored report, and a
/// report produced on Windows must not differ from one produced on Linux for a
/// reason as incidental as a separator.
fn relative(root: &Path, counted: &Path) -> String {
    counted
        .strip_prefix(root)
        .unwrap_or(counted)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
