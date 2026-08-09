//! Application state.
//!
//! Bounded on purpose: everything reachable from a handler is listed here, so
//! "what can this endpoint touch?" is answered by reading one struct rather
//! than by tracing imports.

use std::collections::HashSet;
use std::sync::Arc;

use repolens_github::GitHubRestClient;

use crate::auth::SharedVerifier;
use crate::telemetry::metrics::Metrics;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// The commit this binary was built from.
///
/// Resolved at compile time from `REPOLENS_BUILD_SHA`, which the container
/// build sets. `option_env!` rather than `env!` so a local `cargo run` still
/// compiles — a developer build genuinely does not have a deployed SHA, and
/// reporting `unknown` is more honest than failing to build or inventing one.
pub const BUILD_SHA: &str = match option_env!("REPOLENS_BUILD_SHA") {
    Some(sha) => sha,
    None => "unknown",
};

/// Ceiling on pooled connections held by one instance.
///
/// Neon's free plan allows a small connection budget shared across every
/// compute, and Cloud Run may run several instances. A pool that sized itself
/// for one machine would exhaust the project's budget as soon as it scaled, so
/// this is deliberately small and is measured in issue #9 rather than guessed
/// at again later.
const MAX_POOL_CONNECTIONS: u32 = 5;

/// Everything a request handler may reach.
#[derive(Clone)]
pub struct AppState {
    /// Pooled database handle, absent when `DATABASE_URL` is not configured.
    ///
    /// Optional so the server still starts for frontend work without a
    /// database. The probe then reports the database as unavailable, which is
    /// the truthful answer — better than refusing to boot and blocking UI
    /// development, and better than pretending the database is fine.
    pool: Option<PgPool>,
    /// GitHub access. Always present.
    ///
    /// Deliberately not an `Option`, unlike the pool. A client can always be
    /// built: the public API base is a compile-time constant and the token is
    /// optional, so "no GitHub access" describes no reachable configuration.
    /// Modelling it as absent would let a handler answer
    /// `REPOSITORY_INACCESSIBLE` without asking GitHub — inventing a failure for
    /// a repository that is very likely accessible, since only public ones are
    /// read. Whether a request succeeds is GitHub's answer to give.
    ///
    /// `Arc` because the client is not `Clone` — it holds a configured
    /// `reqwest::Client` and a secret — and an analysis is spawned onto a task
    /// that outlives the request.
    github: Arc<GitHubRestClient>,
    /// Firebase ID token verifier, absent when no project is configured.
    ///
    /// `Option` unlike `github`, and the asymmetry is deliberate. A GitHub
    /// client can always be built, so "no GitHub access" describes no reachable
    /// configuration. A verifier cannot: it needs a project id, and a
    /// deployment may genuinely not have one. Modelling that honestly is what
    /// lets creation **close** in that case rather than silently run
    /// unauthenticated — see `api::authenticated`.
    verifier: Option<SharedVerifier>,
    /// Firebase uids permitted to read the operational snapshot.
    ///
    /// **Not** an `Option`, and the difference from `verifier` above is the
    /// point. An absent verifier means identity cannot be established at all,
    /// which is a distinct answer a caller must be told (`503`, not `401`). An
    /// absent allowlist is not a third state: it is an empty one, and an empty
    /// one already means what it should — nobody is an admin. Modelling it as
    /// `Option<HashSet<_>>` would create a `None` case indistinguishable in
    /// behaviour from `Some(empty)` and invite a future reader to give the two
    /// different meanings, which is exactly how a door gets opened by accident.
    ///
    /// A set rather than a `Vec`: membership is the only question ever asked of
    /// it, and the lists are small enough that the choice is about saying so.
    admin_uids: Arc<HashSet<String>>,
    /// What this process has observed about itself.
    ///
    /// Held here rather than beside the router because there must be exactly
    /// one: the middleware that records into it and the handler that will read
    /// it are built at different points, and two registries would show a
    /// dashboard the half of the traffic it happened to be handed.
    ///
    /// Not an `Option`. A registry is always constructible and costs a few
    /// hundred bytes; modelling it as absent would give a future reader a
    /// "metrics unavailable" branch describing no reachable configuration.
    metrics: Metrics,
}

impl AppState {
    /// Builds state with a live pool.
    #[must_use]
    pub fn with_pool(pool: PgPool, github: GitHubRestClient) -> Self {
        Self {
            pool: Some(pool),
            github: Arc::new(github),
            verifier: None,
            admin_uids: Arc::new(HashSet::new()),
            metrics: Metrics::new(),
        }
    }

    /// Attaches a token verifier, enabling authenticated creation.
    #[must_use]
    pub fn with_verifier(mut self, verifier: SharedVerifier) -> Self {
        self.verifier = Some(verifier);
        self
    }

    /// Borrows the verifier, when one is configured.
    #[must_use]
    pub fn verifier(&self) -> Option<&SharedVerifier> {
        self.verifier.as_ref()
    }

    /// Sets the uids permitted to read the operational snapshot.
    ///
    /// Takes owned strings from [`crate::config::admin_firebase_uids`], which
    /// has already discarded blank entries — an allowlist containing the empty
    /// string would admit any caller whose uid arrived as nothing.
    #[must_use]
    pub fn with_admin_uids(mut self, uids: impl IntoIterator<Item = String>) -> Self {
        self.admin_uids = Arc::new(uids.into_iter().collect());
        self
    }

    /// Whether `uid` may read the operational snapshot.
    ///
    /// Compared exactly. A Firebase uid is case-sensitive, so an allowlist that
    /// helpfully folded case would admit an identity nobody configured — the
    /// same rule the parser in [`crate::config`] keeps, and it has to be kept at
    /// both ends or one of them is decorative.
    #[must_use]
    pub fn is_admin(&self, uid: &str) -> bool {
        self.admin_uids.contains(uid)
    }

    /// Whether anybody at all is configured as an admin.
    ///
    /// Exists only so a refusal can be logged with the reason an operator needs.
    /// "Nobody is allow-listed" and "you specifically are not" are the same
    /// `403` to the caller, deliberately, and completely different problems to
    /// whoever deployed this. The *list itself* is never logged: it is not a
    /// secret the way a token is, but it names people.
    #[must_use]
    pub fn has_admins(&self) -> bool {
        !self.admin_uids.is_empty()
    }

    /// Borrows the GitHub client.
    #[must_use]
    pub fn github(&self) -> &Arc<GitHubRestClient> {
        &self.github
    }

    /// Builds state with no database configured.
    #[must_use]
    pub fn without_database(github: GitHubRestClient) -> Self {
        Self {
            pool: None,
            github: Arc::new(github),
            verifier: None,
            admin_uids: Arc::new(HashSet::new()),
            metrics: Metrics::new(),
        }
    }

    /// Borrows this process's metrics registry.
    #[must_use]
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Borrows the pool, if one was configured.
    #[must_use]
    pub fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    /// Connects lazily to `url`.
    ///
    /// Lazy on purpose: Neon scale-to-zero suspends an idle compute, so the
    /// first connection can take seconds while it resumes. Connecting eagerly
    /// at startup would push that latency into container start, where Cloud Run
    /// counts it against the startup probe rather than against a request.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL cannot be parsed into connection options.
    pub fn connect_lazy(url: &str) -> Result<PgPool, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(MAX_POOL_CONNECTIONS)
            .connect_lazy(url)
    }
}
