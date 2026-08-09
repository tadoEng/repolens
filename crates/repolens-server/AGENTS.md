# `repolens-server`

> The nearest `AGENTS.md` governs this subtree. It extends the root
> [`AGENTS.md`](../../AGENTS.md) rather than replacing it, and loses to
> executable code, tests, generated contracts, and CI whenever prose is stale.

Everything that touches the world: the axum API and its wire DTOs, PostgreSQL
adapters, the durable worker, and the adapters for extraction and line counting.

## Layering

Handlers stay thin. A handler extracts, calls one application service, and maps
the result to a response; it does not open a transaction, call GitHub, or decide
a finding. Domain logic lives in `repolens-core`, retrieval in `repolens-github`,
everything database- or process-shaped in this crate's infrastructure.

Row structs stay private to this crate and never carry `ToSchema`. If a row were
the DTO, a column rename would silently become a breaking API change and the
schema gate would guard the shape of the database instead of the contract.

## The three binaries

`src/bin/server.rs`, `src/bin/worker.rs`, and `src/bin/migrate.rs` compile into
one image, entrypoint chosen per resource — a Cloud Run design, and Cloud Run is
not the host. No argument parser and no loop mode: `worker` claims one analysis,
runs it, persists, and exits. A worker that can also be a daemon has two sets of
shutdown and lease-renewal semantics, and the second would run only on a
developer's machine — the worst place to discover a lease bug.

Because an execution can be killed by a job timeout or by memory exhaustion
during extraction, **abandoned-lease recovery is mandatory and every worker
effect is idempotent.** Re-running a claimed step must not duplicate a row, a
trigger, or a report.

The intended split is that an HTTP request never performs analysis: the API
enqueues, the worker works. **That is not yet true.** `POST /api/v1/analyses`
writes the row, then runs the pipeline on a spawned task in the API process, so
an analysis in flight does not survive a restart. Issue #7 replaces the spawn
with a durable claim. Until then, treat "the worker works" as the design, not as
a description of `master`.

## Configuration

| Variable              | Required | Used by                                         |
| --------------------- | -------- | ----------------------------------------------- |
| `DATABASE_URL`        | yes      | API and ordinary worker transactions, pooled endpoint |
| `DATABASE_DIRECT_URL` | yes      | migrations and session-dependent administration, direct endpoint |
| `GH_ANALYSIS_TOKEN`   | no       | raises GitHub's rate-limit ceiling               |
| `FIREBASE_PROJECT_ID` | no       | verifying ID tokens on analysis creation and on the admin route |
| `ADMIN_FIREBASE_UIDS` | no       | who may read `GET /api/v1/admin/overview`; empty means nobody |

Both database URLs should carry `sslmode=verify-full`; `config.rs` warns at
startup when a non-local URL does not. `CORS_ALLOWED_ORIGIN` names one exact
origin, or no CORS layer is applied at all.

`FIREBASE_PROJECT_ID` is the **public** project id — verification needs only that
and Google's published keys, so there is no service account to rotate. Absent, it
**closes** creation: `POST /api/v1/analyses` answers `503` with
`AUTHENTICATION_UNAVAILABLE`, because a forgotten variable must not leave an
anonymous, work-creating endpoint open. Reads stay anonymous either way.

`ADMIN_FIREBASE_UIDS` answers the other question: the project id decides whether
an identity can be established, this decides whether that identity may read
`GET /api/v1/admin/overview`. The `Admin` extractor asks both in that order, so
the refusals stay distinct — `401` no usable credential, `403` verified but not
allow-listed, `503` nothing verifiable at all. Absent or empty it **closes** the
endpoint. UIDs not emails, compared exactly, never logged: the list names people.

`GH_ANALYSIS_TOKEN` is optional and its absence is not a failure: only public
repositories are analyzed, so it raises the request budget without widening what
can be read. `AppState` therefore holds a `GitHubRestClient`, never an
`Option<GitHubRestClient>` — a state that can represent "no GitHub access"
invites a handler to answer `REPOSITORY_INACCESSIBLE` without asking GitHub, and
whether a request succeeds is GitHub's answer to give.

Logs carry neither credentials nor repository excerpts, and `config.rs` never
echoes a value it rejected — a mistyped connection string would otherwise be
copied verbatim into a log line.

## Middleware order is a tested behaviour

The layer stack in `src/api/mod.rs` is ordered deliberately: body limit, timeout,
panic capture, metrics, tracing, CORS. Assert it by driving the real `Router`
through `oneshot` in `tests/api.rs` and `tests/middleware.rs`, never by reading
the builder — a reordering that leaves a panic uncounted is invisible in review
and obvious in a request. Metric labels come from `MatchedPath`, never a URI.

The same rule covers the rest: `tests/openapi.rs` regenerates
`contracts/openapi.json` from the live router, so a route without a
`utoipa::path` annotation cannot be served, and `tests/admin.rs` asserts
`401`/`403`/`200` plus the response bytes against planted canary secrets.

## Commands

```sh
cargo clippy -p repolens-server --all-targets
cargo test -p repolens-server
cargo run --bin server
cargo run --bin migrate
```

Queries are written with runtime `sqlx::query`, **not** the `query!` macro, so
there is no `.sqlx/` offline cache and nothing to regenerate; the trade, and what
would have to change to earn one, is in
[`migrations/README.md`](../../migrations/README.md). Migrations are authored with
`sqlx migrate add <name>`, are append-only once deployed, and run over the direct
endpoint via `cargo run --bin migrate`.

`tests/postgres.rs` runs against a real PostgreSQL and **panics when
`DATABASE_URL` is unset rather than skipping** — a suite that disappears with its
dependency reports green while proving nothing. CI provisions `postgres:17` and
applies `migrations/` with the `migrate` binary, so the schema under test is
never a fixture free to drift. Locally, export both database URLs and run
`cargo run --bin migrate` once.

It holds what cannot be checked without a database: that a terminal write carries
the canonical repository coordinate. Adopting it mid-pipeline is best-effort, so
the only durable guarantee is the one `store::complete` and `store::fail` make in
the statement setting the terminal state — SQL, not any Rust value.
