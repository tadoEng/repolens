//! The ingestion budgets.
//!
//! Every number here is a refusal to let the repository under analysis decide
//! how much work RepoLens does. An unbounded collector is not merely slow: on a
//! free-tier worker with a fixed memory ceiling and a job timeout, it is a
//! stranded lease and a report that never arrives.
//!
//! The budgets are split by what exceeding one *means*, and the two halves
//! behave differently on purpose:
//!
//! * **Incompleteness** — a tree larger than we will walk, a file larger than we
//!   will read. Partial evidence is still true evidence, so these mark the
//!   result as incomplete and carry on. `RepositoryTree::truncated` and
//!   [`SkipReason`](crate::SkipReason) exist to say so out loud.
//! * **Nonsense** — a repository too large to attempt, a half-written tarball, a
//!   JSON body that stopped mid-object. A partial result here would be
//!   *misleading* rather than merely incomplete, so these fail with
//!   [`GitHubSourceError::LimitExceeded`](crate::GitHubSourceError::LimitExceeded),
//!   which carries both the ceiling and the observed value so a report can state
//!   what happened instead of only that something did.

use std::time::Duration;

/// Largest recursive tree listing walked, in entries.
///
/// Matches GitHub's own ceiling for the recursive tree endpoint, so in practice
/// GitHub truncates first and this never binds. It is stated anyway because a
/// bound that exists only in someone else's service is not a bound: if GitHub
/// raised its limit, an unstated assumption here would quietly become an
/// unbounded allocation.
pub const MAX_TREE_ENTRIES: usize = 100_000;

/// Largest number of files whose contents are retrieved for one analysis.
///
/// The named files in issue #4 are roughly fifteen; the rest of the budget is
/// headroom for the bounded implementation-file rules in
/// [`select_paths`](crate::select_paths). Kept small deliberately — the point of
/// this collector is that it reads a chosen handful, not that it reads
/// everything slowly.
pub const MAX_SELECTED_FILES: usize = 64;

/// Largest single file retrieved, in bytes.
///
/// Sized for the largest *interesting* file rather than the largest possible
/// one: a workspace `Cargo.lock` or a `package-lock.json` reaches a few hundred
/// kilobytes, and anything past a megabyte is generated or vendored and carries
/// no architectural signal worth the transfer.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Largest total across all retrieved files, in bytes.
///
/// Deliberately far below `MAX_SELECTED_FILES * MAX_FILE_BYTES`. Per-file and
/// per-analysis ceilings answer different questions — "is this one file
/// pathological?" and "how much may one analysis cost?" — and multiplying the
/// first by the count would answer only the first, twice.
pub const MAX_TOTAL_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Hard ceiling on a downloaded archive, in compressed bytes.
///
/// [`download_archive`](crate::GitHubRepositorySource::download_archive) takes
/// its own budget from the caller, and this clamps it: a caller that asks for
/// more does not get more. Compressed rather than extracted, because it is the
/// only figure knowable while the bytes are still arriving — the extracted size
/// is the extractor's budget to enforce (issue #12).
pub const MAX_ARCHIVE_COMPRESSED_BYTES: u64 = 256 * 1024 * 1024;

/// Largest JSON response body read from the REST API, in bytes.
///
/// A recursive tree is the largest response by a wide margin and GitHub caps it
/// near seven megabytes, so this is roughly double the worst legitimate case.
/// It exists because `Content-Length` is a claim, not a promise, and a body that
/// keeps arriving would otherwise be allocated in full before anything looked at
/// it.
pub const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

/// Largest repository accepted, in kilobytes, as GitHub reports it.
///
/// Checked at
/// [`resolve_repository`](crate::GitHubRepositorySource::resolve_repository) so
/// that "too big to analyze" costs one request instead of a download that is
/// killed halfway through. GitHub's figure covers the whole object database
/// rather than one commit's worth of files, so it overstates what an archive
/// would weigh — which is the right direction for a guard to err in.
pub const MAX_REPOSITORY_KILOBYTES: u64 = 512 * 1024;

/// Redirect hops followed before giving up.
///
/// Three is enough for the archive endpoint's single documented hop plus a
/// repository rename, and small enough that a redirect loop costs four requests
/// rather than a request budget. See
/// [`GitHubRestClient`](crate::GitHubRestClient) for why the hops are followed
/// by hand.
pub const MAX_REDIRECT_HOPS: u8 = 3;

/// Bytes examined when deciding whether a retrieved file is binary.
///
/// The rule and the window are Git's own: a `NUL` inside the first eight
/// thousand bytes. Matching Git means RepoLens and `git diff` disagree about no
/// file, which matters because the report cites paths a reader will open in a
/// checkout.
pub const BINARY_SNIFF_BYTES: usize = 8000;

/// Wall-clock budget for one HTTP request, redirect hops excluded.
///
/// Shorter than the worker's own job timeout by a wide margin, so a stalled
/// GitHub connection surfaces as a typed transport failure that the worker can
/// record and retry, rather than as an execution killed mid-lease.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::{
        MAX_ARCHIVE_COMPRESSED_BYTES, MAX_FILE_BYTES, MAX_SELECTED_FILES, MAX_TOTAL_FILE_BYTES,
    };

    #[test]
    fn the_per_analysis_budget_binds_before_the_per_file_one() {
        // If the total were at least `MAX_SELECTED_FILES * MAX_FILE_BYTES`, it
        // could never be reached and the whole-analysis ceiling would be
        // decoration. The two limits are meant to answer different questions,
        // which requires that both can actually bind.
        let unconstrained = MAX_FILE_BYTES * MAX_SELECTED_FILES as u64;
        assert!(
            MAX_TOTAL_FILE_BYTES < unconstrained,
            "a total budget of {MAX_TOTAL_FILE_BYTES} could never bind below {unconstrained}"
        );
    }

    #[test]
    fn one_file_cannot_exhaust_the_analysis_budget() {
        // The mirror of the above: if a single permitted file could spend the
        // whole budget, the first large file would starve every later one and
        // selection order would silently become selection policy.
        const { assert!(MAX_FILE_BYTES < MAX_TOTAL_FILE_BYTES) }
    }

    #[test]
    fn the_archive_budget_dwarfs_the_blob_budget() {
        // They are not alternatives. Blobs are canonical evidence read in full;
        // the archive is ephemeral transport for line counting. Sizing them
        // alike would mean one of the two is wrong.
        const { assert!(MAX_ARCHIVE_COMPRESSED_BYTES > MAX_TOTAL_FILE_BYTES) }
    }
}
