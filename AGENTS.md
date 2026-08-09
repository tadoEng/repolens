# RepoLens

RepoLens analyzes **one public GitHub repository at an exact commit SHA** and
produces a deterministic, evidence-backed report.

Two properties settle most arguments here:

- **Determinism.** The same inputs produce the same report. A report publishes
  the repository coordinate, commit SHA, tree SHA, analyzer version and ruleset
  version, and those five are what a reader has today to tell two reports apart.
  `ReproducibilityKey` names three more inputs — evidence-source version,
  composition counter, exclusion-policy version — that no report carries, and
  nothing constructs that type. So a report differing only for one of those
  reasons cannot yet say so. Issue #28 publishes the key; until it lands, do not
  write that the key is published.
- **Honesty about limits.** A truncated tree or an abandoned line count is a
  result, not an error to swallow. Never turn unknown into zero, empty, or
  success.

[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) holds the reasoning behind every
rule below. This file does not repeat it.

## Repository map

```text
crates/repolens-core           domain — identity, findings, evidence, rule contract,
                               reproducibility key
crates/repolens-github         GitHub REST boundary — resolve, tree, blobs, bounded archive
crates/repolens-server         axum API, worker, migrate binary, public wire DTOs
contracts/                     openapi.json and the executable fixtures (analysis-v1, admin-v1)
packages/repolens-api-client   generated TypeScript client
packages/repolens-msw          MSW handlers driven by those fixtures
web/                           SvelteKit application, static output only
migrations/                    SQL applied by the `migrate` binary
docs/                          architecture, deployment runbook, agent-contract manifest
scripts/                       repository-level checks that are neither Cargo nor pnpm
```

`repolens-server` → `repolens-github` → `repolens-core`. No other edges, no
cycles; the crate manifests are the enforcement.

## Source of truth, in order

1. Executable tests and generated artefacts —
   [`contracts/openapi.json`](contracts/openapi.json),
   [`contracts/fixtures`](contracts/fixtures),
   `packages/repolens-api-client/src/schema.ts`.
2. [`.github/workflows/ci.yml`](.github/workflows/ci.yml) and the crate and
   package manifests.
3. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
4. Issue and pull-request descriptions.
5. This file and the scoped `AGENTS.md` files.

Agent prose is guidance. It holds no authority over code, CI, or repository
state, and where it disagrees with any of them the prose is the defect — fix it
in the same change.

The nearest `AGENTS.md` governs its subtree and extends this file rather than
replacing it:

- [`crates/repolens-core`](crates/repolens-core/AGENTS.md)
- [`crates/repolens-github`](crates/repolens-github/AGENTS.md)
- [`crates/repolens-server`](crates/repolens-server/AGENTS.md)
- [`contracts`](contracts/AGENTS.md)
- [`packages`](packages/AGENTS.md)
- [`web`](web/AGENTS.md)

[`docs/agent-contracts.json`](docs/agent-contracts.json) declares that set and
`scripts/check-agent-contracts.mjs` enforces it, so an undeclared or missing
instruction file fails CI.

## Verification

Run from the repository root.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo build --workspace --locked
cargo test --workspace --locked
cargo doc --workspace --no-deps
pnpm install --frozen-lockfile
pnpm -r check
pnpm -r lint
pnpm -r test
pnpm -r build
pnpm --filter @repolens/web run test:e2e
pnpm --filter @repolens/web run test:e2e:integration
node scripts/check-agent-contracts.mjs
node --test "scripts/*.test.mjs"
```

Five things about that list are not visible from it:

- `cargo test --workspace` needs a PostgreSQL. `crates/repolens-server/tests/postgres.rs`
  panics when `DATABASE_URL` is unset rather than skipping, because a suite that
  vanishes with its dependency reports green while proving nothing. Export
  `DATABASE_URL` and `DATABASE_DIRECT_URL` and run `cargo run --bin migrate`
  once against that database first; CI provisions a `postgres:17` service and
  does the same.

- CI exports `RUSTFLAGS: -D warnings`, so locally either export it too or append
  `-- -D warnings` to the Clippy line. The workspace enables `clippy::pedantic`,
  which makes missing doc backticks (`doc_markdown`) and identical match arms
  (`match_same_arms`) build failures rather than suggestions.
- `pnpm -r test` needs a browser: `web`'s Vitest suite runs in browser mode, not
  jsdom. Install once with
  `pnpm --filter @repolens/web exec playwright install --with-deps chromium chromium-headless-shell`
  — `chromium` and the headless shell are separate downloads and browser mode
  uses the shell.
- `pnpm -r build` fails closed without `PUBLIC_API_ORIGIN`. That is deliberate:
  the value is baked into the CSP `connect-src` allowlist at build time, so a
  silent default would ship an artifact whose own API calls the browser blocks.
- Delete `web/.svelte-kit` and every `*.tsbuildinfo` before trusting a local
  typecheck. Stale generated types have twice let a local run pass while CI
  failed.

`cargo doc --workspace --no-deps` is local-only; no CI job runs rustdoc yet, so
broken intra-doc links are caught by whoever runs it.

## Regenerating generated artefacts

`contracts/openapi.json`, every fixture under `contracts/fixtures/`,
`packages/repolens-api-client/src/schema.ts`, and
`packages/repolens-api-client/src/fixtures.ts` are **generated end to end and
never hand-edited**. Editing one is writing a second definition of the contract
that drifts the moment a DTO changes, and drifts silently.

After deliberately changing a route or a DTO, regenerate in this order — each
step reads the previous step's output:

```sh
UPDATE_OPENAPI=1 cargo test -p repolens-server --test openapi
UPDATE_FIXTURES=1 cargo test -p repolens-server --test fixtures
pnpm --filter @repolens/api-client schema:update
pnpm --filter @repolens/api-client fixtures:update
```

Then commit the results. CI asserts a clean working tree, so a regeneration that
was run but never committed fails there.

## Invariants

- `repolens-core` depends on nothing infrastructural — no `axum`, `sqlx`,
  `reqwest`, no async runtime. Its manifest is the enforcement; there is nothing
  else to remember.
- Analyzer rules perform no I/O, read no clock, and consult no configuration
  they were not handed explicitly.
- Database rows are never public DTOs. Wire types live in
  `crates/repolens-server/src/contract` and are the only types `utoipa` sees.
- The contract flows one way: Rust DTOs → OpenAPI → generated TypeScript →
  fixtures → MSW → Svelte. Nothing flows back.
- Tree and blob data is canonical evidence. The commit archive is bounded
  ephemeral transport for line counting only: its bytes are not guaranteed
  stable for a fixed commit, so they never enter the reproducibility key.
- `server`, `worker`, and `migrate` share one image and stay three distinct
  roles. Analysis never runs inside an HTTP request.
- JSON object fields are `snake_case` — Rust already produces that, so no struct
  carries `rename_all`. Enum values are `SCREAMING_SNAKE_CASE`, where the rename
  is unavoidable and is applied once per enum.
- `MISSING` and `UNABLE_TO_VERIFY` are different claims and must never be
  collapsed. Neither may severity and confidence: one is impact if the finding
  holds, the other is the strength of the evidence behind it.

## Security and privacy

- Real values live only in a git-ignored `.env.local`, GitHub Actions secrets,
  or the provider console. [`.env.example`](.env.example) is committed and its
  right-hand sides stay empty.
- Never commit or paste a Neon URL, a GitHub token, service-account material, or
  a private key — not into code, an issue, a pull request, a CI log, or a chat
  window. **A credential that has appeared anywhere public is rotated at the
  provider.** Deleting it does not un-disclose it.
- The Firebase browser config is public configuration and carries the `PUBLIC_`
  prefix for that reason. Server credentials never do.
- Errors and logs carry neither secrets nor repository content beyond what the
  evidence contract already publishes.
- Creating an analysis is authenticated and resource-bounded. CORS names one
  exact configured origin or is absent; a permissive default is a security
  decision made by omission.

## Change discipline

- Read the issue and the nearest `AGENTS.md` before editing.
- Do not widen scope, invent a DTO, or add an abstraction with no consumer in
  the same change.
- Say what you verified by running it and what you assumed. Assumptions about
  the deployed platform stay marked until observed there — `vite preview` is not
  Cloudflare.
- Commit subjects are present-tense imperative under 72 characters; the body
  explains why, not what the diff already shows.

## Before opening a pull request

- The gates above pass, run rather than assumed.
- Generated artefacts are regenerated and committed.
- No secret, provider host, or absolute local path in the diff.
- No new crate edge, and no database row on the wire.
- Accessibility assertions cover any new interactive surface, and visual
  baselines exist for both platforms if rendered layout changed.
- Anything still unproven on the real platform says so, rather than reading as
  verified.

## Reference

- [`README.md`](README.md) — the product boundary and what is deliberately out
  of scope
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — why each invariant exists
- [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) — every deployed variable, the
  order the two hosts must go out in, and the smoke test
- [`docs/PRE-REGISTRATION.md`](docs/PRE-REGISTRATION.md) — the measurement
  criteria, fixed before any result was seen. **Feature development ends at the
  freeze tag it defines**; during measurement no application change is
  permitted, and no criterion may be revised because a result is inconvenient
- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) — the gates exactly as
  they run
- [`Cargo.toml`](Cargo.toml), [`package.json`](package.json),
  [`rust-toolchain.toml`](rust-toolchain.toml), [`.nvmrc`](.nvmrc) — toolchain
  and dependency pins
