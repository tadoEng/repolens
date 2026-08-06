//! Persistence for analyses and reports.
//!
//! Plain functions over a pool, not a repository trait. There is one
//! implementation and one caller; an abstraction between them would exist only
//! to be mocked, and the tests that matter here run against a real PostgreSQL
//! anyway.
//!
//! Queries are written with runtime `sqlx::query`, not the `query!` macro.
//! Compile-time verification needs either a live database at build time or a
//! committed `.sqlx` cache that every query change has to regenerate — friction
//! this query set has not yet earned, and a stale cache fails CI in a way that
//! reads as a broken build rather than a missed step. Recorded here rather than
//! left implicit: it is exactly the kind of stack trade-off #10 exists to
//! report on.

use repolens_core::RepositoryCoordinate;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::contract::analysis::{
    Analysis, AnalysisState, ExecutionMetadata, RepositoryIdentity, RetryPolicy, TriggerStatus,
};
use crate::contract::error::{ApiError, ErrorCode};
use crate::contract::report::Report;

/// How long a client should wait before polling a running analysis again.
///
/// One value for now. It belongs on the server so the interval can widen as an
/// analysis ages without shipping a new frontend, which is the reason the field
/// exists on the wire at all.
const POLL_INTERVAL_MS: u32 = 1_500;

/// Everything that can go wrong talking to the database.
///
/// Deliberately not `anyhow`: a handler needs to distinguish "no such analysis"
/// from "the database is unreachable", and a string cannot be matched on.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No analysis with that id.
    #[error("analysis not found")]
    NotFound,
    /// The query failed. The underlying error is not rendered — it can carry
    /// connection parameters, and this type crosses into a handler that logs.
    #[error("database query failed")]
    Query(#[source] sqlx::Error),
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Query(other),
        }
    }
}

/// Creates a queued analysis.
///
/// `commit_sha` is deliberately absent: nothing has been resolved yet, and
/// writing a placeholder would make "not resolved" indistinguishable from
/// "resolved to something".
///
/// # Errors
///
/// Returns [`StoreError::Query`] when the insert fails.
pub async fn create_analysis(
    pool: &PgPool,
    coordinate: &RepositoryCoordinate,
) -> Result<Analysis, StoreError> {
    // UUIDv7: time-ordered for index locality, 74 random bits so an id in a URL
    // stays unguessable — which is what allows anonymous progress viewing.
    let id = Uuid::now_v7();

    let row = sqlx::query(
        "INSERT INTO analyses (id, owner, name, state, retry_allowed, retry_reason)
         VALUES ($1, $2, $3, 'QUEUED', false, $4)
         RETURNING created_at, updated_at",
    )
    .bind(id)
    .bind(&coordinate.owner)
    .bind(&coordinate.name)
    .bind("the analysis has not failed")
    .fetch_one(pool)
    .await?;

    Ok(Analysis {
        id,
        repository: RepositoryIdentity {
            owner: coordinate.owner.clone(),
            name: coordinate.name.clone(),
        },
        commit_sha: None,
        state: AnalysisState::Queued,
        execution: ExecutionMetadata {
            // Inline execution: by the time this row exists the work is already
            // spawned, so the trigger has succeeded by construction. When #7
            // replaces this with a Cloud Run Job, PENDING and FAILED become
            // reachable and the field starts earning its place.
            trigger_status: TriggerStatus::Succeeded,
            execution_id: None,
            triggered_at: Some(row.get("created_at")),
        },
        retry: RetryPolicy {
            allowed: false,
            reason: Some("the analysis has not failed".to_owned()),
        },
        error: None,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        poll_after_ms: Some(POLL_INTERVAL_MS),
        report_available: false,
    })
}

/// Moves an analysis to a non-terminal state.
///
/// # Errors
///
/// Returns [`StoreError::Query`] when the update fails.
pub async fn advance(
    pool: &PgPool,
    id: Uuid,
    state: AnalysisState,
    commit_sha: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE analyses
            SET state = $2,
                commit_sha = COALESCE($3, commit_sha),
                updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(state_name(state))
    .bind(commit_sha)
    .execute(pool)
    .await?;
    Ok(())
}

/// Adopts the canonical coordinate GitHub resolved a submission to.
///
/// A renamed or transferred repository still answers under its old address, so
/// the row created from the submission can name something that no longer
/// identifies it. Overwriting with GitHub's answer keeps the progress record
/// pointing at the repository that was actually read.
///
/// # Errors
///
/// Returns [`StoreError::Query`] when the update fails.
pub async fn adopt_coordinate(
    pool: &PgPool,
    id: Uuid,
    coordinate: &RepositoryCoordinate,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE analyses
            SET owner = $2, name = $3, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&coordinate.owner)
    .bind(&coordinate.name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Records a failure, with the retry decision the server has made.
///
/// The decision is persisted rather than derived at read time, so two readers
/// cannot disagree about whether a retry is currently permitted.
///
/// # Errors
///
/// Returns [`StoreError::Query`] when the update fails.
pub async fn fail(
    pool: &PgPool,
    id: Uuid,
    error: &ApiError,
    canonical: Option<&RepositoryCoordinate>,
) -> Result<(), StoreError> {
    let retriable = error.code().is_retriable();
    let state = if retriable {
        AnalysisState::FailedRetriable
    } else {
        AnalysisState::FailedPermanent
    };

    // Owner and name are written here, with the terminal state, rather than
    // relying on the best-effort update made when the redirect was discovered.
    // `COALESCE` leaves them untouched when resolution never got far enough to
    // learn the canonical coordinate — a submission that failed before GitHub
    // answered has nothing better than what the submitter typed.
    sqlx::query(
        "UPDATE analyses
            SET state = $2,
                error_code = $3,
                error_message = $4,
                retry_after_seconds = $5,
                retry_allowed = $6,
                retry_reason = $7,
                owner = COALESCE($8, owner),
                name = COALESCE($9, name),
                updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(state_name(state))
    .bind(error_code_name(error.code()))
    .bind(error.message())
    .bind(
        error
            .retry_after_seconds()
            .map(i32::try_from)
            .and_then(Result::ok),
    )
    // Retry is not offered yet: the endpoint that would accept it is #6/#13
    // work, and advertising a control the API cannot serve is worse than
    // offering none.
    .bind(false)
    .bind(if retriable {
        "Retry is not available in this build."
    } else {
        "This failure is deterministic: the same commit and ruleset will fail again."
    })
    .bind(canonical.map(|coordinate| coordinate.owner.as_str()))
    .bind(canonical.map(|coordinate| coordinate.name.as_str()))
    .execute(pool)
    .await?;
    Ok(())
}

/// Stores a finished report and marks the analysis complete.
///
/// One transaction: an analysis marked `COMPLETED` whose report never landed
/// would send the frontend to an endpoint that 404s, and the two writes have no
/// meaning apart.
///
/// # Errors
///
/// Returns [`StoreError::Query`] when either write fails.
pub async fn complete(pool: &PgPool, id: Uuid, report: &Report) -> Result<(), StoreError> {
    let document = serde_json::to_value(report)
        .map_err(|error| StoreError::Query(sqlx::Error::Encode(Box::new(error))))?;

    let mut transaction = pool.begin().await?;

    sqlx::query("INSERT INTO reports (analysis_id, document) VALUES ($1, $2)")
        .bind(id)
        .bind(&document)
        .execute(&mut *transaction)
        .await?;

    // Owner and name come from the report, in the same transaction that marks
    // the analysis complete.
    //
    // They were previously left to a separate best-effort update made when the
    // redirect was discovered, so a transient failure of that write left a
    // COMPLETED analysis naming one repository while its own report named
    // another — two answers to "what was analyzed" from one row and its
    // document. The report is the authority here because it is what the
    // pipeline actually built its findings from.
    sqlx::query(
        "UPDATE analyses
            SET state = 'COMPLETED',
                commit_sha = $2,
                owner = $3,
                name = $4,
                updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&report.commit_sha)
    .bind(&report.repository.owner)
    .bind(&report.repository.name)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(())
}

/// Reads one analysis.
///
/// # Errors
///
/// Returns [`StoreError::NotFound`] when no such analysis exists.
pub async fn load_analysis(pool: &PgPool, id: Uuid) -> Result<Analysis, StoreError> {
    let row = sqlx::query(
        "SELECT a.id, a.owner, a.name, a.commit_sha, a.state,
                a.error_code, a.error_message, a.retry_after_seconds,
                a.retry_allowed, a.retry_reason, a.created_at, a.updated_at,
                (r.analysis_id IS NOT NULL) AS report_available
           FROM analyses a
           LEFT JOIN reports r ON r.analysis_id = a.id
          WHERE a.id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    let state = parse_state(row.get("state"))?;
    let error = row
        .get::<Option<String>, _>("error_code")
        .and_then(|code| parse_error_code(&code))
        .map(|code| {
            let message: String = row.get("error_message");
            match row.get::<Option<i32>, _>("retry_after_seconds") {
                Some(seconds) if code == ErrorCode::RateLimited => {
                    ApiError::rate_limited(message, u32::try_from(seconds).unwrap_or(0))
                }
                _ => ApiError::new(code, message),
            }
        });

    let created_at: OffsetDateTime = row.get("created_at");

    Ok(Analysis {
        id: row.get("id"),
        repository: RepositoryIdentity {
            owner: row.get("owner"),
            name: row.get("name"),
        },
        commit_sha: row.get("commit_sha"),
        state,
        execution: ExecutionMetadata {
            trigger_status: TriggerStatus::Succeeded,
            execution_id: None,
            triggered_at: Some(created_at),
        },
        retry: RetryPolicy {
            allowed: row.get("retry_allowed"),
            reason: row.get("retry_reason"),
        },
        error,
        created_at,
        updated_at: row.get("updated_at"),
        // Absent in a terminal state: there is nothing left to poll for, and a
        // client that kept polling a finished analysis would pay for cold
        // starts forever.
        poll_after_ms: (!state.is_terminal()).then_some(POLL_INTERVAL_MS),
        report_available: row.get("report_available"),
    })
}

/// Reads a finished report.
///
/// # Errors
///
/// Returns [`StoreError::NotFound`] when no report exists for that analysis.
pub async fn load_report(pool: &PgPool, id: Uuid) -> Result<Report, StoreError> {
    let row = sqlx::query("SELECT document FROM reports WHERE analysis_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;

    let document: serde_json::Value = row.get("document");
    serde_json::from_value(document)
        .map_err(|error| StoreError::Query(sqlx::Error::Decode(Box::new(error))))
}

/// Wire name for a state, matching the contract exactly.
///
/// A `match` rather than `serde_json::to_string` plus trimming: the mapping is
/// the same one the wire uses, and writing it out means a new state fails to
/// compile here instead of silently storing something no reader recognises.
const fn state_name(state: AnalysisState) -> &'static str {
    match state {
        AnalysisState::Queued => "QUEUED",
        AnalysisState::Resolving => "RESOLVING",
        AnalysisState::Collecting => "COLLECTING",
        AnalysisState::Analyzing => "ANALYZING",
        AnalysisState::BuildingReport => "BUILDING_REPORT",
        AnalysisState::Completed => "COMPLETED",
        AnalysisState::FailedRetriable => "FAILED_RETRIABLE",
        AnalysisState::FailedPermanent => "FAILED_PERMANENT",
    }
}

fn parse_state(name: String) -> Result<AnalysisState, StoreError> {
    serde_json::from_value(serde_json::Value::String(name))
        .map_err(|error| StoreError::Query(sqlx::Error::Decode(Box::new(error))))
}

const fn error_code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidRepositoryUrl => "INVALID_REPOSITORY_URL",
        ErrorCode::RepositoryNotFound => "REPOSITORY_NOT_FOUND",
        ErrorCode::RepositoryInaccessible => "REPOSITORY_INACCESSIBLE",
        ErrorCode::RepositoryArchived => "REPOSITORY_ARCHIVED",
        ErrorCode::RepositoryTooLarge => "REPOSITORY_TOO_LARGE",
        ErrorCode::RateLimited => "RATE_LIMITED",
        ErrorCode::WorkerFailedRetriable => "WORKER_FAILED_RETRIABLE",
        ErrorCode::AnalyzerFailedPermanent => "ANALYZER_FAILED_PERMANENT",
        ErrorCode::AnalysisNotFound => "ANALYSIS_NOT_FOUND",
        ErrorCode::MalformedRequest => "MALFORMED_REQUEST",
        ErrorCode::RequestTooLarge => "REQUEST_TOO_LARGE",
        ErrorCode::RequestTimedOut => "REQUEST_TIMED_OUT",
        ErrorCode::InternalError => "INTERNAL_ERROR",
    }
}

fn parse_error_code(name: &str) -> Option<ErrorCode> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_wire_name_matching_the_contract() {
        // The stored name and the wire name are the same string. If they drifted,
        // an analysis would round-trip into a state the frontend never sees.
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
            let wire = serde_json::to_string(&state).unwrap();
            assert_eq!(
                format!("\"{}\"", state_name(state)),
                wire,
                "stored name and wire name disagree for {state:?}"
            );
            assert_eq!(parse_state(state_name(state).to_owned()).unwrap(), state);
        }
    }

    #[test]
    fn every_error_code_round_trips_through_its_stored_name() {
        for code in ErrorCode::ALL {
            let wire = serde_json::to_string(&code).unwrap();
            assert_eq!(format!("\"{}\"", error_code_name(code)), wire);
            assert_eq!(parse_error_code(error_code_name(code)), Some(code));
        }
    }

    #[test]
    fn an_unrecognised_stored_state_is_an_error_rather_than_a_guess() {
        assert!(parse_state("PAUSED_BY_OPERATOR".to_owned()).is_err());
    }
}
