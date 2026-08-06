//! Environment configuration.
//!
//! Hand-rolled rather than layered through a configuration crate: every
//! deployed environment is configured purely by environment variables, so
//! multi-format layering would be unused weight. What matters is failing fast
//! and naming the missing variable, which is what these functions do.
//!
//! The one outbound credential, `GH_ANALYSIS_TOKEN`, is returned as a
//! `SecretString` so it cannot reach a log through an accidental `Debug` or
//! `Display`. Database URLs are still plain `String`, because `sqlx` takes a
//! `&str` and wrapping a value only to unwrap it at the single call site would
//! be ceremony rather than protection — `warn_on_weak_tls` below is written to
//! never emit one.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use secrecy::SecretString;

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

/// Why an environment file could not be loaded.
///
/// A category, never the underlying message. `dotenvy::Error::LineParse`
/// carries the **entire malformed line** and prints it through `Display`, so a
/// mistyped `DATABASE_URL=` or token line would otherwise be copied verbatim
/// into the log — turning a typo into a credential disclosure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotenvErrorKind {
    /// The file could not be read.
    Io,
    /// A line was malformed. The line itself is deliberately discarded.
    Parse,
    /// A variable could not be applied to the process environment.
    Environment,
}

impl DotenvErrorKind {
    /// Stable label for logs.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Parse => "parse",
            Self::Environment => "environment",
        }
    }
}

/// What happened while loading one environment file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotenvOutcome {
    /// File that was attempted.
    pub filename: &'static str,
    /// Failure category, or `None` when the file loaded or was simply absent.
    pub error: Option<DotenvErrorKind>,
    /// Character index of a parse failure, when the error reported one. Safe to
    /// log: a position locates the problem without reproducing its content.
    pub index: Option<usize>,
}

/// Loads `.env.local`, then `.env`, without overriding what is already set.
///
/// Call once, first thing in `main`, and pass the result to [`report_dotenv`]
/// **after** telemetry is initialised. Splitting the two is not ceremony: this
/// has to run before `telemetry::init()` so `RUST_LOG` from the file is
/// honoured, which means any `tracing` call made here would be emitted before a
/// subscriber exists and silently discarded.
///
/// Without this a developer has to `set -a; . ./.env.local` before every `cargo
/// run`, and forgetting produces a server with no database rather than an
/// error.
///
/// Deliberately does not override existing variables: a value explicitly
/// exported for one command must win over a file, or `DATABASE_URL=... cargo
/// run` would silently do something else.
///
/// Paths are relative to the working directory, so binaries are run from the
/// workspace root. In a deployed environment neither file exists and this is a
/// no-op — Cloud Run supplies the variables, and a `.env` in a container image
/// would be a packaging mistake worth failing on rather than absorbing.
#[must_use]
pub fn load_dotenv() -> Vec<DotenvOutcome> {
    let mut outcomes = Vec::new();

    for filename in [".env.local", ".env"] {
        match dotenvy::from_filename(filename) {
            Ok(_) => outcomes.push(DotenvOutcome {
                filename,
                error: None,
                index: None,
            }),
            // Absent is the normal case in production and not worth reporting.
            Err(error) if error.not_found() => {}
            Err(error) => {
                let (kind, index) = match &error {
                    // The `String` here is the offending line. It is matched
                    // but never bound, so it cannot reach a log by accident.
                    dotenvy::Error::LineParse(_, index) => (DotenvErrorKind::Parse, Some(*index)),
                    dotenvy::Error::EnvVar(_) => (DotenvErrorKind::Environment, None),
                    // `Io`, plus any variant added later. Defaulting an unknown
                    // variant to a read failure is safe: it names a category
                    // without reproducing content we have not inspected.
                    _ => (DotenvErrorKind::Io, None),
                };
                outcomes.push(DotenvOutcome {
                    filename,
                    error: Some(kind),
                    index,
                });
            }
        }
    }

    outcomes
}

/// Logs what [`load_dotenv`] found, once a subscriber exists.
///
/// Emits only the filename, the category, and a character index. Never the
/// error text and never the source line.
pub fn report_dotenv(outcomes: &[DotenvOutcome]) {
    for outcome in outcomes {
        match outcome.error {
            None => tracing::debug!(file = outcome.filename, "loaded environment file"),
            Some(kind) => tracing::warn!(
                file = outcome.filename,
                error = kind.as_str(),
                index = outcome.index,
                "could not load environment file; contents are not logged"
            ),
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

/// Credential raising the GitHub rate-limit ceiling, if one is configured.
///
/// Optional on purpose. Unauthenticated ingestion works — GitHub allows roughly
/// sixty requests an hour from an anonymous client, and only public
/// repositories are ever read, so a token widens the budget without widening
/// what is visible.
///
/// Absent is therefore a *lower ceiling*, not a broken deployment. Treating it
/// as fatal would invent a failure before trying, and would report
/// `REPOSITORY_INACCESSIBLE` for a repository that is in fact perfectly
/// accessible. Whether a request succeeds is GitHub's answer to give.
pub fn github_token() -> Option<SecretString> {
    let raw = env::var("GH_ANALYSIS_TOKEN").ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(SecretString::from(raw))
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
    let raw = env::var("CORS_ALLOWED_ORIGIN").ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    match parse_origin(raw) {
        Ok(origin) => Some(origin),
        Err(reason) => {
            // Rejected rather than passed through. The previous version accepted
            // any string that could become a header value, which let `*` and
            // path-bearing values through while the architecture claimed they
            // were impossible.
            tracing::error!(
                value = raw,
                reason,
                "CORS_ALLOWED_ORIGIN is not a valid origin; serving without CORS"
            );
            None
        }
    }
}

/// Validates a browser origin: scheme, host, optional port — nothing else.
///
/// Hand-rolled rather than pulling in `url`, which belongs to the ingestion
/// stage (#4). The grammar being enforced is small and closed, and writing it
/// out states the rule the architecture claims rather than delegating it.
///
/// A trailing slash is tolerated and normalised away, because `Origin` headers
/// never carry one and copying a browser address bar is the obvious mistake.
///
/// # Errors
///
/// Returns a short, static reason suitable for logging next to the value.
fn parse_origin(value: &str) -> Result<String, &'static str> {
    // `*` would permit every site on the internet, and `null` is what a
    // sandboxed iframe or `file://` page sends — neither is an origin we could
    // have deliberately chosen.
    if value == "*" || value.eq_ignore_ascii_case("null") {
        return Err("wildcard and null are never valid origins");
    }

    let (scheme, rest) = value.split_once("://").ok_or("missing scheme")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err("scheme must be http or https");
    }

    // Everything after the authority must be empty. A path, query, or fragment
    // means this is a URL rather than an origin, and browsers would never match
    // it against the `Origin` header.
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if authority.contains('/') {
        return Err("an origin has no path");
    }
    if authority.contains('?') || authority.contains('#') {
        return Err("an origin has no query or fragment");
    }
    if authority.contains('@') {
        return Err("an origin carries no credentials");
    }
    if authority.is_empty() {
        return Err("missing host");
    }
    if authority.contains(char::is_whitespace) {
        return Err("an origin contains no whitespace");
    }

    // Reject `https://host:` and `https://host:notaport` before the header
    // parser turns them into something surprising.
    if let Some((host, port)) = authority.rsplit_once(':')
        && !host.is_empty()
        && !host.contains(']')
        && (port.is_empty() || port.parse::<u16>().is_err())
    {
        return Err("port must be a number");
    }

    Ok(format!("{}://{}", scheme.to_ascii_lowercase(), authority))
}

#[cfg(test)]
mod origin_tests {
    use super::parse_origin;

    #[test]
    fn accepts_real_origins() {
        for (input, expected) in [
            ("https://repolens.pages.dev", "https://repolens.pages.dev"),
            ("http://localhost:5173", "http://localhost:5173"),
            // A trailing slash is what copying a browser address bar produces.
            ("https://example.test/", "https://example.test"),
            ("HTTPS://Example.test", "https://Example.test"),
            ("https://[::1]:8080", "https://[::1]:8080"),
        ] {
            assert_eq!(
                parse_origin(input).as_deref(),
                Ok(expected),
                "input: {input}"
            );
        }
    }

    #[test]
    fn rejects_wildcard_and_null() {
        // These are the two values that would silently widen the policy to
        // everything, which is what the architecture claims is impossible.
        assert!(parse_origin("*").is_err());
        assert!(parse_origin("null").is_err());
        assert!(parse_origin("NULL").is_err());
    }

    #[test]
    fn rejects_anything_that_is_not_an_origin() {
        for input in [
            "",
            "example.test",                  // no scheme
            "ftp://example.test",            // wrong scheme
            "https://",                      // no host
            "https://example.test/path",     // path
            "https://example.test?q=1",      // query
            "https://example.test#frag",     // fragment
            "https://user:pw@example.test",  // credentials
            "https://example.test:",         // empty port
            "https://example.test:notaport", // non-numeric port
            "https://example.test:99999",    // port out of range
            "https://exa mple.test",         // whitespace
        ] {
            assert!(parse_origin(input).is_err(), "should reject: {input:?}");
        }
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
