# `repolens-github`

> The nearest `AGENTS.md` governs this subtree. It extends the root
> [`AGENTS.md`](../../AGENTS.md) rather than replacing it, and loses to
> executable code, tests, generated contracts, and CI whenever prose is stale.

Everything RepoLens is allowed to ask GitHub for, and nothing else: resolve a
repository, resolve a reference to an exact commit and its root tree, list the
tree, fetch an explicitly chosen set of blobs, and stream one commit archive.
`GitHubRepositorySource` in `src/lib.rs` is that list; adding a sixth operation
is a design decision, not a refactor.

`reqwest` rather than an SDK, because the controls that matter at this boundary
are exactly what an SDK abstracts away.

## Non-negotiable at this boundary

- **The version header goes on every request.** `GITHUB_REST_API_VERSION` is
  exact-pinned. Omitting it does not mean "latest": GitHub falls back to an
  older version, silently pinning us to something never tested.
- **`Authorization` never crosses a host.** The archive endpoint redirects to a
  storage host; forwarding the credential there leaks it. `MAX_REDIRECT_HOPS`
  bounds the chain, and the policy is asserted by tests rather than inherited
  from library defaults.
- **Truncation and rate-limit metadata survive.** A truncated tree is data — the
  caller must be able to say so in a limitation. Rate-limit exhaustion carries
  its reset instant, and is distinguishable from a plain `403`.
- **Every response is read through a size cap** and parsed from the resulting
  slice. `reqwest`'s `json` feature is deliberately absent: `Response::json`
  reads an unbounded body, which is the one path around the byte budget this
  crate exists to enforce.
- **The archive streams to a path**, bounded by `max_compressed_bytes`, and
  never becomes an in-memory buffer. Exceeding the budget must be a catchable
  error, not an out-of-memory kill that strands a worker lease. A failed
  download leaves behind no file it did not create.
- **Tokei and LOC counting do not live here.** Downloading an archive is
  GitHub-specific; counting lines is not. The counter belongs to
  `repolens-server`, implementing the `RepositoryCompositionCounter` contract
  `repolens-core` defines. Binding it to this adapter would make a future
  local-folder analyzer conceptually depend on GitHub.

Tree and blob data is canonical evidence and is digested. Archive bytes are not:
GitHub does not guarantee them stable for a fixed commit, so hashing the tarball
would break reproducibility rather than establish it.

Every skipped or unread path is recorded with a reason. Silently returning fewer
blobs than requested turns a budget stop into a claim that the file was absent.

## Commands

Tests run against a `wiremock` server, never GitHub: a suite that needs the
network fails for reasons unrelated to the code, and rate-limit exhaustion
cannot be provoked against the real API on purpose.

```sh
cargo clippy -p repolens-github --all-targets
cargo test -p repolens-github
cargo test -p repolens-github --test ingestion
cargo doc -p repolens-github --no-deps
```

New transport behaviour needs a test in `tests/ingestion.rs` that exercises the
wire, not the type: redirects, caps, and budgets are only real when a server
actually answers.
