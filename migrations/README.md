# Migrations

SQL migrations applied by the `migrate` binary, which compiles them in via
`sqlx::migrate!` and runs them against the **direct** (unpooled) database
endpoint.

- Author locally with `sqlx migrate add <name>`. `sqlx-cli` is a development
  tool and must never ship inside the container image.
- Migrations are append-only once applied to a deployed environment.

## Current schema

| Migration              | What it establishes                                  |
| ---------------------- | ---------------------------------------------------- |
| `0001_schema_meta.sql` | A baseline row the system probe reads, so `GET /api/v1/system/probe` proves connection, migration and query in one call |
| `0002_analyses.sql`    | `analyses` and `reports` — two tables, not eight      |

A report is stored as one JSONB document rather than normalised into findings,
evidence and limitation rows. Nothing in Phase 0 queries *inside* a report: it
is written once when an analysis completes and read back whole. Normalising it
would buy join complexity and a second definition of a shape the contract
already fixes. A later phase that needs "every repository where rule X fired"
gets a deliberate migration with a real query behind it.

## Queries are written at runtime, not compiled

There is no `.sqlx/` offline cache, because no `sqlx::query!` macro is used.
Compile-time verification needs either a live database at build time or a
committed cache that every query change must regenerate — friction this query
set has not earned, and a stale cache fails CI in a way that reads as a broken
build rather than a missed step.

That trade is recorded rather than assumed: it is exactly the kind of stack
friction issue #10 exists to report on. If the query set grows enough to earn
the cache, `cargo sqlx prepare --workspace` is what generates it.
