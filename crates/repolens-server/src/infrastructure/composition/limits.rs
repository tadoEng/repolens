//! The seven controls, and what each of them is protecting.
//!
//! Values are measured; behaviour is fixed. Issue #12 states that division
//! deliberately — a number here can be retuned once #9 measures the deployed
//! job, but "above the limit yields `UNABLE_TO_VERIFY` carrying the exact limit
//! and the observed value" is not a tuning decision and does not change.
//!
//! Every ceiling is stated in bytes or counts rather than as a ratio. A ratio
//! (`decompressed / compressed`) reads like a bomb detector and is not one: an
//! archive can sit under any ratio you pick and still be larger than the
//! machine, because the ratio says nothing about absolute size.

/// Compressed bytes accepted from GitHub for one archive.
///
/// Enforced at the ingestion boundary, which is where the bytes arrive —
/// `repolens_github::limits::MAX_ARCHIVE_COMPRESSED_BYTES` is the authority and
/// this is the extraction side's view of the same budget.
pub const MAX_COMPRESSED_BYTES: u64 = repolens_github::limits::MAX_ARCHIVE_COMPRESSED_BYTES;

/// Decompressed bytes accepted from one archive, across every entry.
///
/// The second of the two independent layers issue #12 requires. A compressed
/// cap alone cannot bound this: 4 MiB of zeroes expands to gigabytes, and the
/// download looks entirely ordinary while it happens.
pub const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

/// Entries examined in one archive.
///
/// Bounds the *iteration*, not the extraction. A tarball of ten million empty
/// files breaches no byte ceiling at all — every entry is zero-length — and
/// still costs an inode apiece and an unbounded walk.
pub const MAX_ENTRIES: usize = 200_000;

/// Bytes accepted from any single entry.
///
/// Separate from the archive total so one pathological file cannot consume the
/// whole budget and leave the rest of the repository uncounted. A source file
/// this large is not source.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Wall-clock ceiling for download plus extraction plus counting.
///
/// The control that catches what the byte ceilings cannot: an archive that
/// decompresses slowly enough to hold a worker without ever exceeding a size.
pub const MAX_DURATION_SECONDS: u64 = 300;

/// Bytes the extraction volume may hold.
///
/// Deliberately below [`MAX_DECOMPRESSED_BYTES`], so the *stream* ceiling and
/// the *storage* ceiling are different numbers with different names. Issue #12
/// is explicit about why this one exists: on a memory-backed filesystem an
/// unbounded write is an OOM kill, which leaves a stale lease and no diagnosis,
/// while a bounded volume makes the write fail — and a failed write is
/// catchable and reportable.
pub const MAX_EXTRACTION_STORAGE_BYTES: u64 = 384 * 1024 * 1024;

/// Names carried in [`CompositionLimitBreach::limit_name`].
///
/// Stable, low-cardinality strings rather than a `Display` of the limit: a
/// report groups by these, and a name that embedded the observed value would
/// put every breach in a bucket of one.
///
/// [`CompositionLimitBreach::limit_name`]: repolens_core::CompositionLimitBreach::limit_name
pub mod names {
    /// Compressed archive stream exceeded its ceiling.
    pub const COMPRESSED_STREAM: &str = "ARCHIVE_COMPRESSED_LIMIT";
    /// Decompressed bytes exceeded their ceiling across the archive.
    pub const DECOMPRESSED_STREAM: &str = "ARCHIVE_DECOMPRESSED_LIMIT";
    /// The archive held more entries than the walk accepts.
    pub const ENTRY_COUNT: &str = "ARCHIVE_ENTRY_COUNT_LIMIT";
    /// One entry was larger than any single file may be.
    pub const FILE_BYTES: &str = "ARCHIVE_FILE_SIZE_LIMIT";
    /// Download, extraction and counting together ran out of time.
    pub const DURATION: &str = "ARCHIVE_DURATION_LIMIT";
    /// The bounded extraction volume filled.
    ///
    /// The name issue #12 specifies by hand, because it is the one a reader is
    /// most likely to meet and the one that distinguishes graceful degradation
    /// from an OOM-killed job.
    pub const EXTRACTION_STORAGE: &str = "EXTRACTION_STORAGE_LIMIT";
}
