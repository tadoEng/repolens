import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { loadEnv } from 'vite';

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

/*
 * The repository's single root `.env.local`, read the same way Vite reads it.
 *
 * `vite.config.ts` sets `envDir: '..'`, so the application resolves `PUBLIC_*`
 * from the repository root. This file runs *before* that and reads
 * `process.env` directly, so without loading the same files it would decide the
 * CSP from a different set of values than the app is built with — permitting
 * hosts the bundle never contacts, or omitting ones it does.
 *
 * `loadEnv` comes from Vite itself rather than a second dotenv dependency, and
 * the merge order matches the Rust side: an explicitly exported variable wins
 * over a file, so `PUBLIC_API_ORIGIN=… pnpm build` still overrides.
 */
const rootEnv = loadEnv(process.env.NODE_ENV ?? 'development', '..', 'PUBLIC_');
const fromEnv = (name) => process.env[name] ?? rootEnv[name] ?? undefined;

// `vite build` sets NODE_ENV=production; `vite dev` does not.
const isProductionBuild = process.env.NODE_ENV === 'production';
const configuredOrigin = fromEnv('PUBLIC_API_ORIGIN');

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

/*
 * Firebase browser configuration (#13).
 *
 * Every one of these is public by design — the `PUBLIC_` prefix is the whole
 * statement. A Firebase web API key identifies a project; it authorizes nothing.
 * What actually gates creation is the ID token this API verifies server-side.
 *
 * Defaulted to empty rather than required, unlike `PUBLIC_API_ORIGIN`. A
 * deployment without a Firebase project is a **read-only demo**: reports are
 * public, the submit control says sign-in is unavailable, and the API refuses
 * creation anyway. Making these mandatory would turn that honest configuration
 * into a build failure.
 */
const firebase = {
	apiKey: fromEnv('PUBLIC_FIREBASE_API_KEY') ?? '',
	authDomain: fromEnv('PUBLIC_FIREBASE_AUTH_DOMAIN') ?? '',
	projectId: fromEnv('PUBLIC_FIREBASE_PROJECT_ID') ?? '',
	appId: fromEnv('PUBLIC_FIREBASE_APP_ID') ?? ''
};

for (const [key, value] of Object.entries(firebase)) {
	const name = `PUBLIC_FIREBASE_${key.replace(/[A-Z]/g, (c) => `_${c}`).toUpperCase()}`;
	// `$env/static/public` inlines whatever is on process.env at build time, so
	// setting the empty string here is what lets the app import these
	// unconditionally and decide at runtime whether sign-in is available.
	process.env[name] = value;
}

const signInConfigured = Boolean(firebase.apiKey && firebase.authDomain);

if (!signInConfigured) {
	console.warn(
		'[repolens] Firebase is not configured; the build will ship without sign-in and the ' +
			'submit control will say so. Reports remain publicly viewable.'
	);
}

/*
 * What Firebase Auth needs at the network layer, and nothing more.
 *
 * Added only when sign-in is configured, so a read-only build keeps the tighter
 * policy rather than permitting hosts it will never contact.
 *
 * `signInWithPopup` opens `https://<authDomain>/__/auth/handler`, which is a
 * separate browsing context and not governed by this document's CSP. Everything
 * else the flow needs *is* governed by it:
 *
 *   - `script-src` — the SDK loads Google's gapi helper from `apis.google.com`
 *     into this document before it can open anything;
 *   - `connect-src` — Identity Toolkit and the secure-token service;
 *   - `frame-src` — the helper iframe on the auth domain, and the account
 *     chooser on `accounts.google.com`.
 *
 * `script-src` was missed on the first pass and cost a deployed, broken sign-in
 * button: the helper was blocked, the flow never started, and the UI reported
 * only "Sign-in did not complete." Nothing catches this before deploy — the
 * e2e suite never signs in, and the CSP is inert until a real browser applies
 * it — so the list above is the check, and it is written out rather than
 * remembered.
 */
const authConnectSrc = signInConfigured
	? [
			'https://identitytoolkit.googleapis.com',
			'https://securetoken.googleapis.com',
			`https://${firebase.authDomain}`
		]
	: [];
const authScriptSrc = signInConfigured ? ['https://apis.google.com'] : [];
const authFrameSrc = signInConfigured
	? [`https://${firebase.authDomain}`, 'https://accounts.google.com']
	: [];

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
				'script-src': ['self', ...authScriptSrc],
				'style-src': ['self'],
				'img-src': ['self', 'data:'],
				'font-src': ['self'],
				'connect-src': ['self', apiOrigin, ...authConnectSrc],
				'frame-src': authFrameSrc.length > 0 ? authFrameSrc : ['none'],
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
