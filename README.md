# RepoLens

RepoLens analyzes a GitHub repository at an exact commit SHA and produces a reproducible, evidence-backed architecture and engineering-health report.

## Product contract

```text
GitHub repository URL
        ↓
resolve exact commit SHA
        ↓
collect bounded repository evidence
        ↓
run deterministic analyzers
        ↓
persist findings and report
        ↓
render through a static SvelteKit application
```

The first release is deliberately narrow:

- public GitHub repositories only;
- exact-SHA reproducibility;
- deterministic evidence before AI interpretation;
- no repository writes;
- no universal quality score;
- no claim to replace security scanners or SonarQube.

## Intended stack

- SvelteKit static frontend
- Axum modular monolith
- Tower middleware
- PostgreSQL
- PostgreSQL-backed worker
- OpenAPI-generated TypeScript client
- Cloudflare frontend hosting
- Google Cloud Run API/worker
- Neon Postgres
- GitHub App later, personal development token initially

See the issue tracker for the implementation sequence.
