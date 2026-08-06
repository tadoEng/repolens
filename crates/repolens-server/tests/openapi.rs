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

use std::collections::BTreeSet;
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

/// Whether a name is `snake_case` by grammar, not merely lowercase.
///
/// Checking only for absent uppercase accepts `commit-sha`, `commit sha`,
/// `_commit_sha` and `commit__sha` — every one of which would reach a client as
/// a field it cannot address with ordinary property syntax, or would signal a
/// rename nobody intended.
fn is_snake_case(name: &str) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    if name.ends_with('_') {
        return false;
    }
    if name.contains("__") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The mirror grammar for enum values.
fn is_screaming_snake_case(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    if value.ends_with('_') {
        return false;
    }
    if value.contains("__") {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
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

    let offenders: Vec<&String> = names.iter().filter(|name| !is_snake_case(name)).collect();

    assert!(
        offenders.is_empty(),
        "these property names are not snake_case: {offenders:?}"
    );
}

#[test]
fn the_snake_case_grammar_rejects_what_it_claims_to() {
    // Guards the guard. An earlier version tested only for absent uppercase and
    // would have accepted every name below.
    for good in [
        "commit_sha",
        "id",
        "schema_version",
        "sha256_digest",
        "a1_b2",
    ] {
        assert!(is_snake_case(good), "should accept {good}");
    }
    for bad in [
        "",
        "commitSha",
        "commit-sha",
        "commit sha",
        "_commit_sha",
        "commit_sha_",
        "commit__sha",
        "1commit",
        "Commit_sha",
    ] {
        assert!(!is_snake_case(bad), "should reject {bad:?}");
    }

    for good in ["OK", "RATE_LIMITED", "SHA256", "A1_B2"] {
        assert!(is_screaming_snake_case(good), "should accept {good}");
    }
    for bad in [
        "",
        "Ok",
        "RATE-LIMITED",
        "_RATE",
        "RATE_",
        "RATE__LIMITED",
        "1RATE",
    ] {
        assert!(!is_screaming_snake_case(bad), "should reject {bad:?}");
    }
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
                is_screaming_snake_case(text),
                "{name} has a non-SCREAMING_SNAKE_CASE value: {text}"
            );
        }
    }

    assert!(checked > 0, "no enum values found — the walk is broken");
}

#[test]
fn published_max_items_matches_the_enforced_bound() {
    // Two independent literals describing one limit: `MAX_LARGEST_FILES` in the
    // newtype, and `max_items` in the field attribute utoipa reads. utoipa
    // accepts the attribute only on fields, not on newtype structs, so they
    // cannot be written once — which makes them exactly the kind of pair that
    // drifts, publishing a bound the server does not enforce or enforcing one it
    // never published.
    let document: serde_json::Value = serde_json::from_str(&generate()).expect("valid JSON");
    let published = document["components"]["schemas"]["LineCountSummary"]["properties"]
        ["largest_files"]["maxItems"]
        .as_u64()
        .expect("largest_files publishes maxItems");

    assert_eq!(
        usize::try_from(published).expect("fits"),
        repolens_server::contract::report::MAX_LARGEST_FILES,
        "the published maxItems and the enforced bound disagree"
    );
}

#[test]
fn error_code_all_matches_the_generated_enum() {
    // `ErrorCode::ALL` is what every exhaustiveness gate iterates, and an
    // exhaustive `match` cannot prove it is *complete*: a new variant can be
    // added to the match and omitted from the array, leaving the fixture gate
    // green while covering less than it claims.
    //
    // The OpenAPI enum is generated by utoipa from the enum itself, so
    // comparing against it closes that gap — the document cannot be missing a
    // variant the type has.
    let document: serde_json::Value = serde_json::from_str(&generate()).expect("valid JSON");
    let generated: BTreeSet<String> = document["components"]["schemas"]["ErrorCode"]["enum"]
        .as_array()
        .expect("ErrorCode is an enum schema")
        .iter()
        .map(|v| v.as_str().expect("string variant").to_owned())
        .collect();

    let listed: BTreeSet<String> = repolens_server::contract::error::ErrorCode::ALL
        .iter()
        .map(|code| {
            serde_json::to_value(code)
                .expect("serializes")
                .as_str()
                .expect("string")
                .to_owned()
        })
        .collect();

    assert_eq!(
        listed, generated,
        "ErrorCode::ALL disagrees with the generated schema. Add the missing          variant to ALL — every exhaustiveness gate iterates it."
    );
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
