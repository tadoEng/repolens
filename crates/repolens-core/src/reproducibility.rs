//! The reproducibility key.
//!
//! Two RepoLens runs are expected to agree only when every value below matches.
//! Any of them changing is a legitimate reason for a report to differ, and the
//! report must therefore carry them so a reader can tell "the repository
//! changed" apart from "RepoLens changed".
//!
//! The test for membership is narrow: **does changing this value change the
//! report?** If yes, it belongs here, otherwise it does not. A key that omits a
//! real input silently claims reproducibility it cannot deliver, which is worse
//! than having no key at all — the report would assert determinism while two
//! runs disagreed.
//!
//! # What this key does *not* determine
//!
//! Semantic inputs only. The key says what the analysis was asked to do; it
//! says nothing about the machine that did it.
//!
//! ```text
//! determines                        does not determine
//! ──────────                        ──────────────────
//! repository, commit, tree          host scheduling and contention
//! evidence API and version          whether a wall-clock ceiling was crossed
//! analyzer semantics                transient storage exhaustion
//! ruleset                           transient memory pressure
//! selection policy
//! counter and its version
//! exclusion policy
//! classification policy
//! ```
//!
//! This boundary is load-bearing rather than pedantic. Composition runs under a
//! wall-clock ceiling, so two runs with an identical key can legitimately
//! disagree: one finishes inside the limit and counts, the other crosses it on
//! a loaded host and reports `UNABLE_TO_VERIFY` with the limit and the observed
//! value. Both are correct. A key claiming to determine the whole report would
//! make one of them a bug.
//!
//! The claim this key actually supports is therefore bounded:
//!
//! > Given identical semantic inputs **and a composition run that completed
//! > within its limits**, the normalized result is identical.
//!
//! Not "every analysis with the same key produces a byte-identical report".
//! The difference is also what keeps a transient timeout from being cached and
//! served later as though it were a finding about the repository.

use serde::{Deserialize, Serialize};

use crate::repository::{CommitSha, RepositoryCoordinate, TreeSha};

/// The evidence source an analysis was built from, and its interface version.
///
/// GitHub isolates breaking changes into dated REST versions, so the same
/// commit read through two different API versions can yield different fields
/// and therefore different findings. Recording the version makes that
/// difference explainable rather than mysterious.
///
/// Modelled as a struct rather than a bare string so a future non-GitHub source
/// (a local directory, an uploaded archive) is a new `api` value instead of a
/// breaking change to the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSource {
    /// Identifier for the retrieval interface, e.g. `github-rest`.
    pub api: String,
    /// Version of that interface, e.g. `2026-03-10`.
    pub version: String,
}

impl EvidenceSource {
    /// Builds an evidence source from its two parts.
    pub fn new(api: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            api: api.into(),
            version: version.into(),
        }
    }
}

/// The tool that produced line counts, and its version.
///
/// Line counting is delegated (Tokei today), and counters change their language
/// definitions and comment handling between releases. Two runs of the same
/// commit under different counter versions can legitimately report different
/// numbers, so the counter identity is part of what makes a count reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionCounter {
    /// Counter name, e.g. `tokei`.
    pub counter: String,
    /// Exact counter version.
    pub version: String,
}

impl CompositionCounter {
    /// Builds a composition counter from its two parts.
    pub fn new(counter: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            counter: counter.into(),
            version: version.into(),
        }
    }
}

/// Everything that determines deterministic report output.
///
/// Deliberately *not* included: the archive tarball hash. GitHub does not
/// guarantee archive bytes are stable over time for a fixed commit, so keying
/// on it would break reproducibility rather than establish it. The archive is
/// ephemeral transport for line counting; the canonical identity is the
/// coordinate, commit, and root tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReproducibilityKey {
    /// Which repository was analyzed. Two repositories can share a commit SHA
    /// — a fork, or a commit present in both — and they are not the same
    /// analysis, so the coordinate is part of the identity rather than
    /// decoration.
    pub repository: RepositoryCoordinate,
    /// The exact commit that was analyzed.
    pub commit_sha: CommitSha,
    /// Root tree of that commit: what the collectors actually walked.
    pub tree_sha: TreeSha,
    /// Retrieval interface and version the evidence came through.
    pub source: EvidenceSource,
    /// Version of the analyzer that produced the report.
    pub analyzer_version: String,
    /// Version of the rule set that was evaluated.
    pub ruleset_version: String,
    /// Counter that produced line counts, absent when composition was not
    /// computed — for example when extraction exceeded its configured limit and
    /// the section reported `UNABLE_TO_VERIFY`. Nullable because "no counting
    /// happened" is a real, reproducible outcome, not a missing value.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub composition_counter: Option<CompositionCounter>,
    /// Version of the policy deciding which paths are excluded from counting.
    pub exclusion_policy_version: String,
    /// Version of the policy deciding each counted file's role and area.
    ///
    /// Separate from the exclusion policy because the two answer different
    /// questions — what was left out, and what the rest *is* — and either can
    /// change without the other. A changed classifier moves the production
    /// share without a single file changing.
    pub classification_policy_version: String,
    /// Version of the policy deciding **which files are read at all**, and in
    /// what order.
    ///
    /// The last of the three to be recorded here, and the one whose absence was
    /// least visible: selection decides the evidence every finding is drawn
    /// from, so changing it changes findings without changing a rule, a count,
    /// or a repository. The selection module has always said as much in prose.
    /// Until this field existed, that sentence had no mechanism behind it —
    /// selection could change, every version here could stay put, and two
    /// reports drawn from different evidence would claim to be comparable.
    pub selection_policy_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> ReproducibilityKey {
        ReproducibilityKey {
            repository: RepositoryCoordinate::new("rust-lang", "crates.io"),
            commit_sha: CommitSha::parse("0584a2df65968a4e9e6859ef46bbed430408a3f1").unwrap(),
            tree_sha: TreeSha::parse("4b825dc642cb6eb9a060e54bf8d69288fbee4904").unwrap(),
            source: EvidenceSource::new("github-rest", "2026-03-10"),
            analyzer_version: "0.1.0".into(),
            ruleset_version: "1".into(),
            composition_counter: Some(CompositionCounter::new("tokei", "14.0.0")),
            exclusion_policy_version: "1".into(),
            classification_policy_version: "1".into(),
            selection_policy_version: "1".into(),
        }
    }

    #[test]
    fn every_field_of_the_key_is_listed_here() {
        /*
         * An inventory, so that adding an input to the key is a deliberate act
         * with a second place to update, and *omitting* one is what fails.
         *
         * The failure this guards is the one the key already suffered: three
         * policies decided report output while only one of them appeared here.
         * Nothing broke, nothing failed, and the key quietly asserted a
         * reproducibility it could not deliver — which its own documentation
         * calls worse than having no key at all.
         *
         * The membership test is above: does changing this value change the
         * report? If a new constant answers yes, it belongs in the struct and
         * in this list. If it answers no, it belongs in neither.
         */
        let json = serde_json::to_value(key()).unwrap();
        let mut fields: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();

        assert_eq!(
            fields,
            [
                "analyzer_version",
                "classification_policy_version",
                "commit_sha",
                "composition_counter",
                "exclusion_policy_version",
                "repository",
                "ruleset_version",
                "selection_policy_version",
                "source",
                "tree_sha",
            ],
            "the reproducibility key gained or lost an input; if a policy now decides report \
             output, it belongs here and in this list"
        );
    }

    #[test]
    fn round_trips_through_json() {
        let original = key();
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ReproducibilityKey = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn absent_composition_counter_is_omitted_and_restored() {
        let mut original = key();
        original.composition_counter = None;

        let json = serde_json::to_string(&original).unwrap();
        assert!(
            !json.contains("composition_counter"),
            "an absent counter should be omitted entirely, not serialized as null"
        );

        let parsed: ReproducibilityKey = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn every_field_changes_the_key() {
        // The key's whole purpose is that a differing input yields a differing
        // key. A field that can change without changing equality would be a
        // silent reproducibility hole, so each one is exercised.
        let base = key();

        let mut repository = base.clone();
        repository.repository = RepositoryCoordinate::new("tadoEng", "repolens");
        assert_ne!(base, repository);

        let mut commit = base.clone();
        commit.commit_sha = CommitSha::parse("1111111111111111111111111111111111111111").unwrap();
        assert_ne!(base, commit);

        let mut tree = base.clone();
        tree.tree_sha = TreeSha::parse("2222222222222222222222222222222222222222").unwrap();
        assert_ne!(base, tree);

        let mut source = base.clone();
        source.source = EvidenceSource::new("github-rest", "2022-11-28");
        assert_ne!(base, source);

        let mut analyzer = base.clone();
        analyzer.analyzer_version = "0.2.0".into();
        assert_ne!(base, analyzer);

        let mut ruleset = base.clone();
        ruleset.ruleset_version = "2".into();
        assert_ne!(base, ruleset);

        let mut counter = base.clone();
        counter.composition_counter = Some(CompositionCounter::new("tokei", "13.0.0"));
        assert_ne!(base, counter);

        let mut exclusions = base.clone();
        exclusions.exclusion_policy_version = "2".into();
        assert_ne!(base, exclusions);

        let mut classification = base.clone();
        classification.classification_policy_version = "2".into();
        assert_ne!(base, classification);

        let mut selection = base.clone();
        selection.selection_policy_version = "2".into();
        assert_ne!(base, selection);
    }

    #[test]
    fn commit_and_tree_shas_are_not_interchangeable() {
        // Both are 40-hex digests. If they shared a type, transposing them would
        // compile and produce a wrong-but-plausible key.
        let json = serde_json::to_string(&key()).unwrap();
        let swapped = json
            .replace("0584a2df65968a4e9e6859ef46bbed430408a3f1", "PLACEHOLDER")
            .replace(
                "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
                "0584a2df65968a4e9e6859ef46bbed430408a3f1",
            )
            .replace("PLACEHOLDER", "4b825dc642cb6eb9a060e54bf8d69288fbee4904");

        let parsed: ReproducibilityKey = serde_json::from_str(&swapped).unwrap();
        assert_ne!(key(), parsed);
    }
}
