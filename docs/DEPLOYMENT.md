# Deploying RepoLens

Two hosts and one database: the Axum API on **Render**, the SvelteKit static
build on **Cloudflare Workers Static Assets**, and **Neon** PostgreSQL behind
the API.

This file names variables and never their values.
[`.env.example`](../.env.example) keeps its right-hand sides empty and so does
this runbook. A credential that has appeared in a document, an issue, a pull
request, a CI log or a chat window is rotated at the provider — deleting it does
not un-disclose it.

[`ARCHITECTURE.md`](ARCHITECTURE.md) and the crate documentation describe Cloud
Run, which is the platform the three-role split was designed against. Render is
where the API is being deployed. Where the two disagree, the architecture
document states the design and this file states the deployment.

## Deploy in this order, because the two origins point at each other

`CORS_ALLOWED_ORIGIN` on the API has to name the deployed frontend, and
`PUBLIC_API_ORIGIN` in the frontend build has to name the deployed API. Neither
value exists before the other host does. The circle breaks on the observation
that the two are not equally expensive to change:

- the frontend's value is **baked into the artifact at build time** — into the
  CSP `connect-src` allowlist and into the generated client's base URL — so
  changing it means another build and another deploy;
- the API's value is **read at startup**, so changing it costs a restart.

The cheap one therefore moves last.

1. Deploy the API with every variable below except `CORS_ALLOWED_ORIGIN`.
   Nothing works from a browser yet; the probe answers from `curl`.
2. Note the URL Render assigns the service.
3. Build the frontend with `PUBLIC_API_ORIGIN` set to that URL, deploy it to
   Cloudflare, and note the domain.
4. Set `CORS_ALLOWED_ORIGIN` on the API to that exact origin and restart.
5. Add the Cloudflare domain to Firebase's authorized domains.
6. Run the smoke test at the end of this file.

## Backend — Render

### Environment

| Variable              | Secret | Required | What it is for                                                     |
| --------------------- | ------ | -------- | ------------------------------------------------------------------ |
| `DATABASE_URL`        | yes    | yes      | Neon **pooled** endpoint. Every API query and every worker transaction. |
| `DATABASE_DIRECT_URL` | yes    | yes      | Neon **direct** endpoint. Migrations and session-dependent administration. |
| `GH_ANALYSIS_TOKEN`   | yes    | no       | Raises GitHub's rate-limit ceiling from roughly sixty requests an hour to five thousand. |
| `CORS_ALLOWED_ORIGIN` | no     | yes      | The one origin a browser may call this API from — the deployed frontend. |
| `FIREBASE_PROJECT_ID` | no     | see below | The **public** Firebase project id whose ID tokens this deployment accepts. |
| `PORT`                | no     | injected | Supplied by the platform. Do not set it by hand.                    |

Both database URLs should carry `sslmode=verify-full`, and neither should carry
`channel_binding`. Neon issues `sslmode=require&channel_binding=require`, which
is not the same guarantee: with native roots `require` may validate the
certificate chain like `verify-ca`, but only `verify-full` also checks hostname
identity, and `sqlx` ignores `channel_binding` outright rather than enforcing
it. The server warns at startup when a non-local URL lacks `verify-full`, and it
never echoes the URL it warned about.

`GH_ANALYSIS_TOKEN` is genuinely optional. Only public repositories are ever
analyzed, so the token raises the request budget without widening what can be
read; absent, the server logs a warning and analyzes anyway. Use a fine-grained
token with a short expiry and no write permissions.

`CORS_ALLOWED_ORIGIN` is one exact origin — scheme, host, optional port, no
path, no trailing slash beyond the one that is normalised away, never `*`. A
value that is not a valid origin is **rejected**: the server logs an error and
then serves with no CORS layer at all. That failure presents as "every browser
request is blocked", not as a service that refuses to start, so check the
startup log rather than inferring it from the frontend.

`FIREBASE_PROJECT_ID` is public configuration, not a credential. Verifying a
Firebase ID token needs Google's published signing keys and the project id, both
public, so there is deliberately **no service account** in this deployment —
nothing to hold, rotate, or leak. Absent, it **closes** analysis creation:
`POST /api/v1/analyses` answers `503 AUTHENTICATION_UNAVAILABLE` and the reads
stay anonymous. That is the safe direction for a variable somebody can forget,
and it makes a read-only public deployment a supported configuration rather than
a broken one. See the read-only fallback below.

The startup log states which way the deployment landed, and it is the cheapest
check available:

- `analysis creation requires a Firebase ID token`, naming the project;
- or `FIREBASE_PROJECT_ID is not set, so analysis creation is closed`.

RepoLens ships no `Dockerfile`, so how the Render service builds and starts the
`server` binary is a property of the service configuration and is not recorded
in this repository. Read the service settings rather than assuming a shape for
them.

### Migrations

Migrations are compiled into the `migrate` binary by `sqlx::migrate!`, so no
migration files and no `sqlx-cli` need to reach the deployed environment. The
binary reads `DATABASE_DIRECT_URL` — schema changes rely on session-level
behaviour that a connection pooler restricts.

```sh
cargo run --bin migrate
```

Run it from the workspace root with the production `DATABASE_DIRECT_URL` in the
environment, before the first deploy and after any change under
[`migrations/`](../migrations). Migrations are append-only once applied to a
deployed environment.

Confirm the result through the probe rather than from the migrator's own output:

```sh
curl -s https://<api-host>/api/v1/system/probe
```

The production database must report `"database": "OK"` and
`"schema_version": 2`. The two failure shapes are deliberately distinguishable —
`UNAVAILABLE` means the database could not be reached, `DEGRADED` means it was
reached but migrations have never been applied, and a `null` `schema_version`
means the version could not be read at all. None of those is zero, and none of
them should be read as one.

### Auto-Deploy must be confirmed Off before anything merges to `master`

This is a gate, not a preference. With Auto-Deploy on, merging a pull request
deploys it, and the review gate silently becomes a release gate — including for
changes nobody intended to put in front of a demo. Open the Render service's
settings, confirm Auto-Deploy reads **Off**, and only then merge. Deploying is a
deliberate manual action taken when someone is watching the logs.

## Frontend — Cloudflare

### Build-time configuration

Every variable the frontend reads is public and is inlined into the artifact at
build time. There is no runtime configuration: a wrong value is fixed by
building again, never by editing a deployed file.

| Variable                     | Required | What it is for                                          |
| ---------------------------- | -------- | ------------------------------------------------------- |
| `PUBLIC_API_ORIGIN`          | yes      | The production API origin. Baked into the CSP `connect-src` allowlist and the generated client's base URL. |
| `PUBLIC_FIREBASE_API_KEY`    | no       | Identifies the Firebase project to the browser SDK. Authorizes nothing on its own. |
| `PUBLIC_FIREBASE_AUTH_DOMAIN`| no       | Host the sign-in popup and helper iframe come from.      |
| `PUBLIC_FIREBASE_PROJECT_ID` | no       | The same public project id the API verifies against.     |
| `PUBLIC_FIREBASE_APP_ID`     | no       | Identifies the web app within the project.               |

`PUBLIC_API_ORIGIN` fails the build closed when absent, which is the cheaper
failure by a wide margin: a defaulted value would ship an artifact whose CSP
permits only `localhost:8080`, so every API call from the deployed site would be
blocked by the browser and the symptom — a page that renders and then does
nothing — appears a long way from its cause. A *wrong* value fails the same way
and the build cannot catch it, so check the origin against the deployed API URL
before building, not after.

The `PUBLIC_FIREBASE_*` values default to empty instead. A build without them is
a **read-only demo**: [`web/svelte.config.js`](../web/svelte.config.js) warns
during the build, the submit control says sign-in is unavailable, and the API
refuses creation regardless.

Sign-in also changes the shipped Content Security Policy. When
`PUBLIC_FIREBASE_API_KEY` and `PUBLIC_FIREBASE_AUTH_DOMAIN` are both present,
`svelte.config.js` extends `connect-src` with Identity Toolkit, the secure-token
service and the auth domain, and sets `frame-src` to the auth domain plus
`accounts.google.com`; when they are absent, none of those hosts are permitted
and `frame-src` stays `'none'`. The policy is a `<meta>` tag inside every
generated document, so **configure Firebase before the build you intend to
deploy** — adding the values afterwards changes nothing until the artifact is
rebuilt.

### Build and deploy

```sh
PUBLIC_API_ORIGIN=<production-api-origin> pnpm -r build
```

Run from the repository root, with the `PUBLIC_FIREBASE_*` values in the
environment as well. The deployable artifact is `web/build`, which is what
[`wrangler.jsonc`](../wrangler.jsonc) points `assets.directory` at. That
configuration is assets-only — no Worker script — and sets
`not_found_handling: "single-page-application"`, which is what makes direct
navigation and hard refresh on `/analyses/<id>` and `/reports/<id>` serve
`index.html` with `200` instead of Cloudflare's own 404. Report URLs are meant
to survive a copy-paste, so that behaviour is part of the smoke test rather than
an implementation detail.

### The Firebase console step people forget

In the Firebase console, open **Authentication → Settings → Authorized domains**
(the console spells it that way) and add the deployed Cloudflare domain.

Skipping this fails **only in production**. `localhost` is authorized by default,
so sign-in works throughout development and then fails on the deployed site with
`auth/unauthorized-domain` — a failure with no local reproduction and no build
that could have caught it.

## Smoke test

Run these in order against the deployed hosts. Each one either passes or the
deployment is not done.

1. `GET /api/v1/system/probe` reports `"api": "OK"` and `"database": "OK"`.
2. The same response reports `"schema_version": 2` — the number, not `null` and
   not `0`.
3. Signed out, submitting a repository URL is refused. The API answers `401`
   with `UNAUTHENTICATED`, or `503` with `AUTHENTICATION_UNAVAILABLE` if this
   deployment has no Firebase project configured.
4. Sign-in succeeds on the deployed domain — no `auth/unauthorized-domain`, and
   no CSP violation in the browser console.
5. Signed in, creating an analysis answers `202` with an analysis id.
6. Polling `GET /api/v1/analyses/{id}` walks the states in order: `QUEUED` →
   `RESOLVING` → `COLLECTING` → `ANALYZING` → `BUILDING_REPORT` → `COMPLETED`.
7. `GET /api/v1/analyses/{id}/report` carries a `commit_sha` and a `tree_sha`
   that are **different values**. Both are 40-character hex digests and they are
   not interchangeable; equal values mean the commit SHA has been echoed into
   the tree field. The progress response publishes `commit_sha` alone, so this
   one is checked on the report.
8. The report renders at `/reports/{id}`.
9. `composition` in the report is `null`, and the page renders it as an explicit
   unknown rather than as zeros or an empty section. Line counting is issue #12
   and does not exist yet, so `null` is the correct and honest answer here — a
   rendered `0` would be the dishonest one.
10. Reloading the report page re-reads it from PostgreSQL and renders the same
    content — the report is persisted, not held in browser state.
11. A signed-out browser (or a private window) can open the same report URL and
    read it. Reads are anonymous by design: the unguessable analysis id is the
    capability, which is what lets a report be shared by URL.
12. No secret appears anywhere a browser can see it. Check the network responses
    and the JavaScript bundle: the `PUBLIC_FIREBASE_*` values are expected and
    public, and nothing else configured on the API — no database URL, no GitHub
    token — appears in either.

## Read-only fallback

If Firebase cannot be finished in time, deploy with `FIREBASE_PROJECT_ID` unset
on the API and the `PUBLIC_FIREBASE_*` values unset in the frontend build.

The result is coherent rather than half-broken: the API refuses every create
with `503 AUTHENTICATION_UNAVAILABLE`, the UI says sign-in is unavailable
instead of offering a control that cannot work, and existing report URLs remain
publicly readable. Steps 3 and 8 to 12 of the smoke test still apply; steps 4 to
7 do not.

**This is an acceptable honest demo. An anonymous public creation endpoint is
not.** Creating an analysis spends GitHub request budget and database rows on
behalf of whoever asked, and opening that to the internet to make a demo look
complete is the exact failure the closed-by-default configuration exists to
prevent.

### Analyses that already exist

Three analyses of `rust-lang/crates.io` are already persisted in the production
database, at commit `7bef82cebb702b89ec8d3f13facf67a83bc7d090`:

```text
019fd76c-7fbc-74f1-8ff0-e19a5b7de821
019fd76d-5567-7f10-bee8-42671939ae36
019fd7fe-26fe-7123-ab8c-f5cae8ac7e90
```

Open `/reports/<id>` for one of them and confirm it renders before relying on it
in a demo, rather than assuming from this list that it will.

## Known limitation: an analysis dies with the process

Analysis does not yet run in the `worker` binary. `POST /api/v1/analyses` writes
the row and then runs the whole pipeline on a `tokio::spawn`ed task **inside the
API process**, which contradicts both the three-role split in
[`ARCHITECTURE.md`](ARCHITECTURE.md) and the rule that an HTTP request never
performs analysis.

The consequence for a deployment is concrete. A restart, a redeploy, or an
instance being reclaimed mid-run leaves a row in `ANALYZING` that nothing will
ever move: there is no lease to expire and no recovery to reclaim it. Which is
the other reason Auto-Deploy is off — a merge landing during a live analysis
strands it.

So: run **one controlled analysis at a time** during a demo, and do not deploy
while one is running. Issue #7 replaces the spawn with a durable PostgreSQL
claim (`FOR UPDATE SKIP LOCKED`, explicit leases, abandoned-lease recovery,
bounded retries, idempotent effects) and is the immediate reliability follow-up.
