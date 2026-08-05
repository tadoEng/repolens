//! The `analysis-v1` public wire contract.
//!
//! These types exist to be *generated from*, not merely described. The chain is
//! one-directional and gated at both ends:
//!
//! ```text
//! these types → utoipa → contracts/openapi.json → openapi-typescript
//!            → schema.ts → fixtures type-check → MSW → SvelteKit
//! ```
//!
//! # Why the endpoints are not here
//!
//! Issue #14 owns the *contract*; issue #6 owns the routes that serve it. The
//! schemas are registered on the OpenAPI document directly rather than being
//! collected from paths, so the generated TypeScript client and the executable
//! fixtures exist before any endpoint does. That is what unblocks frontend work
//! without inviting anyone to invent a DTO in Svelte.
//!
//! # Conventions
//!
//! Object fields are `snake_case`, which is what Rust produces already, so no
//! struct carries `rename_all` — an attribute required on every DTO is one that
//! will eventually be forgotten on one. Enum values are
//! `SCREAMING_SNAKE_CASE`, which `PascalCase` variants cannot produce, so there
//! the rename is unavoidable and applied once per enum.
//!
//! Nullable-but-required fields (`commit_sha`, `composition`) use
//! `#[schema(required)]`. utoipa treats `Option<T>` as optional by default,
//! which would generate `field?: T | null` and let a consumer skip the null
//! case entirely — and the null case is the one that matters.

pub mod analysis;
pub mod error;
pub mod report;

/// How a client must treat an enum value it does not recognise.
///
/// A statically hosted frontend outlives the build it was compiled against: the
/// API can gain a `FindingState` or an `ErrorCode` months after a browser
/// cached the bundle. Three rules, in order of importance:
///
/// 1. **Never crash.** An unknown variant is data, not a bug in the client.
/// 2. **Never silently drop it.** Discarding an unrecognised finding hides
///    exactly the information the report exists to surface, and does so
///    invisibly. Render it in a neutral fallback that names the raw value.
/// 3. **Fail the build, not the browser.** A contract test compares the
///    variants in the generated schema against the set the frontend handles, so
///    adding one without handling it breaks CI rather than production.
///
/// Rule 3 is what makes rules 1 and 2 a safety net rather than a plan.
pub const UNKNOWN_VARIANT_POLICY: &str = "render unknown enum values in a neutral fallback that names the raw value; \
     never crash, never drop silently; a contract test fails CI when a variant \
     is added without frontend handling";
