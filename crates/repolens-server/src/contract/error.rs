//! The public error envelope.
//!
//! Every failure the frontend must render differently gets its own code. A bare
//! `{ message }` is unusable: the UI cannot tell "this repository does not
//! exist" (offer a corrected URL) from "we are rate limited" (offer a time to
//! come back) from "the analyzer broke" (offer nothing, and say so), and it
//! must not guess by matching on prose that a backend refactor will reword.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Why a request or an analysis failed.
///
/// `SCREAMING_SNAKE_CASE` per the settled convention (#14). The set is closed on
/// purpose — see [`super::UNKNOWN_VARIANT_POLICY`] for what the frontend does
/// when a future backend adds one it has never seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// The submitted string is not a GitHub repository URL. User-correctable.
    InvalidRepositoryUrl,
    /// No such repository, or it is private. Deliberately not distinguished:
    /// telling an anonymous caller that a private repository *exists* leaks its
    /// existence, which is why GitHub itself answers 404 for both.
    RepositoryNotFound,
    /// The repository exists and is public but cannot be read — most often a
    /// GitHub outage or a permissions change mid-analysis.
    RepositoryInaccessible,
    /// Archived repositories are readable, but analyzing one implies ongoing
    /// engineering that is not happening. Surfaced so the UI can say why.
    RepositoryArchived,
    /// Beyond the configured ingestion bounds. Carries no blame: the limits are
    /// ours, and the message names the one that was exceeded.
    RepositoryTooLarge,
    /// GitHub rate limit exhausted. The only code that populates
    /// `retry_after_seconds`, because it is the only one where the wait is
    /// knowable rather than guessed.
    RateLimited,
    /// The worker failed in a way that may succeed on another attempt.
    WorkerFailedRetriable,
    /// The analyzer failed deterministically. Retrying the same commit with the
    /// same ruleset will fail identically, so the UI must not offer retry.
    AnalyzerFailedPermanent,
}

impl ErrorCode {
    /// Whether another attempt at the *same* input could plausibly succeed.
    ///
    /// Advisory only. The authority is [`super::analysis::RetryPolicy`] on the
    /// analysis itself, because only the server knows how many attempts have
    /// already been spent. This exists so a rule and its rendering cannot drift
    /// silently apart, not so the frontend can compute retryability itself.
    #[must_use]
    pub const fn is_retriable(self) -> bool {
        matches!(
            self,
            Self::RepositoryInaccessible | Self::RateLimited | Self::WorkerFailedRetriable
        )
    }
}

/// A failure, as the browser receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    /// Stable machine code. Switch on this, never on `message`.
    pub code: ErrorCode,
    /// Human-readable explanation. Safe to display, and deliberately free of
    /// internal identifiers, hostnames, and credentials — this string crosses
    /// into a browser.
    pub message: String,
    /// How long to wait before retrying, when that is actually knowable.
    ///
    /// Absent rather than zero when unknown: a UI that renders "retry in 0s"
    /// from a missing value is worse than one that renders no countdown at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u32>,
}

impl ApiError {
    /// Builds an error with no retry hint.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_seconds: None,
        }
    }

    /// Builds an error carrying a concrete wait.
    pub fn retry_after(code: ErrorCode, message: impl Into<String>, seconds: u32) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_seconds: Some(seconds),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_serialize_as_screaming_snake_case() {
        let json = serde_json::to_string(&ErrorCode::InvalidRepositoryUrl).unwrap();
        assert_eq!(json, "\"INVALID_REPOSITORY_URL\"");
    }

    #[test]
    fn an_absent_retry_hint_is_omitted_rather_than_null() {
        let json =
            serde_json::to_string(&ApiError::new(ErrorCode::RepositoryNotFound, "nope")).unwrap();
        assert!(
            !json.contains("retry_after_seconds"),
            "an unknown wait must be absent, not rendered as a countdown"
        );
    }

    #[test]
    fn permanent_failures_are_never_marked_retriable() {
        // The UI offers a retry control from this. Getting it wrong either hides
        // a recoverable failure or invites an infinite loop on a deterministic
        // one.
        assert!(!ErrorCode::AnalyzerFailedPermanent.is_retriable());
        assert!(!ErrorCode::InvalidRepositoryUrl.is_retriable());
        assert!(!ErrorCode::RepositoryArchived.is_retriable());
        assert!(ErrorCode::RateLimited.is_retriable());
        assert!(ErrorCode::WorkerFailedRetriable.is_retriable());
    }
}
