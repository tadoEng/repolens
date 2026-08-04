# Migrations

SQL migrations applied by the `migrate` binary, which compiles them in via
`sqlx::migrate!` and runs them against the **direct** (unpooled) database
endpoint.

- Author locally with `sqlx migrate add <name>`. `sqlx-cli` is a development
  tool and must never ship inside the container image.
- Migrations are append-only once applied to a deployed environment.
- After changing a `sqlx::query!` call, refresh the committed `.sqlx/` offline
  cache with `cargo sqlx prepare --workspace`, or Docker and CI builds fail with
  no live database to verify against.

This directory is empty of migrations on purpose. The first schema lands with
the walking skeleton (issue #11), which is the first code that needs a table.
