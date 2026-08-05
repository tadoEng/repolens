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
        }
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
