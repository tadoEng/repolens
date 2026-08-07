# `web`

> The nearest `AGENTS.md` governs this subtree. It extends the root
> [`AGENTS.md`](../AGENTS.md) rather than replacing it, and loses to executable
> code, tests, generated contracts, and CI whenever prose is stale.

Svelte 5 with runes, SvelteKit through `adapter-static`. `src/routes/+layout.ts`
sets `ssr = false` and `prerender = false`, and both are inherited by every
route. There is no Node production server and there will not be one: a
`+page.server.ts` added here must fail immediately rather than at deploy time.

`build/index.html` is simultaneously the `/` document and the nested-route
fallback. **Direct navigation and hard refresh on `/analyses/<id>` and
`/reports/<id>` are part of the deployment contract**, not a nicety — a report
is shared by URL. Cloudflare's `not_found_handling` requires the fallback to be
named `index.html`, which is why nothing is prerendered; the long note in
`svelte.config.js` has the measurement behind that.

`PUBLIC_API_ORIGIN` is read once in `svelte.config.js` and baked into two
places: the client's base URL and the CSP `connect-src` allowlist. They cannot
be allowed to disagree, which is why there is one source and no default in a
production build.

Every `PUBLIC_*` value comes from the **repository root**, not from `web/`:
`vite.config.ts` sets `envDir: '..'` and `svelte.config.js` reads the same
directory through Vite's own `loadEnv`. The repository keeps one git-ignored
root `.env.local` and this keeps the app and the CSP reading it. Without
`envDir` Vite would search `web/`, find nothing, and resolve every `PUBLIC_*` to
empty — which is quiet rather than loud, because absent Firebase configuration
deliberately degrades to read-only, so it looks like a broken feature instead of
a misconfigured build.

## Data comes from the generated client, only

Import DTOs and operations from `@repolens/api-client`. Never hand-write a copy
of a Rust type and never invent a response literal in a component or a test — a
hand-written shape compiles happily against nothing and is discovered wrong by a
user.

Transport, in order of preference:

1. **Use the generated operation** whenever the OpenAPI document publishes one.
2. **Components never perform transport.** A component that reaches the network
   is a component that cannot be rendered from a fixture.
3. **A single GET-only adapter is permitted while an operation is absent.** The
   contract is fixed ahead of the routes that serve it (#14 owns the shapes, #6
   the endpoints), so a provisional read path exists during that gap. It stays
   centralized in `$lib/api/`, stays read-only, and disappears as each operation
   is published. Shapes are still imported — only the URL is provisional.

A mutation is never provisional. Starting work is authenticated (#13), and a
hand-written POST would be the abuse surface that gate exists to close.

Fixtures for tests come from `@repolens/msw`, which is driven by the same
executable fixtures the backend generates.

## Markup and dependencies

Native semantic HTML first: `<details>`, `<table>`, `<button>`, `<a>`. Reach for
`bits-ui` only when a genuinely nontrivial accessible interaction is needed —
listbox, combobox, dialog, focus trap. It is an implementation detail inside a
primitive, never the design system; `src/lib/styles/tokens.css` stays
authoritative.

Not installed, and not to be installed without a separately approved need:
Tailwind, a shadcn-svelte foundation, TanStack Table, and any charting library.
Two of the composition views draw bars with a `::before` background set through
CSSOM, and that is the whole charting requirement so far.

`src/lib/components/README.md` records which component rules are load-bearing
and which defect each one prevents. Read it before changing a component; several
of them look like style and are not.

## Accessibility is a gate

`e2e/` runs axe against WCAG 2.1 AA on all three routes at 360, 768, and 1280px.
Beyond axe, which cannot see most of it: keyboard reachability and visible
focus, landmark and heading structure, and horizontal overflow at 360px.

Visual baselines are per platform and `playwright.config.ts` pins
`updateSnapshots` to `none`, so an ordinary run can never quietly approve a
regression. Commit the Linux baseline as well as your own — CI fails when it is
missing.

## Commands

Run from `web/`.

```sh
pnpm check
pnpm lint
pnpm test
pnpm test:e2e
pnpm test:e2e:integration
PUBLIC_API_ORIGIN=http://localhost:8080 pnpm build
pnpm exec playwright test visual.spec.ts --update-snapshots
```

`pnpm test` runs Vitest in **browser mode**, not jsdom, so it needs the
Playwright Chromium download as well as the headless shell.

`pnpm preview` serves the built output, and that is all it proves. It is not
Cloudflare: `not_found_handling`, response headers, and cold-start behaviour are
unverified until observed on the deployed origin. Say so rather than implying
coverage that does not exist.
