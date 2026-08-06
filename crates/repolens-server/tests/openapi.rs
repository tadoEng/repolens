//! OpenAPI document staleness gate.
//!
//! `contracts/openapi.json` is the handshake between this service and the
//! generated TypeScript client. It is committed so the frontend can build
//! without running the backend, which means it can also go stale — a route
//! changes, the document does not, and the frontend keeps compiling against an
//! API that no longer exists.
//!
//! This is the backend half of that gate. The frontend half is
//! `packages/repolens-api-client/schema.test.ts`, which regenerates `schema.ts`
//! from this document. Together they make drift a build failure at both ends
//! rather than a runtime surprise.
//!
//! Regenerate after intentionally changing a route or DTO:
//!
//! ```sh
//! UPDATE_OPENAPI=1 cargo test -p repolens-server --test openapi
//! ```

use std::fs;
use std::path::PathBuf;

use repolens_server::api;
use repolens_server::state::AppState;

/// Location of the committed document, relative to this crate.
fn document_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/openapi.json")
}

/// Serializes the document the router actually produces.
///
/// Pretty-printed with a trailing newline so the committed file is reviewable
/// as a diff rather than as one line, and so it matches what an editor would
/// leave behind.
fn generate() -> String {
    let (_router, openapi) = api::build(AppState::without_database());
    let mut json = serde_json::to_string_pretty(&openapi).expect("the document always serializes");
    json.push('\n');
    json
}

#[test]
fn committed_document_matches_the_routes() {
    let path = document_path();
    let generated = generate();

    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("creating contracts/");
        }
        fs::write(&path, &generated).expect("writing contracts/openapi.json");
        return;
    }

    let committed = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "contracts/openapi.json could not be read ({error}).\n\
             Generate it with: UPDATE_OPENAPI=1 cargo test -p repolens-server --test openapi"
        )
    });

    // Git may check the file out with CRLF on a platform that has not honoured
    // .gitattributes yet; the comparison is about content, not line endings.
    assert_eq!(
        committed.replace("\r\n", "\n"),
        generated.replace("\r\n", "\n"),
        "contracts/openapi.json is stale.\n\
         Regenerate with: UPDATE_OPENAPI=1 cargo test -p repolens-server --test openapi\n\
         Then regenerate the client: pnpm --filter @repolens/api-client schema:update"
    );
}

#[test]
fn probe_is_published_under_the_versioned_prefix() {
    // The frontend reaches this API only through the generated client, so a
    // path that silently moved would surface as a 404 in the browser rather
    // than as a failing build. Asserting the path here keeps that a
    // backend-side decision.
    let generated = generate();
    assert!(
        generated.contains("/api/v1/system/probe"),
        "the system probe must be published at /api/v1/system/probe"
    );
}

/// Walks every schema property name in the document.
///
/// Recursive rather than a spot check: nesting is where a stray `rename_all`
/// hides. A convention asserted only on the fields somebody remembered is a
/// convention that holds until the first `struct` nobody thought to list.
fn collect_property_names(node: &serde_json::Value, out: &mut Vec<String>) {
    match node {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if key == "properties"
                    && let Some(properties) = value.as_object()
                {
                    out.extend(properties.keys().cloned());
                }
                collect_property_names(value, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_property_names(item, out);
            }
        }
        _ => {}
    }
}

#[test]
fn every_property_name_is_snake_case() {
    // Issue #14 settled snake_case object fields. Documenting that is not
    // enforcing it: the failure mode is one DTO acquiring `rename_all`, which
    // compiles, passes every existing test, and silently breaks every consumer
    // reading the old name.
    let document: serde_json::Value = serde_json::from_str(&generate()).expect("valid JSON");

    let mut names = Vec::new();
    collect_property_names(&document, &mut names);
    assert!(
        !names.is_empty(),
        "no properties found — the walk is broken"
    );

    let offenders: Vec<&String> = names
        .iter()
        .filter(|name| name.chars().any(|c| c.is_ascii_uppercase()))
        .collect();

    assert!(
        offenders.is_empty(),
        "these property names are not snake_case: {offenders:?}"
    );
}

#[test]
fn every_enum_value_is_screaming_snake_case() {
    // The mirror rule. Rust's PascalCase variants cannot produce it, so a
    // missing `rename_all` on an enum shows up here rather than in a browser.
    let document: serde_json::Value = serde_json::from_str(&generate()).expect("valid JSON");
    let schemas = document["components"]["schemas"]
        .as_object()
        .expect("components.schemas");

    let mut checked = 0;
    for (name, schema) in schemas {
        let Some(values) = schema.get("enum").and_then(|v| v.as_array()) else {
            continue;
        };
        for value in values {
            let text = value.as_str().expect("enum values are strings");
            checked += 1;
            assert!(
                text.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
                "{name} has a non-SCREAMING_SNAKE_CASE value: {text}"
            );
        }
    }

    assert!(checked > 0, "no enum values found — the walk is broken");
}

#[test]
fn contract_uses_the_settled_naming_convention() {
    // Issue #14 settled snake_case object fields and SCREAMING_SNAKE_CASE enum
    // values. Asserting it on the emitted document catches a stray serde
    // attribute at the point it is introduced, rather than after it has been
    // baked into generated TypeScript and a fixture.
    let generated = generate();

    assert!(
        generated.contains("build_sha") && generated.contains("schema_version"),
        "object fields are snake_case"
    );
    assert!(
        !generated.contains("buildSha") && !generated.contains("schemaVersion"),
        "camelCase field leaked into the contract"
    );
    for value in ["OK", "DEGRADED", "UNAVAILABLE"] {
        assert!(
            generated.contains(value),
            "enum value {value} must be SCREAMING_SNAKE_CASE"
        );
    }
}
