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

The intended split is that an HTTP request never performs analysis: the API
enqueues, the worker works. **That is not yet true.** `POST /api/v1/analyses`
writes the row, then runs the pipeline on a spawned task in the API process, so
an analysis in flight does not survive a restart or a revision rollout. Issue #7
replaces the spawn with a durable claim and the `worker` binary above becomes
the thing that runs it. Until then, treat "the worker works" as the design, not
as a description of `master`.

## Configuration

| Variable              | Required | Used by                                         |
| --------------------- | -------- | ----------------------------------------------- |
| `DATABASE_URL`        | yes      | API and ordinary worker transactions, pooled endpoint |
| `DATABASE_DIRECT_URL` | yes      | migrations and session-dependent administration, direct endpoint |
| `GH_ANALYSIS_TOKEN`   | no       | raises GitHub's rate-limit ceiling               |
| `FIREBASE_PROJECT_ID` | no       | verifying ID tokens on analysis creation         |

Both database URLs should carry `sslmode=verify-full`; `config.rs` warns at
startup when a non-local URL does not. `CORS_ALLOWED_ORIGIN` names one exact
origin or the layer is not applied at all.

`FIREBASE_PROJECT_ID` is the **public** project id — verification needs only
that and Google's published keys, so there is no service account to hold or
rotate. Absent, it **closes** creation: every `POST /api/v1/analyses` answers
`503 AUTHENTICATION_UNAVAILABLE`, because a forgotten variable must not leave an
anonymous, public, work-creating endpoint open. Reads stay anonymous either way.

`GH_ANALYSIS_TOKEN` is optional and its absence is not a failure. The client is
built either way, because only public repositories are analyzed, so the token
raises the request budget without widening what can be read — roughly sixty
requests an hour become five thousand. `AppState` therefore holds a
`GitHubRestClient`, never an `Option<GitHubRestClient>`: a state that can
represent "no GitHub access" invites a handler to answer
`REPOSITORY_INACCESSIBLE` without asking GitHub, which reports a failure for a
repository that is very likely readable. Whether a request succeeds is GitHub's
answer to give.

Logs carry neither credentials nor repository excerpts. `config.rs` deliberately
never echoes a value it rejected — a mistyped connection string or token would
otherwise be copied verbatim into a log line.

## Middleware order is a tested behaviour

The layer stack in `src/api/mod.rs` is ordered deliberately: body limit, timeout,
panic capture, metrics, tracing, CORS. Assert it by driving the real `Router`
through `oneshot` in `tests/api.rs` and `tests/middleware.rs`, never by reading
the builder — a reordering that sends a panic elsewhere first, or leaves it
uncounted, is invisible in review and obvious in a request. Metric labels come
from `MatchedPath`, never a URI; `tests/metrics.rs` asserts that on the registry.

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

Queries are written with runtime `sqlx::query`, **not** the `query!` macro, so
there is no `.sqlx/` offline cache and nothing to regenerate. The trade, and
what would have to change to earn one, is recorded in
[`migrations/README.md`](../../migrations/README.md).

Migrations are authored with `sqlx migrate add <name>`, are append-only once
deployed, and run over the direct endpoint via `cargo run --bin migrate`.

`tests/postgres.rs` runs against a real PostgreSQL and **panics when
`DATABASE_URL` is unset rather than skipping** — a suite that disappears with
its dependency reports green while proving nothing. CI provisions a
`postgres:17` service and applies `migrations/` with the `migrate` binary, so
the schema under test is never a fixture free to drift. Locally, export both
database URLs and run `cargo run --bin migrate` once.

It holds what cannot be checked without a database: that a terminal write
carries the canonical repository coordinate. Adopting that coordinate
mid-pipeline is best-effort, so the only durable guarantee is the one
`store::complete` and `store::fail` make in the statement that sets the terminal
state — a property of the SQL, not of any Rust value.
