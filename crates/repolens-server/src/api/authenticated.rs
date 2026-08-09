//! The authentication and authorisation gates, as extractors.
//!
//! Extractors rather than middleware layers, because each gate applies to
//! exactly one route and a layer would have to name paths. A handler that takes
//! [`Caller`] cannot be reached without a verified token, one that takes
//! [`Admin`] cannot be reached without a verified token belonging to an
//! allow-listed uid, and a handler that takes neither is anonymous — all three
//! facts are visible in the signature, which is where a reviewer looks.
//!
//! Reads of an analysis stay anonymous on purpose: the unguessable analysis id
//! *is* the capability, which is what lets a report be shared by URL and viewed
//! by someone who has never signed in. The operational snapshot is the opposite
//! case — nothing about it is shareable, and `/admin` being hard to guess is not
//! access control.

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
    ///
    /// The message is a parameter because the remedy is phrased per endpoint and
    /// a caller reads it verbatim. The *code* is the same either way, which is
    /// what a client switches on.
    fn unauthenticated(message: &str) -> Self {
        Self(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthenticated, message),
        )
    }

    /// The caller is known, and is not permitted.
    ///
    /// `403`, not `401`. A client that treated this as a missing credential
    /// would send the user back through a sign-in flow that cannot change the
    /// answer — they were already signed in, and that is precisely the problem.
    ///
    /// The message names no allow-listed identity and does not say how the list
    /// is configured. Telling a caller who *is* permitted is telling them whose
    /// account to go after.
    fn forbidden() -> Self {
        Self(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "This account is not permitted to read operational data. Signing in again will \
                 not change that.",
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

/// Why the shared credential check did not yield a caller.
///
/// Two outcomes rather than one, because they are opposite claims about whose
/// fault the refusal is, and each gate below has to be able to answer for its
/// own endpoint. Collapsing them here would push the `401`/`503` decision into
/// two places that must agree forever.
enum Refusal {
    /// No usable credential was presented, or it did not verify.
    Unauthenticated,
    /// Nothing could be verified. Ours, not the caller's.
    Unavailable,
}

impl Refusal {
    /// Turns a refusal into a response, with the endpoint's own wording.
    fn into_failure(self, unauthenticated_message: &str) -> Failure {
        match self {
            Self::Unauthenticated => Failure::unauthenticated(unauthenticated_message),
            Self::Unavailable => Failure::authentication_unavailable(),
        }
    }
}

/// Verifies the bearer token on a request, if there is one to verify.
///
/// The single place a token becomes an identity. Both gates below call it, so a
/// change to how credentials are checked cannot apply to one endpoint and miss
/// the other — which is the failure mode of copying the block instead.
async fn verify_caller(parts: &mut Parts, state: &AppState) -> Result<AuthenticatedUser, Refusal> {
    // Absent configuration closes the door rather than opening it.
    //
    // The opposite default would mean a deployment that forgot
    // `FIREBASE_PROJECT_ID` served an anonymous, public, work-creating endpoint
    // and an anonymous operational snapshot — the exact things these gates
    // exist to prevent, arrived at by omission. Refusing to start would be the
    // other safe choice, but it would also block local frontend work against a
    // server with no Firebase project, and the anonymous reads are what that
    // work needs.
    let Some(verifier) = state.verifier() else {
        tracing::warn!(
            "a request was refused: no Firebase project is configured, so no token can be verified"
        );
        return Err(Refusal::Unavailable);
    };

    let header = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let Some(token) = bearer_token(header) else {
        tracing::info!(reason = AuthError::Missing.as_str(), "request refused");
        return Err(Refusal::Unauthenticated);
    };

    match verifier.verify(token).await {
        Ok(user) => Ok(user),
        // Ours, so it must not read as the caller's.
        Err(AuthError::KeysUnavailable) => Err(Refusal::Unavailable),
        Err(reason) => {
            // The category, never the token. A bearer token is a live credential
            // for as long as it is valid, and a log is the last place one should
            // be able to be replayed from.
            tracing::info!(reason = reason.as_str(), "request refused");
            Err(Refusal::Unauthenticated)
        }
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
        verify_caller(parts, state)
            .await
            .map(Self)
            .map_err(|refusal| {
                refusal.into_failure(
                    "Starting an analysis requires signing in. Sign in and try again.",
                )
            })
    }
}

/// A verified caller who is on the admin allowlist.
///
/// Authentication and authorisation are two questions and this asks both, in
/// that order. Asking only the first would make every signed-in user an
/// operator; asking only the second is not possible, since there is no identity
/// to check against the list until a token has verified.
pub(crate) struct Admin(#[allow(dead_code)] pub(crate) AuthenticatedUser);

impl FromRequestParts<AppState> for Admin {
    type Rejection = Failure;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = verify_caller(parts, state).await.map_err(|refusal| {
            refusal
                .into_failure("Reading operational data requires signing in as an administrator.")
        })?;

        if !state.is_admin(&user.uid) {
            // The uid is deliberately not logged. It identifies a person, and
            // the operational question — *is anyone configured at all* — is
            // answered without naming one. An operator who has just deployed
            // without `ADMIN_FIREBASE_UIDS` needs the first line; nobody needs
            // the second.
            if state.has_admins() {
                tracing::info!("an operational snapshot was refused: the caller is not an admin");
            } else {
                tracing::warn!(
                    "an operational snapshot was refused: ADMIN_FIREBASE_UIDS is empty, so no \
                     account can read it"
                );
            }
            return Err(Failure::forbidden());
        }

        Ok(Self(user))
    }
}
