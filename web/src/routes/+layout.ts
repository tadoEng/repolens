/*
 * Root layout options — these two lines decide the entire deployment shape.
 *
 * ssr = false
 *   There is no Node production server, and there will not be one. The app ships as
 *   static assets to Cloudflare Workers Static Assets. Turning SSR off makes that
 *   explicit at the framework level instead of leaving it as an implicit consequence
 *   of the adapter, so a `+page.server.ts` added later fails immediately and loudly
 *   rather than at deploy time.
 *
 * prerender = false
 *   Nothing is prerendered, so the adapter's `index.html` fallback is the one and only
 *   application shell. See the long note in svelte.config.js: on Cloudflare the
 *   fallback document must be named `index.html`, which collides with the artifact a
 *   prerendered `/` would produce. With `ssr = false` a prerendered page is an empty
 *   hydration shell anyway — identical to the fallback — so prerendering buys nothing
 *   here and costs a name collision.
 *
 * Both are inherited by every route. `/analyses/[analysisId]` and
 * `/reports/[analysisId]` could not be prerendered regardless: their parameters are
 * unguessable analysis IDs and are not enumerable at build time.
 */
export const ssr = false;
export const prerender = false;
