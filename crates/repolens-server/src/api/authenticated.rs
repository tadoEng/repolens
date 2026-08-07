//! The authentication gate, as an extractor.
//!
//! An extractor rather than a middleware layer, because the gate applies to
//! exactly one route and a layer would have to name paths. A handler that takes
//! [`AuthenticatedUser`] cannot be reached without a verified token, and a
//! handler that does not take it is anonymous — both facts are visible in the
//! signature, which is where a reviewer looks.
//!
//! Reads stay anonymous on purpose: the unguessable analysis id *is* the
//! capability, which is what lets a report be shared by URL and viewed by
//! someone who has never signed in.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;

use crate::api::failure::Failure;
use crate::auth::{AuthError, AuthenticatedUser, bearer_token};
use crate::contract::error::{ApiError, ErrorCode};
use crate::state::AppState;

impl Failure {
    /// No usable credential was presented.
    ///
    /// One message for absent, malformed, invalid and expired. Which check
    /// failed is recorded in the log and withheld from the response: it helps
    /// someone probing far more than it helps someone holding a valid token.
    fn unauthenticated() -> Self {
        Self(
            StatusCode::UNAUTHORIZED,
            ApiError::new(
                ErrorCode::Unauthenticated,
                "Starting an analysis requires signing in. Sign in and try again.",
            ),
        )
    }

    /// Sign-in could not be checked.
    ///
    /// `503`, not `401`. A client that treated an outage of Google's key
    /// endpoint — or a deployment with no authentication configured — as a
    /// rejected credential would sign a perfectly valid user out.
    fn authentication_unavailable() -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                ErrorCode::AuthenticationUnavailable,
                "Sign-in cannot be checked right now, so this request was refused rather than \
                 allowed through. This is usually temporary.",
            ),
        )
    }
}

/// A verified caller, as a handler argument.
///
/// A newtype around [`AuthenticatedUser`] rather than an impl on it directly:
/// the rejection type is this crate's private `Failure`, and a public type
/// cannot name it. Keeping the extractor here also keeps `auth` free of HTTP.
pub(crate) struct Caller(#[allow(dead_code)] pub(crate) AuthenticatedUser);

impl FromRequestParts<AppState> for Caller {
    type Rejection = Failure;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Absent configuration closes creation rather than opening it.
        //
        // The opposite default would mean a deployment that forgot
        // `FIREBASE_PROJECT_ID` served an anonymous, public, work-creating
        // endpoint — the exact thing this gate exists to prevent, arrived at by
        // omission. Refusing to start would be the other safe choice, but it
        // would also block local frontend work against a server with no
        // Firebase project, and the reads are what that work needs.
        let Some(verifier) = state.verifier() else {
            tracing::warn!(
                "an analysis creation was refused: no Firebase project is configured, so no \
                 token can be verified"
            );
            return Err(Failure::authentication_unavailable());
        };

        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());

        let Some(token) = bearer_token(header) else {
            tracing::info!(reason = AuthError::Missing.as_str(), "creation refused");
            return Err(Failure::unauthenticated());
        };

        match verifier.verify(token).await {
            Ok(user) => Ok(Self(user)),
            Err(AuthError::KeysUnavailable) => {
                // Ours, so it must not read as the caller's.
                Err(Failure::authentication_unavailable())
            }
            Err(reason) => {
                // The category, never the token. A bearer token is a live
                // credential for as long as it is valid, and a log is the last
                // place one should be able to be replayed from.
                tracing::info!(reason = reason.as_str(), "creation refused");
                Err(Failure::unauthenticated())
            }
        }
    }
}
