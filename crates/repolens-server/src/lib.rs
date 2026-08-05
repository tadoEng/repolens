//! RepoLens application and infrastructure layer.
//!
//! Everything that touches the outside world lives here: the HTTP surface,
//! PostgreSQL adapters, the durable worker, Firebase verification, the Cloud
//! Run Jobs execution trigger, and — per plan §4.4 — the hardened archive
//! extraction and Tokei adapter that implement
//! [`repolens_core::composition::RepositoryCompositionCounter`].
//!
//! Two rules this crate exists to keep out of the others:
//!
//! * database row structs are never public API DTOs, and are never exposed
//!   through `utoipa`;
//! * `anyhow` is for `main`, startup, migration, and worker context only. It is
//!   never returned from a domain or application interface, which use typed
//!   `thiserror` errors.

pub mod api;
pub mod config;
pub mod telemetry;
