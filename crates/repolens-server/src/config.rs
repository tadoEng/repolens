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
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::Missing(name)),
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
