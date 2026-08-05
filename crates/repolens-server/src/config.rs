//! Environment configuration.
//!
//! Hand-rolled rather than layered through a configuration crate: every
//! deployed environment is configured purely by environment variables, so
//! multi-format layering would be unused weight. What matters is failing fast
//! and naming the missing variable, which is what these functions do.
//!
//! Secrets are returned as plain `String` today. Wrapping them in `secrecy`
//! arrives with the ingestion stage that introduces the first outbound
//! credential (issue #4); see plan §4.3.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Cloud Run injects the listening port; the default matches its convention.
const DEFAULT_PORT: u16 = 8080;

/// Configuration failures. Always fatal at startup — a service that guesses at
/// a missing database URL is worse than one that refuses to boot.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable was absent or empty.
    #[error("required environment variable {0} is not set")]
    Missing(&'static str),
    /// A variable was present but could not be interpreted.
    #[error("environment variable {name} is invalid: {reason}")]
    Invalid {
        /// Variable name. The value is never included — it may be a secret.
        name: &'static str,
        /// What was wrong with it.
        reason: String,
    },
}

/// Loads `.env.local`, then `.env`, without overriding what is already set.
///
/// Call once, first thing in `main`. Without this the binaries see only the
/// ambient environment, so a developer has to `set -a; . ./.env.local` before
/// every `cargo run` — and forgetting silently produces a server with no
/// database rather than an error.
///
/// Deliberately does not override existing variables: a value explicitly
/// exported for one command must win over a file, or `DATABASE_URL=... cargo
/// run` would silently do something else.
///
/// Paths are relative to the working directory, so binaries are run from the
/// workspace root. In a deployed environment neither file exists and this is a
/// no-op — Cloud Run supplies the variables, and a `.env` in a container image
/// would be a packaging mistake worth failing on rather than absorbing.
pub fn load_dotenv() {
    for filename in [".env.local", ".env"] {
        match dotenvy::from_filename(filename) {
            Ok(path) => tracing::debug!(?path, "loaded environment file"),
            // Absent is the normal case in production; anything else is worth
            // seeing, but never fatal — the file is a convenience, not a
            // contract. Missing *variables* still fail loudly at their point of
            // use.
            Err(error) if error.not_found() => {}
            Err(error) => {
                tracing::warn!(filename, %error, "could not read environment file");
            }
        }
    }
}

/// Address the HTTP server binds to.
///
/// Binds to all interfaces because the process runs inside a container whose
/// network namespace is the isolation boundary.
pub fn bind_address() -> Result<SocketAddr, ConfigError> {
    let port = match env::var("PORT") {
        Ok(raw) => raw
            .trim()
            .parse::<u16>()
            .map_err(|error| ConfigError::Invalid {
                name: "PORT",
                reason: error.to_string(),
            })?,
        Err(_) => DEFAULT_PORT,
    };

    Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port))
}

/// Pooled database connection string, for the API and ordinary worker
/// transactions.
pub fn database_url() -> Result<String, ConfigError> {
    required("DATABASE_URL")
}

/// Direct, unpooled database connection string.
///
/// Migrations and session-dependent administration use this endpoint. Pooled
/// connections restrict session-level behaviour — `LISTEN`, `SET`, cursors,
/// advisory locks — which is exactly what schema changes rely on.
pub fn database_direct_url() -> Result<String, ConfigError> {
    required("DATABASE_DIRECT_URL")
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => {
            warn_on_weak_tls(name, &value);
            Ok(value)
        }
        _ => Err(ConfigError::Missing(name)),
    }
}

/// Warns when a database URL does not verify the server's hostname.
///
/// Measured against a real Neon endpoint, not assumed. Neon issues connection
/// strings containing `sslmode=require` and `channel_binding=require`:
///
/// * This build enables `sqlx`'s `tls-rustls-ring-native-roots`, so with system
///   roots available `sslmode=require` can validate the certificate **chain**,
///   behaving like `verify-ca`. What it does not guarantee is **hostname
///   identity** — only `verify-full` checks both trust and hostname.
/// * `sqlx` does not implement `channel_binding` and silently ignores it, which
///   is visible in its own log as `ignoring unrecognized connect parameter`.
///   The parameter should be removed rather than left implying a protection
///   the client does not provide.
///
/// `sslmode=verify-full` is confirmed to work against Neon.
///
/// This warns rather than refuses: a developer pointing at a local PostgreSQL
/// without TLS has a legitimate reason to. Production enforcement belongs with
/// the deployment work in issue #9, where the URL comes from Secret Manager and
/// there is no such case.
fn warn_on_weak_tls(name: &'static str, url: &str) {
    // Substring rather than a URL parse: this must never panic or allocate on a
    // secret, and the parameter is unambiguous enough not to need parsing.
    if url.contains("sslmode=verify-full") {
        return;
    }

    // Local development over a plaintext socket is a deliberate choice, not an
    // oversight worth shouting about.
    if url.contains("@localhost") || url.contains("@127.0.0.1") {
        return;
    }

    tracing::warn!(
        variable = name,
        "database URL does not use sslmode=verify-full, so hostname identity is not verified. \
         With native roots the certificate chain may still be validated like verify-ca, but \
         only verify-full checks both trust and hostname. sqlx also ignores the \
         channel_binding parameter, so remove it rather than implying a protection the client \
         does not provide."
    );
}

/// Exact origin permitted to call this API from a browser, if any.
///
/// A statically hosted frontend on Cloudflare calling Cloud Run is
/// **cross-origin**, so without this the browser blocks every request — which
/// is precisely the class of failure the walking skeleton exists to surface
/// before it reaches production.
///
/// Absent means no CORS layer at all, which is correct for same-origin local
/// development and for the container's own health checks. It is never a
/// wildcard: `Access-Control-Allow-Origin: *` would have to be revisited the
/// moment any endpoint requires credentials, and a permissive default is a
/// security decision made by omission.
pub fn cors_allowed_origin() -> Option<String> {
    match env::var("CORS_ALLOWED_ORIGIN") {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tls_tests {
    use super::warn_on_weak_tls;

    // The function's contract is "never panic, never leak" — it is handed a
    // string containing a password on every startup.
    #[test]
    fn tolerates_any_shape_of_url() {
        // `.invalid` is reserved by RFC 2606 and can never resolve, and the
        // credentials are named rather than plausible. A fixture that merely
        // *looks* fake still trips credential scanners and still makes a
        // reviewer stop and check.
        for url in [
            "",
            "not-a-url",
            "postgres://EXAMPLE_USER:EXAMPLE_PASSWORD@db.example.invalid/db?sslmode=require",
            "postgres://EXAMPLE_USER:EXAMPLE_PASSWORD@db.example.invalid/db?sslmode=verify-full",
            "postgres://EXAMPLE_USER:EXAMPLE_PASSWORD@localhost/db",
        ] {
            warn_on_weak_tls("DATABASE_URL", url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, bind_address};

    #[test]
    fn error_messages_never_echo_a_value() {
        let error = ConfigError::Invalid {
            name: "DATABASE_URL",
            reason: "unsupported scheme".to_owned(),
        };
        let rendered = error.to_string();
        assert!(rendered.contains("DATABASE_URL"));
        assert!(!rendered.contains("postgres://"));
    }

    #[test]
    fn bind_address_defaults_to_all_interfaces() {
        // PORT is read from the ambient environment; only the host portion is
        // asserted so this stays independent of how the test runner is invoked.
        let address = bind_address().expect("default is always valid");
        assert!(address.ip().is_unspecified());
    }
}
