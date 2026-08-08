# RepoLens architecture

RepoLens analyzes one public GitHub repository at an exact commit SHA and
produces a deterministic, evidence-backed architecture report.

Two properties drive every structural decision below, and both are easy to lose
by accident:

- **Determinism.** The same inputs must produce the same report, and when they
  do not, the report must say which input changed.

  Bounded precisely, because the unbounded version is false: two runs receive
  different `analysis_id`s and different `completed_at` timestamps, so complete
  wire reports are never byte-identical. What must not vary is everything
  derived from the reproducibility key, which `Report::analytical_payload`
  defines by removing exactly those two execution-metadata fields. The claim
  lives in code rather than in this sentence, so it cannot quietly outgrow what
  holds.
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
token verification, `infrastructure::composition` — hardened archive extraction,
the Tokei adapter, and the exclusion policy — and the trigger that starts a
worker, which was designed as a Cloud Run Job execution and is an open question
again since the move off Cloud Run.

That list is the crate's charter, not an inventory of its contents. The API, the
`analysis-v1` DTOs, the three binaries, Firebase ID token verification, and
`infrastructure::composition` exist today; the rest arrives with the issue that
needs it, and belongs here rather than in a fourth crate when it does.

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

Three binaries, one container image, with the entrypoint chosen per resource.

**Cloud Run is the platform this split was designed against. Render is where the
API is deployed, and Cloud Run is not coming back.** Under Cloud Run the
per-resource entrypoint was what let a Service and a Job share one build and one
Artifact Registry repository; that argument is history, and the shape it bought
— one image, three roles — is not.

The reason is the thesis itself: Google Cloud requires a billing account with a
card before anything runs, including inside its free tier. RepoLens exists partly
to answer whether this stack runs on genuinely free infrastructure, so a card
requirement disqualifies the platform rather than inconveniencing it. Render's
free tier needs no card. Every Cloud Run reference that remains — below, and in
the crate documentation, where
[`crates/repolens-server/src/contract/analysis.rs`](../crates/repolens-server/src/contract/analysis.rs)
still explains `ExecutionMetadata` in terms of a Cloud Run Job execution —
describes the *design's* original target and not the host;
[`DEPLOYMENT.md`](DEPLOYMENT.md) records the deployment as it actually is.

Most of the split survives the move unchanged — three roles, one image, an
entrypoint chosen per resource is a property of the image, not of Cloud Run. Two
things do not, and both belong to #9 and #7 rather than to this section:

- **A free Render web service spins down when idle** and takes tens of seconds to
  wake. Cold starts stop being a number to measure and become something a reader
  of a shared report URL experiences.
- **`worker` may have nowhere free to run.** Cloud Run Jobs was the mechanism:
  one execution per analysis, started on demand, billed only while running.
  Render's equivalents — Background Workers and Cron Jobs — are, as far as this
  has been checked, paid-only. If that holds, #7 cannot be "a Job claims a
  lease"; it has to be "the web service claims a lease", which changes the
  shutdown and lease-renewal story the run-once design was chosen to avoid. A
  third option is on #7 — an external free scheduler calling the API to run one
  claim cycle — which would keep the split. **None of the three is settled, and
  the Render pricing question is unverified: that is exactly what #9 is for.**
  It is written down here because discovering it while implementing #7 would be
  discovering it too late.

| Binary    | Role                                                                                                                       |
| --------- | -------------------------------------------------------------------------------------------------------------------------- |
| `server`  | Serves the HTTP API. Long-running; drains on `SIGTERM`. Never performs analysis inside a request.                            |
| `worker`  | Claims one queued analysis, runs it, persists the result, exits. **Always run-once.**                                        |
| `migrate` | Applies migrations compiled in via `sqlx::migrate!`, over the direct database endpoint. Runs to completion and exits.         |

There is no argument parser. The design assumed a runner that starts one
execution and exits — a Cloud Run Job — so `worker` needs no `--once` flag, and
with no flag to parse there is no reason to depend on a CLI crate. What starts
that execution now is open (#9); that it runs once is the part the lease design
rests on, and it has not moved. There is deliberately no loop mode either, not
even behind an environment variable: a worker that can be a daemon under some
configuration has two sets of shutdown and lease-renewal semantics, and the
second would only ever be exercised on a developer's machine — the worst place
to discover a lease bug. Local iteration repeats the *process*, which is the
code path a run-once worker takes:

```sh
while cargo run --bin worker; do sleep 5; done
```

An always-on polling worker would have needed Cloud Run's `min-instances >= 1`,
which bills continuously and would falsify the free-stack thesis RepoLens exists
to test — and the objection carries to any host that charges for an idle
process. Only the *activation trigger* changes: the durable state machine —
`SELECT ... FOR UPDATE SKIP LOCKED` claims, explicit leases, abandoned-lease
recovery, bounded retries, idempotent effects — is unchanged, and lease recovery
matters *more* rather than less, since whatever runs the worker can be killed by
a timeout or by memory exhaustion during extraction, stranding a lease.

Two database endpoints, not one:

| Variable              | Endpoint | Used by                                            |
| --------------------- | -------- | -------------------------------------------------- |
| `DATABASE_URL`        | pooled   | API and ordinary worker transactions               |
| `DATABASE_DIRECT_URL` | direct   | migrations and session-dependent administration    |

Measured against a real Neon project, not assumed:

- **SQLx works over Neon's pooled endpoint.** The plan flagged this as an open
  question, since PgBouncer in transaction mode historically broke prepared
  statements. It does not here, and `statement_cache_capacity=0` is not needed.
- **Neon's default connection string does not guarantee hostname verification.**
  It carries `sslmode=require` and `channel_binding=require`. With `sqlx`'s
  native roots, `require` may validate the certificate chain like `verify-ca`;
  only `verify-full` additionally guarantees hostname identity. `sqlx` does not
  implement the supplied `channel_binding` parameter, so remove it rather than
  leaving it to imply a protection the client does not provide.
  `sslmode=verify-full` is confirmed to work against Neon, and the server warns
  at startup when a non-local URL lacks it.
- **A suspended compute costs seconds on the first connection.** Neon
  scale-to-zero suspends after idle, and that resume stacks on top of whatever
  cold start the API host has — a free Render service spins down too, per
  [`DEPLOYMENT.md`](DEPLOYMENT.md) — so the first request after quiet pays both.
  The progress UI has to tolerate it without looking broken.

## The contract pipeline

The frontend never sees a hand-written description of this API. One source, two
branches, and a gate on every committed artefact:

```text
Axum routes + DTOs   (crates/repolens-server/src/contract)
   │
   ├── utoipa, collected by OpenApiRouter from routes that actually exist
   │      ↓
   │   contracts/openapi.json                       ← committed, generated
   │      ↓  openapi-typescript
   │   packages/repolens-api-client/src/schema.ts   ← committed, generated
   │      ↓  openapi-fetch
   │   web/  (SvelteKit)
   │
   └── serde, serialized by the fixture test from the same types
          ↓
       contracts/fixtures/analysis-v1/*.json        ← committed, generated
          ↓  emitted as `satisfies` literals
       packages/repolens-api-client/src/fixtures.ts ← committed, generated
          ↓
       @repolens/msw handlers  →  browser and end-to-end tests
```

A route cannot be served without being documented, because the router and the
document are produced by the same call and there is no way to obtain one
without the other. Every committed artefact on that chain is generated, so
every one of them can go stale; four gates make that a build failure rather
than a runtime surprise:

| Gate                                            | Catches                                        |
| ----------------------------------------------- | ---------------------------------------------- |
| `cargo test -p repolens-server --test openapi`  | `contracts/openapi.json` no longer matches the routes |
| `cargo test -p repolens-server --test fixtures` | a fixture no longer matches the DTOs it was written from |
| `pnpm --filter @repolens/api-client test`       | `schema.ts` or `fixtures.ts` no longer matches its input |
| `pnpm -r check`                                 | a fixture shape the frontend cannot consume    |

CI additionally asserts that `contracts/openapi.json` still matches the
committed copy, which catches a regeneration that was run but never committed.

Nothing on either branch is edited by hand. After deliberately changing a route
or a DTO, regenerate in order — each step reads the previous step's output:

```sh
UPDATE_OPENAPI=1 cargo test -p repolens-server --test openapi
UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures
pnpm --filter @repolens/api-client schema:update
pnpm --filter @repolens/api-client fixtures:update
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

A report is reproducible with respect to every value below. Five of the eight
are published today; the other three are not, and the section after the list
says which is which, because the difference is the whole point of having a key.

```text
repository coordinate
commit SHA
root tree SHA
evidence source API + version
analyzer version
ruleset version
composition counter + version   (nullable — absent when nothing was counted)
exclusion-policy version
```

Two runs are expected to agree only when all of them match. Any one changing is
a legitimate reason for the report to differ, which is why the point of the key
is to be *published*: a reader who cannot see these values cannot tell "the
repository changed" from "RepoLens changed".

The membership test is narrow: **does changing this value change the report?**
The repository coordinate is included because two repositories can share a
commit SHA — a fork, or a commit present in both — and they are not the same
analysis. The root tree SHA is included because it is what the collectors
actually walked; commit metadata such as author and message affects no finding.
The archive hash is deliberately **excluded**: GitHub does not guarantee stable
archive bytes for a fixed commit, so keying on it would break reproducibility
rather than establish it.

`TreeSha` is a distinct type from `CommitSha` even though both are 40-character
hex digests, so that transposing them cannot compile.

### What a report carries today, and what it does not

Five of the eight. `Report` in
[`crates/repolens-server/src/contract/report.rs`](../crates/repolens-server/src/contract/report.rs)
publishes the repository coordinate, `commit_sha`, `tree_sha`,
`analyzer_version` and `ruleset_version`, and
[`web/src/lib/components/report/ReportHeader.svelte`](../web/src/lib/components/report/ReportHeader.svelte)
renders all five. That is the whole of what a reader can use to tell two reports
apart — and one of the five has never moved: `ANALYZER_VERSION` is
`env!("CARGO_PKG_VERSION")`, and the workspace version has been `0.1.0` since
the workspace was created, while `RULESET_VERSION` has gone 1 → 4. A change to
report assembly therefore changes no version anyone can see.

The other three are absent for two different reasons, and the difference
matters:

- **Evidence source API + version.** `GITHUB_REST_API_VERSION` in
  [`crates/repolens-github/src/lib.rs`](../crates/repolens-github/src/lib.rs) is
  sent on every request and published in no response. There is no field for it.
- **Composition counter + version, and exclusion-policy version.** These *are*
  in the contract, on `LineCountSummary`, but they hang off `composition`, and
  `build_report` in
  [`crates/repolens-server/src/pipeline.rs`](../crates/repolens-server/src/pipeline.rs)
  sets `composition: None` on every report it builds — line counting is not
  wired into the pipeline yet (#12). The fields exist and have never carried a
  value.

`ReproducibilityKey` in
[`crates/repolens-core/src/reproducibility.rs`](../crates/repolens-core/src/reproducibility.rs)
is the type that would state all eight at once. It is exported from that crate's
`lib.rs` and constructed nowhere outside its own unit tests. The definition is
not wrong — it is the ambition written down early, and it is unwired.

So the honest form of the determinism claim right now is: two reports of the
same repository and commit, under the same ruleset, are expected to agree, and a
reader can check those inputs. A disagreement traceable only to a GitHub API
version bump or a counter change is currently invisible, and would present as
determinism failing rather than as an input having moved. Publishing the key is
scoped to #28, whose design comment puts it first on the grounds that it needs
no database change. That same comment names a version the list above is missing
outright — the file-selection policy in
[`crates/repolens-github/src/policy.rs`](../crates/repolens-github/src/policy.rs),
which decides which files are read and carries no version at all, so raising a
selection limit changes a report while leaving every key component equal.

These values are the reason the version-pinning policy is split. Ordinary
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

- **No durable execution.** The analysis and report endpoints now exist —
  `POST /api/v1/analyses`, `GET /api/v1/analyses/{analysis_id}`, and
  `GET /api/v1/analyses/{analysis_id}/report` — and the seed ruleset runs
  against a real repository at an exact commit. **What does not exist is the
  worker claiming that work.** The create handler writes the row and then runs
  the pipeline on a `tokio::spawn`ed task inside the API process, which
  contradicts the run-once worker described above and the rule that an HTTP
  request never performs analysis.

  Stated plainly because it is the kind of gap that is discovered rather than
  read: **an analysis dies with the process.** A deploy or a revision rollout
  mid-run leaves a row in `ANALYZING` that nothing will ever move — there is no
  lease to expire and no recovery to reclaim it. The `worker` binary compiles
  and does nothing. Issue #7 replaces the spawn with a PostgreSQL claim
  (`FOR UPDATE SKIP LOCKED`, explicit leases, abandoned-lease recovery, bounded
  retries, idempotent effects), and until it lands the three-role split above is
  the design rather than a description of `master`.

  Types in `repolens-core` marked **PROVISIONAL** still express a boundary
  rather than a wire format — the wire format is owned by `contracts/`, and a
  `serde` derive in the domain crate promises nothing to a client.
- **No idempotent reuse or evidence cache.** Every equivalent `POST` mints a
  fresh analysis id, a fresh row, and a fresh GitHub traversal. There is no
  reuse of an existing analysis for the same commit, no immutable-evidence
  cache, no eviction policy, and no hit/miss observability, so an unchanged
  repeat costs the same request budget as the first run. Issue #6 stays open for
  that work, which lands with #7 rather than here.
- **No deployed verification.** `/api/v1/system/probe` and its database
  reporting now exist and are confirmed against a real Neon project, but only
  from a local process. `wrangler.jsonc` declares the assets-only Cloudflare
  configuration (issue #11), and that is a written intention rather than an
  observation: static hosting, `not_found_handling`, the CSP `connect-src`
  allowlist against a deployed origin, and cold-start behaviour stay unproven
  until something is actually deployed. `vite preview` is not Cloudflare and
  cannot stand in for it.
- **No account model behind the one authenticated route.** Creating an analysis
  requires a verified Firebase ID token; reading progress and reading a report
  stay anonymous, because the unguessable analysis id *is* the capability, and
  that is what lets a report be shared by URL with someone who has never signed
  in. What does not exist is everything that usually follows sign-in: no
  accounts table, no per-user history, no owner column on an analysis, and no
  profile beyond the `sub` claim the verifier reads — a field nothing consumes
  is a field that leaks into a log.

  There is deliberately no service-account credential either. Verifying an ID
  token needs Google's public signing keys and the public project id, so the
  Admin SDK's private key would be a high-value secret to hold and rotate in
  exchange for operations RepoLens never performs. What follows from that is the
  configuration rule: absent `FIREBASE_PROJECT_ID`, creation is **closed**, not
  open. A deployment that forgot the variable must not serve an anonymous,
  public, work-creating endpoint, and the inverted default makes a read-only
  public deployment a supported configuration rather than a failure. Refusing to
  start would be the other safe choice, but it would also block local frontend
  work against a server with no Firebase project, and the reads are what that
  work needs.
- **No CORS by default.** A layer is applied only when `CORS_ALLOWED_ORIGIN`
  names one exact origin, and the origin is passed into `api::build` rather than
  read inside it — reading configuration in the router builder is what made the
  policy untestable, and the method set shipped allowing `GET` alone while the
  one write this API has went unreachable from a browser. It permits `GET` and
  `POST`, and it is never a wildcard: that would need revisiting the moment an
  endpoint requires credentials, and a permissive default is a security decision
  made by omission.
- **No fourth crate.** Extraction, the Tokei adapter, the worker, and auth stay
  inside `repolens-server` until real code justifies splitting them.
