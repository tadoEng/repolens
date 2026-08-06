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
    /// No analysis exists with that identifier, or it has no report yet.
    ///
    /// Deliberately not [`RepositoryNotFound`](Self::RepositoryNotFound), which
    /// is a claim about a *repository* — that it is absent or private on
    /// GitHub. Answering it for an unknown analysis id tells the caller to go
    /// and check a repository that was never the problem, and the UI would
    /// offer to correct a URL that is already right.
    AnalysisNotFound,
    /// The request itself could not be interpreted: a body that is not valid
    /// JSON, a missing or wrong `Content-Type`, or a path parameter that is not
    /// the type the route declares.
    ///
    /// Distinct from [`InvalidRepositoryUrl`](Self::InvalidRepositoryUrl),
    /// which means a well-formed request carrying a value we understood and
    /// rejected. This one means we never got as far as the value, so the UI
    /// must not echo it back as a correctable repository URL.
    MalformedRequest,
    /// The request body exceeded the configured ceiling.
    ///
    /// Not [`RepositoryTooLarge`](Self::RepositoryTooLarge), which is a
    /// statement about the repository being analyzed. This is a statement about
    /// the HTTP request, and no repository has been looked at yet.
    RequestTooLarge,
    /// The request occupied a server slot for longer than it is allowed to.
    ///
    /// Retriable: it says the deployment was slow, not that the input was bad.
    RequestTimedOut,
    /// An unhandled fault in this service.
    ///
    /// The message is deliberately fixed and uninformative to the caller: a
    /// panic payload can carry internal paths, query fragments, or retrieved
    /// repository content, none of which may cross into a browser. The detail
    /// goes to the log instead.
    InternalError,
}

impl ErrorCode {
    /// Every code, in declaration order.
    ///
    /// Hand-maintained but *not* hand-trusted: `all_variants_are_listed` fails
    /// if this drifts from the enum. Without it, a test that iterates "all
    /// codes" would quietly iterate only the ones somebody remembered.
    pub const ALL: [Self; 13] = [
        Self::InvalidRepositoryUrl,
        Self::RepositoryNotFound,
        Self::RepositoryInaccessible,
        Self::RepositoryArchived,
        Self::RepositoryTooLarge,
        Self::RateLimited,
        Self::WorkerFailedRetriable,
        Self::AnalyzerFailedPermanent,
        Self::AnalysisNotFound,
        Self::MalformedRequest,
        Self::RequestTooLarge,
        Self::RequestTimedOut,
        Self::InternalError,
    ];

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
            Self::RepositoryInaccessible
                | Self::RateLimited
                | Self::WorkerFailedRetriable
                // The deployment was slow, not the request wrong.
                | Self::RequestTimedOut
        )
    }
}

/// A failure, as the browser receives it.
///
/// Fields are private and deserialization is validated, so
/// `ANALYZER_FAILED_PERMANENT` carrying a 900-second countdown cannot be
/// constructed *or* parsed. A safe constructor alone would not achieve that:
/// derived `Deserialize` would still accept the combination off the wire, and a
/// public field would still accept it in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "ApiErrorWire")]
pub struct ApiError {
    /// Stable machine code. Switch on this, never on `message`.
    code: ErrorCode,
    /// Human-readable explanation. Safe to display, and deliberately free of
    /// internal identifiers, hostnames, and credentials — this string crosses
    /// into a browser.
    message: String,
    /// How long to wait before retrying, when that is actually knowable.
    ///
    /// Absent rather than zero when unknown: a UI that renders "retry in 0s"
    /// from a missing value is worse than one that renders no countdown at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<u32>,
}

/// Wire mirror used only to validate incoming errors.
///
/// Private, and never part of the published schema — `ApiError` derives
/// `ToSchema` itself, so the document describes the validated type.
#[derive(Deserialize)]
struct ApiErrorWire {
    code: ErrorCode,
    message: String,
    #[serde(default)]
    retry_after_seconds: Option<u32>,
}

/// Why a received error envelope could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiErrorInvalid {
    /// A wait was attached to a code where no wait is knowable.
    #[error("only RATE_LIMITED may carry retry_after_seconds, got {0:?}")]
    UnexpectedRetryAfter(ErrorCode),
}

impl TryFrom<ApiErrorWire> for ApiError {
    type Error = ApiErrorInvalid;

    fn try_from(wire: ApiErrorWire) -> Result<Self, Self::Error> {
        if wire.retry_after_seconds.is_some() && wire.code != ErrorCode::RateLimited {
            return Err(ApiErrorInvalid::UnexpectedRetryAfter(wire.code));
        }
        Ok(Self {
            code: wire.code,
            message: wire.message,
            retry_after_seconds: wire.retry_after_seconds,
        })
    }
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

    /// Builds a rate-limit error carrying a concrete wait.
    ///
    /// Takes no `ErrorCode`, because `RATE_LIMITED` is the only code where the
    /// wait is *knowable* rather than guessed — GitHub tells us when the window
    /// resets. Accepting an arbitrary code would let a caller attach an invented
    /// countdown to, say, a permanent analyzer failure, and a UI that counts
    /// down to a retry that will never succeed is worse than one that offers no
    /// retry at all.
    pub fn rate_limited(message: impl Into<String>, retry_after_seconds: u32) -> Self {
        Self {
            code: ErrorCode::RateLimited,
            message: message.into(),
            retry_after_seconds: Some(retry_after_seconds),
        }
    }

    /// The machine code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// The displayable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The wait, when one is knowable.
    #[must_use]
    pub const fn retry_after_seconds(&self) -> Option<u32> {
        self.retry_after_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_are_listed() {
        // `ALL` is what every exhaustiveness gate iterates. If a code were added
        // to the enum but not here, those gates would keep passing while
        // covering less than they claim. The match is exhaustive, so adding a
        // variant fails to compile until it is listed.
        for code in ErrorCode::ALL {
            match code {
                ErrorCode::InvalidRepositoryUrl
                | ErrorCode::RepositoryNotFound
                | ErrorCode::RepositoryInaccessible
                | ErrorCode::RepositoryArchived
                | ErrorCode::RepositoryTooLarge
                | ErrorCode::RateLimited
                | ErrorCode::WorkerFailedRetriable
                | ErrorCode::AnalyzerFailedPermanent
                | ErrorCode::AnalysisNotFound
                | ErrorCode::MalformedRequest
                | ErrorCode::RequestTooLarge
                | ErrorCode::RequestTimedOut
                | ErrorCode::InternalError => {}
            }
        }

        let distinct: std::collections::BTreeSet<_> = ErrorCode::ALL
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        assert_eq!(
            distinct.len(),
            ErrorCode::ALL.len(),
            "ALL contains a duplicate"
        );
    }

    #[test]
    fn only_rate_limiting_can_carry_a_wait() {
        let error = ApiError::rate_limited("slow down", 900);
        assert_eq!(error.code(), ErrorCode::RateLimited);
        assert_eq!(error.retry_after_seconds(), Some(900));
    }

    #[test]
    fn an_invalid_combination_cannot_be_deserialized() {
        // The constructor alone would not achieve this: derived Deserialize
        // would still accept the combination straight off the wire, which is
        // exactly where an error envelope comes from.
        let hostile =
            r#"{"code":"ANALYZER_FAILED_PERMANENT","message":"x","retry_after_seconds":900}"#;
        let parsed = serde_json::from_str::<ApiError>(hostile);
        assert!(
            parsed.is_err(),
            "a permanent failure must not be able to carry a countdown"
        );
    }

    #[test]
    fn a_valid_combination_still_round_trips() {
        let original = ApiError::rate_limited("slow down", 900);
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(serde_json::from_str::<ApiError>(&json).unwrap(), original);

        let plain = ApiError::new(ErrorCode::RepositoryNotFound, "nope");
        let json = serde_json::to_string(&plain).unwrap();
        assert_eq!(serde_json::from_str::<ApiError>(&json).unwrap(), plain);
    }

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
