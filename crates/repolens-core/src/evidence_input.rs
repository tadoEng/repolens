//! What a rule is allowed to look at, and what it may conclude from silence.
//!
//! Rules read paths and, since issue #5, the contents of a bounded set of files
//! the ingestion boundary chose. That second source changes what a negative
//! result *means*, and getting it wrong is the single easiest way to make this
//! product lie.
//!
//! # Four different silences
//!
//! When a content rule finds nothing, exactly one of these is true, and they are
//! not interchangeable:
//!
//! | Situation                                    | Honest outcome     |
//! | -------------------------------------------- | ------------------ |
//! | Contents were never collected for this run    | `UNABLE_TO_VERIFY` |
//! | The tree was truncated, so the file may exist | `UNABLE_TO_VERIFY` |
//! | The path exists but the bytes were not read   | `UNABLE_TO_VERIFY` |
//! | The file was read, and the thing is not in it | `MISSING`          |
//!
//! Only the last is knowledge. The other three are absence of evidence, and
//! collapsing any of them into `MISSING` reports a repository as lacking
//! something nobody actually looked for.
//!
//! [`crate::RuleInput::content_verdict`] is where that decision lives, so it is
//! made once rather than remembered by every rule author.

use crate::ContentDigest;

/// One retrieved file, as a rule sees it.
///
/// Text rather than bytes: every rule that reads content is looking for a
/// declaration, an import or a key, and handing each one raw bytes would mean
/// each one deciding how to decode. Undecodable input is dropped at the
/// boundary rather than reaching a rule as replacement characters that could
/// match a pattern by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContent {
    /// Repository-relative path.
    pub path: String,
    /// Decoded contents.
    pub text: String,
    /// SHA-256 of the retrieved bytes, so a finding can be traced to exactly
    /// what it was drawn from.
    pub digest: ContentDigest,
    /// `true` when the per-blob byte cap cut the file short.
    ///
    /// Load-bearing: a rule that finds nothing in a truncated file has not
    /// established absence, only that it did not find it in the part it saw.
    pub truncated: bool,
}

impl FileContent {
    /// The 1-based line number and text of the first line matching `predicate`.
    ///
    /// Rules cite a line rather than a whole file because evidence a reader
    /// cannot check in one glance is evidence they will not check at all.
    #[must_use]
    pub fn find_line(&self, predicate: impl Fn(&str) -> bool) -> Option<(u32, &str)> {
        self.text
            .lines()
            .enumerate()
            .find(|(_, line)| predicate(line))
            .map(|(index, line)| {
                // Saturating rather than `as`: a file with more than u32::MAX
                // lines cannot be cited honestly, and reporting the last
                // representable line is less wrong than wrapping to zero.
                (
                    u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
                    line,
                )
            })
    }
}

/// One piece of evidence supporting a rule's conclusion.
///
/// `excerpt` and `digest` are `None` for a path-only finding, and that is not a
/// gap to be filled in later: nothing was read, so quoting anything would be
/// fabrication. The wire contract carries the same distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvidence {
    /// Repository-relative path this evidence came from.
    pub path: String,
    /// The exact text the rule matched on, when it read one.
    pub excerpt: Option<String>,
    /// Digest of the bytes the excerpt came from.
    pub digest: Option<ContentDigest>,
    /// 1-based inclusive line range of the excerpt.
    pub line_range: Option<(u32, u32)>,
}

impl RuleEvidence {
    /// Evidence that a path exists, with nothing read from it.
    #[must_use]
    pub fn path_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            excerpt: None,
            digest: None,
            line_range: None,
        }
    }

    /// Evidence quoting one line of a file that was read.
    #[must_use]
    pub fn line(file: &FileContent, line_number: u32, text: &str) -> Self {
        Self {
            path: file.path.clone(),
            // Trimmed and bounded. An excerpt is for a human to recognise, and
            // a minified bundle on one line would otherwise put the whole file
            // in the report.
            excerpt: Some(bounded_excerpt(text)),
            digest: Some(file.digest.clone()),
            line_range: Some((line_number, line_number)),
        }
    }
}

/// Longest excerpt worth showing.
///
/// Generous for a source line, short enough that a single minified line cannot
/// dominate a report.
const MAX_EXCERPT_CHARS: usize = 200;

fn bounded_excerpt(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_EXCERPT_CHARS {
        return trimmed.to_owned();
    }
    // Truncated on a character boundary, and said so — a silently clipped
    // excerpt would look like the line genuinely ended there.
    let kept: String = trimmed.chars().take(MAX_EXCERPT_CHARS).collect();
    format!("{kept}…")
}

/// Why a rule could not reach a conclusion from content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unverifiable {
    /// No file contents were collected for this analysis at all.
    ContentsNotCollected,
    /// The tree listing was truncated, so the file may exist unseen.
    TreeTruncated,
    /// The path was seen in the tree but its bytes were not retrieved.
    NotRetrieved,
    /// The file was read, but the per-blob cap cut it short.
    FileTruncated,
}

impl Unverifiable {
    /// Stable, low-cardinality reason code for the report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContentsNotCollected => "CONTENTS_NOT_COLLECTED",
            Self::TreeTruncated => "TREE_TRUNCATED",
            Self::NotRetrieved => "FILE_NOT_RETRIEVED",
            Self::FileTruncated => "FILE_TRUNCATED",
        }
    }
}

/// What a content rule may conclude when it matched nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentVerdict {
    /// The file was read in full and the thing is genuinely not in it.
    ReadAndAbsent,
    /// Nothing can be concluded, for this reason.
    Unverifiable(Unverifiable),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> ContentDigest {
        ContentDigest::from_sha256([0x11; 32])
    }

    fn file(path: &str, text: &str) -> FileContent {
        FileContent {
            path: path.to_owned(),
            text: text.to_owned(),
            digest: digest(),
            truncated: false,
        }
    }

    #[test]
    fn evidence_cites_a_line_rather_than_a_file() {
        let file = file(
            "Cargo.toml",
            "[workspace]
members = [\"crates/*\"]
",
        );
        let (number, text) = file
            .find_line(|line| line.starts_with("members"))
            .expect("the line is there");

        assert_eq!(
            number, 2,
            "line numbers are 1-based, as an editor shows them"
        );

        let evidence = RuleEvidence::line(&file, number, text);
        assert_eq!(
            evidence.excerpt.as_deref(),
            Some("members = [\"crates/*\"]")
        );
        assert_eq!(evidence.line_range, Some((2, 2)));
        assert!(
            evidence.digest.is_some(),
            "a quoted excerpt must be traceable"
        );
    }

    #[test]
    fn a_path_only_finding_quotes_nothing() {
        // Nothing was read, so an excerpt or a digest here would be invented.
        let evidence = RuleEvidence::path_only("Cargo.toml");
        assert!(evidence.excerpt.is_none());
        assert!(evidence.digest.is_none());
        assert!(evidence.line_range.is_none());
    }

    #[test]
    fn an_enormous_line_is_bounded_and_says_so() {
        // A minified bundle is one line. Without this a single finding could
        // carry an entire file into the report.
        let long = "x".repeat(5_000);
        let file = file("app.js", &long);
        let (number, text) = file.find_line(|line| line.starts_with('x')).unwrap();
        let evidence = RuleEvidence::line(&file, number, text);

        let excerpt = evidence.excerpt.expect("an excerpt is present");
        assert!(excerpt.chars().count() <= MAX_EXCERPT_CHARS + 1);
        assert!(
            excerpt.ends_with('…'),
            "a clipped excerpt must not look like the line ended there"
        );
    }
}
