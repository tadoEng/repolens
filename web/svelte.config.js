import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/**
 * The origin of the RepoLens API (the Axum service on Cloud Run).
 *
 * Read here so it can be baked into the CSP `connect-src` allowlist at build time,
 * and re-exported onto `process.env` so `$env/static/public` resolves the same value
 * in application code. There is exactly one source of truth for the API origin.
 *
 * Real values come from a git-ignored `.env.local`; nothing is committed.
 *
 * **This fails closed.** A production build with no `PUBLIC_API_ORIGIN` throws rather
 * than defaulting, because the value is baked irreversibly into two places at build
 * time — the CSP `connect-src` allowlist and the client's base URL. A silent localhost
 * default would produce a deployable artifact whose CSP permits only `localhost:8080`,
 * so every API call from the deployed site would be blocked by the browser. That failure
 * surfaces as a blank page in production, far from its cause. Refusing to build is the
 * cheaper failure by a wide margin.
 *
 * Development still defaults, because there the value is right and re-typing it on every
 * clean checkout buys nothing. CI builds set it explicitly — including to a localhost
 * value when that is genuinely what is being tested — so the choice is always recorded
 * rather than inherited.
 */
const LOCAL_API_ORIGIN = 'http://localhost:8080';

// `vite build` sets NODE_ENV=production; `vite dev` does not.
const isProductionBuild = process.env.NODE_ENV === 'production';
const configuredOrigin = process.env.PUBLIC_API_ORIGIN;

if (!configuredOrigin && isProductionBuild) {
	throw new Error(
		'[repolens] PUBLIC_API_ORIGIN is required for a production build.\n' +
			'It is baked into the CSP connect-src allowlist and the generated API client at ' +
			'build time, so defaulting it would ship an artifact that cannot talk to its own ' +
			'API.\n' +
			`Set it explicitly, e.g. PUBLIC_API_ORIGIN=${LOCAL_API_ORIGIN} pnpm build`
	);
}

const apiOrigin = configuredOrigin ?? LOCAL_API_ORIGIN;

if (!configuredOrigin) {
	console.warn(
		`[repolens] PUBLIC_API_ORIGIN is not set; using ${LOCAL_API_ORIGIN} for development. ` +
			'Production builds refuse to start without it.'
	);
	process.env.PUBLIC_API_ORIGIN = apiOrigin;
}

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		// ---------------------------------------------------------------------------
		// Cloudflare nested-route fallback — the decision, and why.
		// ---------------------------------------------------------------------------
		//
		// Cloudflare Workers Static Assets with `not_found_handling: "single-page-application"`
		// serves `/index.html` with `200 OK` for any request that does not match an asset.
		// https://developers.cloudflare.com/workers/static-assets/routing/single-page-application/
		//
		// So the fallback document MUST be named `index.html`. crates.io uses
		// `fallback: '200.html'`, but that is the *Netlify* convention: Netlify treats a
		// file literally named `200.html` as its SPA rewrite target. Cloudflare has no such
		// convention — copying it would leave `/analyses/<id>` resolving to Cloudflare's
		// own 404 page on direct navigation and hard refresh, while working fine in
		// client-side navigation. That is exactly the class of bug that only shows up
		// after deploy, so it is settled here rather than discovered later.
		//
		// The sharp edge, measured rather than assumed. `adapter-static` writes the
		// fallback into the same directory as prerendered pages, so `fallback: 'index.html'`
		// collides with the artifact a prerendered `/` produces (also `index.html`).
		//
		// On @sveltejs/kit 2.70.2 + @sveltejs/adapter-static 3.0.10 this does NOT error.
		// The adapter runs `writePrerendered()` and then `generateFallback()`, and the
		// second overwrites the first with only a yellow warning:
		//
		//     Overwriting build\index.html with fallback page.
		//     Consider using a different name for the fallback.
		//
		// (Verified by building this app with `prerender = true` on the root route;
		//  see kit/src/core/adapt/builder.js — the `existsSync(dest)` branch warns, then
		//  writes anyway.) That is worse than an error, not better: prerendering `/` while
		//  using the Cloudflare-mandated fallback name looks like it works, costs build
		//  time, and silently ships the fallback shell instead of the prerendered page.
		//
		// Two ways to resolve it:
		//
		//   (a) Prerender `/` and rename the fallback. Rejected — the fallback name is not
		//       ours to choose on Cloudflare (see above), and `not_found_handling` offers
		//       no way to point at a different file.
		//
		//   (b) Prerender nothing, and let the fallback be the single application shell.
		//       Adopted. It costs nothing here, because the root layout sets `ssr = false`:
		//       with SSR off, a "prerendered" page is an empty shell that hydrates
		//       client-side — the same thing the fallback already is. And two of our three
		//       routes (`/analyses/[analysisId]`, `/reports/[analysisId]`) take unguessable
		//       analysis IDs, so they are not enumerable at build time and cannot be
		//       prerendered at all: a fallback is mandatory regardless. Building with
		//       `prerender = true` inherited by those routes fails outright with
		//       "marked as prerenderable, but were not prerendered because they were not
		//       found while crawling your app".
		//
		// Net effect: `build/index.html` is both the `/` document and the nested-route
		// fallback — one file, one code path, and the same 200 response either way.
		//
		// If a future route becomes genuinely prerenderable (a static `/about`, say), it
		// can opt in per-route with `export const prerender = true` *as long as it is not
		// the root route*, because only `/` produces a colliding `index.html`. If the root
		// route ever needs real prerendered content, that is a signal SSR is back on the
		// table, and the hosting decision has to be reopened — not worked around here.
		//
		// `precompress` stays off: crates.io enables it so their own Rust backend can
		// serve `.br`/`.gz`; Cloudflare compresses at the edge, so precompressed copies
		// would be dead weight in the bundle.
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: 'index.html',
			precompress: false,
			strict: true
		}),

		// ---------------------------------------------------------------------------
		// Content Security Policy.
		// ---------------------------------------------------------------------------
		//
		// `mode: 'hash'` because there is no server to mint a per-request nonce; SvelteKit
		// computes hashes for its own inline bootstrap and emits the policy as a
		// <meta http-equiv> in every generated document, which is all a static host can do.
		//
		// `connect-src` is the load-bearing directive: it pins the API origin, so a
		// compromised or mistyped build cannot exfiltrate to an arbitrary host. The origin
		// comes from PUBLIC_API_ORIGIN and is never hardcoded.
		//
		// Deliberately absent: `frame-ancestors`, `report-uri`, `sandbox`. Browsers ignore
		// all three in a <meta> policy, so they belong in Cloudflare response headers
		// (a `_headers` file) and are set at deployment, not here.
		csp: {
			mode: 'hash',
			directives: {
				'default-src': ['self'],
				'script-src': ['self'],
				'style-src': ['self'],
				'img-src': ['self', 'data:'],
				'font-src': ['self'],
				'connect-src': ['self', apiOrigin],
				'base-uri': ['self'],
				'form-action': ['self'],
				'object-src': ['none']
			}
		},

		// Reports are shared by URL and must survive a copy-paste. A canonical, stable
		// path shape avoids `/reports/x` and `/reports/x/` behaving differently.
		paths: {
			relative: false
		},

		alias: {
			$styles: 'src/lib/styles'
		}
	}
};

export default config;
