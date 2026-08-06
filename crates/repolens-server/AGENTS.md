# `repolens-server`

> The nearest `AGENTS.md` governs this subtree. It extends the root
> [`AGENTS.md`](../../AGENTS.md) rather than replacing it, and loses to
> executable code, tests, generated contracts, and CI whenever prose is stale.

Everything that touches the world: the axum API and its wire DTOs, PostgreSQL
adapters, the durable worker, and the infrastructure adapters for archive
extraction and line counting.

## Layering

Handlers stay thin. A handler extracts, calls one application service, and maps
the result to a response; it does not open a transaction, call GitHub, or decide
a finding. Domain logic lives in `repolens-core`, retrieval in `repolens-github`,
and everything database- or process-shaped in this crate's infrastructure.

Row structs stay private to this crate and never carry `ToSchema`. The wire
types in `src/contract` are separate on purpose: if a row were the DTO, a column
rename would silently become a breaking API change and the schema gate would be
guarding the shape of the database instead of the shape of the contract.

## The three binaries

`src/bin/server.rs`, `src/bin/worker.rs`, and `src/bin/migrate.rs` compile into
one image; Cloud Run overrides the entrypoint per resource. There is no argument
parser and no loop mode — `worker` claims one analysis, runs it, persists, and
exits. A worker that can also be a daemon has two sets of shutdown and
lease-renewal semantics, and the second would only ever run on a developer's
machine, the worst place to discover a lease bug.

Because an execution can be killed by a job timeout or by memory exhaustion
during extraction, **abandoned-lease recovery is mandatory and every worker
effect is idempotent.** Re-running a claimed step must not duplicate a row, a
trigger, or a report.

An HTTP request never performs analysis. The API enqueues; the worker works.

## Configuration

| Variable              | Endpoint | Used by                                         |
| --------------------- | -------- | ----------------------------------------------- |
| `DATABASE_URL`        | pooled   | API and ordinary worker transactions            |
| `DATABASE_DIRECT_URL` | direct   | migrations and session-dependent administration |

Both should carry `sslmode=verify-full`; `config.rs` warns at startup when a
non-local URL does not. `CORS_ALLOWED_ORIGIN` names one exact origin or the
layer is not applied at all.

Logs carry neither credentials nor repository excerpts. `config.rs` deliberately
never echoes a value it rejected — a mistyped connection string or token would
otherwise be copied verbatim into a log line.

## Middleware order is a tested behaviour

The layer stack in `src/api.rs` is ordered deliberately: body limit, timeout,
panic capture, tracing, CORS. Assert it by driving the real `Router` through
`tower::ServiceExt::oneshot` in `tests/api.rs`, never by reading the builder —
a reordering that changes which layer sees a panic first is invisible in review
and obvious in a request.

`tests/openapi.rs` regenerates `contracts/openapi.json` from the live router, so
a route added without a `utoipa::path` annotation cannot be served.

## Commands

```sh
cargo clippy -p repolens-server --all-targets
cargo test -p repolens-server
cargo test -p repolens-server --test api
cargo run --bin server
cargo run --bin migrate
```

After changing a `sqlx::query!` call, refresh the committed offline cache with
`cargo sqlx prepare --workspace` or the container and CI builds fail with no
live database to verify against. Migrations are authored with
`sqlx migrate add <name>`, are append-only once deployed, and run over the
direct endpoint.

There are no PostgreSQL integration tests yet, and CI provisions no database.
The first ones must fail loudly when `DATABASE_URL` is unset rather than skip
into a green run, and must arrive together with the CI service that runs them —
a suite nothing executes is a suite nobody maintains.
