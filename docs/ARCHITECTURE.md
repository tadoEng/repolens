# RepoLens architecture

RepoLens analyzes one public GitHub repository at an exact commit SHA and
produces a deterministic, evidence-backed architecture report.

Two properties drive every structural decision below, and both are easy to lose
by accident:

- **Determinism.** The same inputs must produce the same report, and when they
  do not, the report must say which input changed.
- **Honesty about limits.** "We could not see everything" has to be
  representable and visible. A truncated tree or an abandoned line count is a
  result, not an error to be swallowed.

## The three crates

```text
repolens-core  ◄── repolens-github  ◄── repolens-server
      ▲                                        │
      └────────────────────────────────────────┘

Arrows point from dependent to dependency. There are no other edges,
and no cycles.
```

### `crates/repolens-core` — the domain

Repository identity, findings, evidence, severity and confidence, the analyzer
rule contract, the reproducibility key, and the normalized composition result
plus its `RepositoryCompositionCounter` trait.

It answers *what may this report claim?* and nothing else. It has no HTTP
client, no database, and no async runtime.

### `crates/repolens-github` — the ingestion boundary

`GitHubRepositorySource`, with five operations and a hard split between them:

| Operation               | Role                                        |
| ----------------------- | ------------------------------------------- |
| `resolve_repository`    | canonical identity                          |
| `resolve_commit`        | canonical identity                          |
| `fetch_tree`            | canonical evidence                          |
| `fetch_selected_blobs`  | canonical evidence                          |
| `download_archive`      | ephemeral transport for line counting only  |

Ingestion is REST rather than GraphQL because the subject is the immutable Git
object graph, not GitHub's social graph — and because the recursive tree
endpoint reports `truncated: true` explicitly, which makes bounded traversal
honest.

`GITHUB_REST_API_VERSION` is sent on every request and is exact-pinned. Omitting
the header does not mean "latest": GitHub defaults an absent header to an older
version, so relying on the default would silently pin us to something we never
tested.

### `crates/repolens-server` — everything that touches the world

The axum API and its DTOs, PostgreSQL adapters, the durable worker, Firebase ID
token verification, the Cloud Run Jobs execution trigger, and
`infrastructure::composition` — hardened archive extraction, the Tokei adapter,
and the exclusion policy.

## Forbidden dependency directions

These are not style preferences. Each one, if violated, breaks a property the
product is built to demonstrate.

### `repolens-core` depends on nothing infrastructural

No `axum`, no `sqlx`, no `reqwest`, no async runtime. The crate's dependency
list is the enforcement mechanism; there is nothing else to remember.

*Why:* a domain type that can reach a socket is no longer a function of the
evidence it was given, and a report assembled from such types cannot be claimed
to be reproducible.

### Analyzer rules never depend on `axum` or `sqlx`

A rule receives evidence and returns findings. It performs no I/O, reads no
clock, and consults no configuration it was not handed.

*Why:* rules are the part of the system whose output must be identical across
runs. A rule that can query the database can produce a different report on
Tuesday for reasons no reader can see.

### Database rows are never public API DTOs

Row structs stay inside `repolens-server` and are never exposed through
`utoipa`. Wire DTOs are separate types, and the OpenAPI document describes only
those.

*Why:* the frontend consumes a generated TypeScript client. If a row struct were
the DTO, a column rename would silently become a breaking API change, and the
schema snapshot gate would be enforcing the shape of the database rather than
the shape of the contract.

### Tokei lives in `repolens-server::infrastructure::composition`, not in `repolens-github`

Downloading an archive is GitHub-specific. *Counting lines* is not.

```text
repolens-core     RepositoryCompositionCounter + normalized composition result
repolens-github   resolve repository · resolve commit · fetch tree/blobs · stream archive
repolens-server   infrastructure::composition — hardened extraction · Tokei adapter ·
                  exclusion policy
```

*Why:* binding the counter to the GitHub adapter would make a future
local-folder or uploaded-archive analyzer conceptually depend on GitHub. The
analyzer depends on *counts*; it must never depend on Tokei.

Related: GitHub's language endpoint reports **bytes, not lines**, so it cannot
answer this question at all. And the archive is never canonical evidence — its
bytes are not guaranteed stable over time even for a fixed commit, so hashing
the tarball would break reproducibility rather than establish it. What is
persisted is the commit SHA, the Tokei version, the exclusion-policy version,
the counted-file manifest, and the normalized result.

## The three runtime roles

Three binaries, one container image. Cloud Run overrides the entrypoint per
resource, so the Service and the Job share a single build and a single Artifact
Registry repository.

| Binary    | Role                                                                                                                       |
| --------- | -------------------------------------------------------------------------------------------------------------------------- |
| `server`  | Serves the HTTP API. Long-running; drains on `SIGTERM`. Never performs analysis inside a request.                            |
| `worker`  | Claims one queued analysis, runs it, persists the result, exits. **Always run-once.**                                        |
| `migrate` | Applies migrations compiled in via `sqlx::migrate!`, over the direct database endpoint. Runs to completion and exits.         |

There is no argument parser. A Cloud Run Job starts one execution and exits, so
`worker` needs no `--once` flag, and with no flag to parse there is no reason to
depend on a CLI crate. There is deliberately no loop mode either, not even
behind an environment variable: a worker that can be a daemon under some
configuration has two sets of shutdown and lease-renewal semantics, and the
second would only ever be exercised on a developer's machine — the worst place
to discover a lease bug. Local iteration repeats the *process*, which is the
code path Cloud Run actually runs:

```sh
while cargo run --bin worker; do sleep 5; done
```

An always-on polling worker would require `min-instances >= 1`, which bills
continuously and would falsify the free-stack thesis RepoLens exists to test.
Only the *activation trigger* changes: the durable state machine —
`SELECT ... FOR UPDATE SKIP LOCKED` claims, explicit leases, abandoned-lease
recovery, bounded retries, idempotent effects — is unchanged, and lease recovery
matters *more* under Jobs, since an execution can be killed by a job timeout or
by memory exhaustion during extraction, stranding a lease.

Two database endpoints, not one:

| Variable              | Endpoint | Used by                                            |
| --------------------- | -------- | -------------------------------------------------- |
| `DATABASE_URL`        | pooled   | API and ordinary worker transactions               |
| `DATABASE_DIRECT_URL` | direct   | migrations and session-dependent administration    |

## The contract pipeline

The frontend never sees a hand-written description of this API. One chain, with
a gate at each end:

```text
Axum routes + DTOs
      ↓  utoipa, collected by OpenApiRouter from routes that actually exist
contracts/openapi.json          ← committed
      ↓  openapi-typescript
packages/repolens-api-client/src/schema.ts   ← committed, generated
      ↓  openapi-fetch
web/  (SvelteKit)
```

A route cannot be served without being documented, because the router and the
document are produced by the same call and there is no way to obtain one
without the other. Both committed artefacts are generated, so both can go
stale; two gates make that a build failure rather than a runtime surprise:

| Gate                                            | Catches                                        |
| ----------------------------------------------- | ---------------------------------------------- |
| `cargo test -p repolens-server --test openapi`  | `contracts/openapi.json` no longer matches the routes |
| `pnpm --filter @repolens/api-client test`       | `schema.ts` no longer matches the document     |

CI additionally asserts the working tree is clean, which catches a regeneration
that was run but never committed.

After deliberately changing a route or DTO, regenerate both — in order, because
the second reads the first's output:

```sh
UPDATE_OPENAPI=1 cargo test -p repolens-server --test openapi
pnpm --filter @repolens/api-client schema:update
```

Naming is settled (issue #14): **object fields are `snake_case`**, which is what
Rust produces already, so no `rename_all` is used on structs — an attribute that
must be repeated on every DTO is an attribute that will eventually be forgotten
on one. **Enum values are `SCREAMING_SNAKE_CASE`**, which Rust's `PascalCase`
variants cannot produce, so there `rename_all` is unavoidable and is applied
once per enum.

### The system probe

`GET /api/v1/system/probe` reports the whole hosting path in one response:

```json
{ "api": "OK", "database": "OK", "build_sha": "abc1234", "schema_version": 1 }
```

It answers `200` even when a dependency is down, because failing the request
would make "the API is up but the database is not" indistinguishable from "the
API is down" — the exact distinction the endpoint exists to draw. Dependency
health is data, not a status code.

Two facts it refuses to conflate:

- `database` separates `UNAVAILABLE` (unreachable) from `DEGRADED` (reachable,
  but migrations have never been applied). One query against `_sqlx_migrations`
  could not tell them apart: a missing table would report an empty database as
  unreachable.
- `schema_version` is **nullable, never zero-by-default**. "No migrations have
  been applied" and "we could not find out" are different facts, and collapsing
  them would let a connection failure read as an empty database.

## The reproducibility key

A report is reproducible with respect to four values, all of which it carries:

```text
commit SHA + analyzer version + ruleset version + exclusion-policy version
```

Two runs are expected to agree only when all four match. Any of them changing is
a legitimate reason for the report to differ — which is precisely why they are
published: without them, a reader cannot tell "the repository changed" from
"RepoLens changed".

These four are the reason the version-pinning policy is split. Ordinary
dependencies use normal compatible requirements (`axum = "0.8"`), because
reproducibility already comes from `Cargo.lock`, `rust-toolchain.toml`, and the
container base-image digest, and blanket exact pins buy nothing but maintenance
brittleness. **Exact pins are reserved for what changes deterministic report
output**: the Tokei version, the analyzer version, the ruleset version, the
exclusion-policy version, the GitHub REST API version, and the container
base-image digest.

## What is deliberately not here yet

The plan's rule — no abstraction before real code requires it — applies to this
document too.

- **No analysis or report DTOs.** Their shapes are owned by executable fixtures
  under `contracts/fixtures/` (issue #14) so that a drifting contract fails CI
  rather than a specification document. Types in `repolens-core` marked
  **PROVISIONAL** exist to express a boundary, not to fix a wire format.
- **No `/api/v1/system/probe`.** The walking skeleton (issue #11) owns it,
  together with the database connectivity it reports on. `GET /healthz` answers
  only for the process, which is all this binary can currently support.
- **No CORS layer.** The allowed origin is a deployed Cloudflare domain that
  does not exist yet, and a permissive default would be a security decision made
  by omission.
- **No fourth crate.** Extraction, the Tokei adapter, the worker, and auth stay
  inside `repolens-server` until real code justifies splitting them.
