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
///
/// Counted on the decoded stream *before* the tar parser, so tar headers and
/// block padding are inside the figure. That is what makes it the right control
/// for a decompression bomb — the cost being bounded is what the machine has to
/// pull through, not what the repository contains — and it is also why crossing
/// it leaves a report ineligible for comparison: the figure is a property of
/// GitHub's archive representation, which is guaranteed stable nowhere.
pub const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

/// Entries examined in one archive.
///
/// Bounds the *iteration*, not the extraction. A tarball of ten million empty
/// files breaches no byte ceiling at all — every entry is zero-length — and
/// still costs an inode apiece and an unbounded walk.
///
/// Counted per tar entry and *before* admission, so directories, links, every
/// entry the extractor goes on to refuse, and the top-level prefix directory
/// GitHub wraps an archive in are all inside the figure. It is therefore a
/// control on the tarball rather than a count of the repository's paths, which
/// is why crossing it leaves a report ineligible for comparison.
///
/// Contrast [`MAX_FILE_BYTES`], which is checked *after* admission: only a
/// regular file the analysis would have counted can breach that one, which is
/// what keeps it a fact about the repository. Deriving an entry ceiling from an
/// admitted-path count would move this one across the same line.
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

#[cfg(test)]
mod tests {
    use crate::contract::report::{ComparisonEligibility, Limitation, Report, minimal_report};

    /// Every limit name, paired with whether crossing it is decided by the
    /// repository or by something else.
    ///
    /// The pairing is the point. A new ceiling added to [`names`] without a row
    /// here fails the test below, so "is this outcome reproducible?" has to be
    /// answered when the ceiling is introduced rather than inferred later by
    /// whoever is debugging two reports that disagree.
    ///
    /// The question a row answers is not "is this ceiling fixed?" — every
    /// ceiling here is fixed — but "is the quantity it measures a property of
    /// the repository?". For the archive ceilings that reduces to *where in
    /// `extract_to` the measurement is taken*: everything counted before
    /// `admit` is a measurement of the tarball, and only what is checked after
    /// it is a measurement of the repository.
    const CLASSIFIED: [(&str, bool); 6] = [
        // The one archive ceiling checked after admission, so nothing but a
        // regular file the analysis would have counted can breach it — and a
        // regular file's declared size is its own byte count.
        (super::names::FILE_BYTES, true),
        // Measured before admission, against GitHub's archive representation,
        // for which nothing is guaranteed stable at a fixed commit. The
        // decompressed figure is counted between the gzip decoder and the tar
        // parser, so it carries tar headers and block padding; the entry count
        // is incremented per tar entry, so it carries directories, links,
        // refused entries and the prefix directory. An archive sitting near any
        // of these can legitimately fall either side of it.
        (super::names::COMPRESSED_STREAM, false),
        (super::names::DECOMPRESSED_STREAM, false),
        (super::names::ENTRY_COUNT, false),
        // Properties of the host.
        (super::names::DURATION, false),
        (super::names::EXTRACTION_STORAGE, false),
    ];

    fn report_limited_by(code: &str) -> Report {
        let mut report = minimal_report();
        report.limitations = vec![Limitation {
            code: code.to_owned(),
            explanation: "fixture".to_owned(),
        }];
        report
    }

    #[test]
    fn every_limit_name_is_classified_as_reproducible_or_not() {
        let named = [
            super::names::COMPRESSED_STREAM,
            super::names::DECOMPRESSED_STREAM,
            super::names::ENTRY_COUNT,
            super::names::FILE_BYTES,
            super::names::DURATION,
            super::names::EXTRACTION_STORAGE,
        ];

        for name in named {
            assert!(
                CLASSIFIED.iter().any(|(code, _)| *code == name),
                "{name} has no reproducibility classification; decide whether crossing it is a \
                 property of the repository or of something else"
            );
        }
        assert_eq!(CLASSIFIED.len(), named.len(), "CLASSIFIED has a stale row");
    }

    #[test]
    fn the_contract_agrees_with_this_classification() {
        // The two live apart on purpose — limitation codes are contract
        // vocabulary, ceilings are infrastructure — so this is what stops them
        // drifting into disagreement about which outcomes are reproducible.
        for (code, reproducible) in CLASSIFIED {
            let eligibility = report_limited_by(code).comparison_eligibility();
            assert_eq!(
                eligibility.is_eligible(),
                reproducible,
                "{code}: infrastructure and contract disagree about whether this outcome may be \
                 compared between runs"
            );
            if !reproducible {
                assert!(
                    matches!(
                        eligibility,
                        ComparisonEligibility::Ineligible { code: ref named, .. } if named == code
                    ),
                    "an ineligible report must name the limitation that made it so, got {eligibility:?}"
                );
            }
        }
    }
}
