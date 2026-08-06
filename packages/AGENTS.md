# `packages`

> The nearest `AGENTS.md` governs this subtree. It extends the root
> [`AGENTS.md`](../AGENTS.md) rather than replacing it, and loses to executable
> code, tests, generated contracts, and CI whenever prose is stale.

Two framework-independent packages sitting between the API contract and any
consumer of it.

`@repolens/api-client` must not depend on Svelte or SvelteKit, directly or
transitively. It is a contract package: a second consumer — a CLI, a script, a
different frontend — must be able to install it without pulling in a UI
framework. `@repolens/msw` may depend on the client; nothing here depends on
`web`.

## Two generated files, neither of them authored

| File                     | Generated from            | Regenerate with       |
| ------------------------ | ------------------------- | --------------------- |
| `repolens-api-client/src/schema.ts`   | `contracts/openapi.json`  | `schema:update`   |
| `repolens-api-client/src/fixtures.ts` | `contracts/fixtures/analysis-v1` | `fixtures:update` |

Each has a staleness test that regenerates from the committed input and compares
byte for byte, so hand-editing either one fails `pnpm -r test` rather than
producing a client that quietly disagrees with the server. Both comparisons
normalize `\r\n` to `\n` first: without that the gate passes in Ubuntu CI and
fails on every Windows checkout, and the diff shows nothing because the
difference is invisible whitespace.

`fixtures.ts` is generated as TypeScript literals under `satisfies` rather than
imported as JSON, because TypeScript widens string literals in JSON modules —
`"state": "QUEUED"` types as `string`, which would silently retire the enum
check the unknown-variant policy depends on.

## MSW handlers hold no payloads

`@repolens/msw` serves the executable fixtures. A handler that returns an
object literal it declared itself is a second, unversioned copy of the contract
that no gate compares against anything.

## Changing a fixture changes the contract

A fixture edit is only complete when the Rust DTO, `contracts/openapi.json`,
`schema.ts`, `fixtures.ts`, the MSW handlers, and whatever renders it in `web`
have all been updated and run. Doing four of the six leaves a chain that still
compiles.

## Commands

Run from the repository root.

```sh
pnpm --filter @repolens/api-client check
pnpm --filter @repolens/api-client test
pnpm --filter @repolens/msw check
pnpm --filter @repolens/msw test
```

`tsc --noEmit` is the `check` script in both packages. Delete any stale
`*.tsbuildinfo` before believing a local pass.
