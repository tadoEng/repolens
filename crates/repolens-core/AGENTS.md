# `repolens-core`

> The nearest `AGENTS.md` governs this subtree. It extends the root
> [`AGENTS.md`](../../AGENTS.md) rather than replacing it, and loses to
> executable code, tests, generated contracts, and CI whenever prose is stale.

The domain, and only the domain: repository identity, findings, evidence,
severity and confidence, the analyzer rule contract, the reproducibility key,
and the normalized composition result. It answers *what may this report claim?*

## Forbidden here

`axum`, `sqlx`, `reqwest`, any async runtime, the filesystem, the network, an
ambient clock, and ambient configuration. `Cargo.toml` is the enforcement — if a
dependency you want needs a socket or a connection pool, it belongs in another
crate.

A domain type that can reach a socket is no longer a function of the evidence it
was given, and a report assembled from such types cannot be claimed reproducible.

## Rules take everything explicitly

`AnalyzerRule` receives a `RuleInput` and returns findings. Every version that
can change the output — analyzer, ruleset, counter, exclusion policy — arrives
through arguments, never through a global or a lookup. A rule that can consult
anything ambient can produce a different report on Tuesday for reasons no reader
can see.

Unavailable evidence is represented, never defaulted. "No line count was
obtainable" is `None` plus a limitation, not an empty map; an empty finding set
without a limitation asserts that nothing was found, which is a different and
much stronger claim.

## This crate does not own the wire format

Doc comments marked **PROVISIONAL** express a boundary, not a serialization
shape. The public contract lives in `crates/repolens-server/src/contract` and
[`contracts/`](../../contracts/AGENTS.md). Do not add `utoipa` here, do not
reshape a type to make a JSON payload convenient, and do not treat a
`serde` derive in this crate as a promise to any client.

## Commands

```sh
cargo fmt -p repolens-core -- --check
cargo clippy -p repolens-core --all-targets
cargo test -p repolens-core
cargo doc -p repolens-core --no-deps
cargo tree -p repolens-core --edges normal
```

The last one is the boundary check: read the output before adding a dependency,
and the crate should stay a short list of `serde`, `thiserror`, and `uuid`.
