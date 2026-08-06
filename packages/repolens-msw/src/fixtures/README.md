# Fixtures

Still empty on purpose. **Nothing is authored here.**

Fixture content is owned by **issue #14** and lives in `contracts/fixtures/analysis-v1/`,
generated from the Rust DTOs:

```
contracts/fixtures/analysis-v1/
├── queued.json            ├── failed-retriable.json
├── resolving.json         ├── failed-permanent.json
└── completed-report.json  └── loc-unavailable.json
```

The pipeline they sit in runs one way, and only one way:

```
Rust DTOs → OpenAPI → generated TypeScript → fixture type-check → MSW → Svelte
```

So a fixture written here would be a hand-authored guess at a response shape, type-checked
against nothing. That is the drift the pipeline exists to eliminate, and it is worse than
having no fixtures at all — a UI built on an invented fixture passes review, ships, and
fails against the real API.

## Where the fixtures actually enter TypeScript

`@repolens/api-client` generates `src/fixtures.ts` from the JSON above and exports the
result as `ANALYSIS_FIXTURES`. `analysis-handlers.ts` imports that. The binding is
generated rather than written because TypeScript widens string literals in JSON modules —
`"state": "QUEUED"` would arrive as `string`, retiring the enum check the unknown-variant
policy depends on — and it is regenerated and compared on every test run, so it cannot
drift from the JSON.

## What would still justify a file here

Only a case the API cannot produce and the fixtures therefore cannot contain: an **unknown
enum variant**, to prove a deployed bundle degrades instead of crashing when the API gains
a variant months later. That case is currently covered without a fixture, by
`unknown-variant.test.ts` in `@repolens/api-client`, which feeds the describe functions a
variant no schema declares.
