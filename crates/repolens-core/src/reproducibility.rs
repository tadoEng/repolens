//! The reproducibility key.
//!
//! Two RepoLens runs are expected to agree only when all four values below
//! match. Any of them changing is a legitimate reason for a report to differ,
//! and the report must therefore carry them so a reader can tell "the
//! repository changed" apart from "RepoLens changed".

use serde::{Deserialize, Serialize};

use crate::repository::CommitSha;

/// Everything that determines deterministic report output.
///
/// Deliberately *not* included: the archive tarball hash. GitHub does not
/// guarantee archive bytes are stable over time for a fixed commit, so keying
/// on it would break reproducibility rather than establish it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReproducibilityKey {
    /// The exact commit that was analyzed.
    pub commit_sha: CommitSha,
    /// Version of the analyzer that produced the report.
    pub analyzer_version: String,
    /// Version of the rule set that was evaluated.
    pub ruleset_version: String,
    /// Version of the policy deciding which paths are excluded from counting.
    pub exclusion_policy_version: String,
}
