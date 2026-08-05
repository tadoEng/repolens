# Fixtures

Empty on purpose.

Fixture content is owned by **issue #14**, which defines the `analysis-v1` and `report-v1`
executable contracts and publishes them under `contracts/fixtures/analysis-v1/`:

```
contracts/fixtures/analysis-v1/
├── queued.json            ├── failed-retriable.json
├── resolving.json         ├── failed-permanent.json
└── completed-report.json  └── loc-unavailable.json
```

The pipeline those fixtures sit in runs one way, and only one way:

```
Rust DTOs → OpenAPI → generated TypeScript → fixture type-check → MSW → Svelte
```

So a fixture written here first would be a hand-authored guess at a response shape,
type-checked against nothing. That is the drift the pipeline exists to eliminate, and it is
worse than having no fixtures at all — a UI built on an invented fixture passes review, ships,
and fails against the real API.

## What lands here at #14

Symlinks or a small loader pointing at `contracts/fixtures/analysis-v1/`, plus any
frontend-only variants that the API cannot produce (an unknown enum variant, for instance,
which the statically deployed frontend has to survive).

Each fixture must exercise something the UI cannot render honestly without:

- nullable `commit_sha` during `QUEUED` / `RESOLVING` — the header shows "resolving…", not a
  blank
- stable machine error codes, `{code, message, retry_after_seconds?}`, `code` an OpenAPI enum
- explicit `retry: {allowed, reason?}` — never inferred from a state name
- deterministic finding order, decided by the server
- bounded evidence excerpts with `truncated` and `digest`
- report-level `limitations[]`, not only per-finding
- `analyzer_version` and `ruleset_version`
- a polling hint (`poll_after_ms`, ideally ETag/304)
- structured LOC exclusions: `{path_or_rule, reason, matched_rule, file_count, bytes}`
- nullable `LineCountSummary` — absent composition is `UNABLE_TO_VERIFY`, a designed state,
  not an error
- an unknown enum variant, to prove the deployed frontend degrades instead of crashing
