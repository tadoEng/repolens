//! The analysis lifecycle, as the browser sees it.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::error::ApiError;

/// Where an analysis has reached.
///
/// Ordered as the work actually proceeds, so a UI can render a checklist by
/// position without a second table mapping states to steps.
///
/// Infrastructure state is deliberately absent — there is no `TRIGGERING` or
/// `WAITING_FOR_WORKER`. Whether a Cloud Run Job execution was accepted is a
/// property of the *execution*, not of the analysis, and mixing them would mean
/// every consumer of this enum had to learn how the work is scheduled. See
/// [`ExecutionMetadata`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AnalysisState {
    /// Accepted and durable, not yet claimed.
    Queued,
    /// Resolving the reference to an exact commit.
    Resolving,
    /// Fetching bounded repository evidence.
    Collecting,
    /// Running deterministic rules over the evidence snapshot.
    Analyzing,
    /// Assembling the report from findings.
    BuildingReport,
    /// Finished. A report exists.
    Completed,
    /// Failed in a way another attempt may survive.
    FailedRetriable,
    /// Failed deterministically. The same commit and ruleset will fail again.
    FailedPermanent,
}

impl AnalysisState {
    /// Whether the analysis has stopped moving.
    ///
    /// The frontend stops polling on this rather than on a hardcoded list, so a
    /// state added later cannot leave a browser polling a finished analysis
    /// forever.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::FailedRetriable | Self::FailedPermanent
        )
    }

    /// Position in the pipeline, for rendering progress.
    ///
    /// `None` for terminal states: "step 6 of 6" and "finished" are different
    /// claims, and a completed analysis is not at a step.
    #[must_use]
    pub const fn step(self) -> Option<u8> {
        match self {
            Self::Queued => Some(1),
            Self::Resolving => Some(2),
            Self::Collecting => Some(3),
            Self::Analyzing => Some(4),
            Self::BuildingReport => Some(5),
            Self::Completed | Self::FailedRetriable | Self::FailedPermanent => None,
        }
    }
}

/// Whether the scheduler accepted the work.
///
/// Separate from [`AnalysisState`] because they fail independently: an analysis
/// can be `QUEUED` with the trigger *succeeded* (normal, waiting for a worker)
/// or `QUEUED` with the trigger *failed* (stuck, and nothing will ever pick it
/// up). Those look identical without this, and the second one is the outage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TriggerStatus {
    /// Not yet attempted.
    Pending,
    /// The scheduler accepted it.
    Succeeded,
    /// The scheduler rejected it. Nothing will run without intervention.
    Failed,
}

/// Scheduling facts about an analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExecutionMetadata {
    /// Whether the work was successfully handed to a runner.
    pub trigger_status: TriggerStatus,
    /// Runner-assigned execution identifier, for correlating logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    /// When the trigger was attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "time::serde::rfc3339::option")]
    pub triggered_at: Option<OffsetDateTime>,
}

/// Whether the caller may retry, decided by the server.
///
/// Never inferred from the state name. `FAILED_RETRIABLE` describes the *kind*
/// of failure; whether a retry is permitted also depends on how many attempts
/// have already been spent and whether the work is still claimable — facts only
/// the server holds. A frontend that derived this would offer a button that
/// does nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RetryPolicy {
    /// Whether a retry request would be accepted right now.
    pub allowed: bool,
    /// Why not, when `allowed` is false. Displayed verbatim, so it explains
    /// rather than merely denies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Repository identity, available from the moment an analysis is created.
///
/// Present before `commit_sha` exists, which is what lets the header render
/// `owner/name` immediately instead of a blank space that looks broken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RepositoryIdentity {
    /// User or organization.
    pub owner: String,
    /// Repository name, without the owner prefix.
    pub name: String,
}

/// One analysis run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Analysis {
    /// Stable identifier. `UUIDv7`: time-ordered for index locality, and with 74
    /// random bits it remains unguessable, which is what allows anonymous
    /// progress viewing by URL.
    pub id: Uuid,
    /// Owner and name, known from creation.
    pub repository: RepositoryIdentity,
    /// The exact commit, once resolved.
    ///
    /// **Null during `QUEUED` and `RESOLVING`** — there genuinely is no commit
    /// yet. Required-but-nullable rather than optional, so a consumer cannot
    /// forget the case: the field is always present, its value is not.
    #[schema(required)]
    pub commit_sha: Option<String>,
    /// Where the analysis has reached.
    pub state: AnalysisState,
    /// Scheduling facts, kept out of `state`.
    pub execution: ExecutionMetadata,
    /// Server's decision on whether a retry is currently permitted.
    pub retry: RetryPolicy,
    /// Why it failed. Present only in a failed state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    /// When the analysis was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it last changed state. Distinct from `created_at` so a UI can show
    /// "stuck for 20 minutes" rather than only "started 20 minutes ago".
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// How long the client should wait before polling again.
    ///
    /// Server-supplied so the interval can widen as an analysis ages, and so a
    /// hardcoded frontend value cannot multiply cold starts and cost. Absent in
    /// terminal states — there is nothing left to poll for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u32>,
    /// Whether `GET /reports/{id}` will return a report.
    ///
    /// Explicit rather than `state == COMPLETED`, because report availability
    /// and analysis completion are separate facts once reports are retained,
    /// pruned, or regenerated under a newer ruleset.
    pub report_available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_serialize_as_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&AnalysisState::BuildingReport).unwrap(),
            "\"BUILDING_REPORT\""
        );
    }

    #[test]
    fn every_terminal_state_has_no_step_and_every_step_state_is_not_terminal() {
        // A state that is both would make a progress checklist render a step for
        // a finished analysis, or drop one for a running analysis.
        for state in [
            AnalysisState::Queued,
            AnalysisState::Resolving,
            AnalysisState::Collecting,
            AnalysisState::Analyzing,
            AnalysisState::BuildingReport,
            AnalysisState::Completed,
            AnalysisState::FailedRetriable,
            AnalysisState::FailedPermanent,
        ] {
            assert_eq!(
                state.is_terminal(),
                state.step().is_none(),
                "{state:?} disagrees about being terminal"
            );
        }
    }

    #[test]
    fn steps_are_contiguous_and_ordered() {
        let steps: Vec<u8> = [
            AnalysisState::Queued,
            AnalysisState::Resolving,
            AnalysisState::Collecting,
            AnalysisState::Analyzing,
            AnalysisState::BuildingReport,
        ]
        .into_iter()
        .filter_map(AnalysisState::step)
        .collect();

        assert_eq!(steps, vec![1, 2, 3, 4, 5], "progress must not skip a step");
    }

    #[test]
    fn a_null_commit_sha_is_serialized_rather_than_omitted() {
        // The field is required-but-nullable. Omitting it would let a consumer
        // treat "not resolved yet" as "field I can ignore".
        let analysis = Analysis {
            id: Uuid::nil(),
            repository: RepositoryIdentity {
                owner: "rust-lang".into(),
                name: "crates.io".into(),
            },
            commit_sha: None,
            state: AnalysisState::Queued,
            execution: ExecutionMetadata {
                trigger_status: TriggerStatus::Succeeded,
                execution_id: None,
                triggered_at: None,
            },
            retry: RetryPolicy {
                allowed: false,
                reason: Some("the analysis has not failed".into()),
            },
            error: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            poll_after_ms: Some(2000),
            report_available: false,
        };

        let json = serde_json::to_value(&analysis).unwrap();
        assert!(
            json.get("commit_sha").is_some(),
            "commit_sha must be present"
        );
        assert!(json["commit_sha"].is_null(), "and null while unresolved");
    }
}
