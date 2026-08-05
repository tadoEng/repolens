/**
 * Thin, hand-written wrapper around `openapi-fetch`.
 *
 * This is the only hand-written file in the package that touches the network. Everything
 * it is typed against comes from the generated `schema.ts`, so an endpoint that does not
 * exist in the OpenAPI document cannot be called.
 *
 * Two rules govern this package:
 *
 *  1. **No Svelte, no SvelteKit.** It is a contract package, not a UI one. That means no
 *     `$env/static/public`, no stores, no `$app/*`. The consuming app injects the origin;
 *     see `resolveApiOrigin` for the fallback used outside a SvelteKit build.
 *  2. **The API origin is never hardcoded.** Environment-specific origins must be explicit,
 *     and the same value is baked into the CSP `connect-src` allowlist at build time, so a
 *     literal here would be a policy violation as well as a bug.
 */

import createClient from 'openapi-fetch';
import type { Client, ClientOptions } from 'openapi-fetch';

import type { paths } from './schema';

/**
 * The single environment variable naming the API origin.
 *
 * `PUBLIC_` is SvelteKit's enforced boundary for values that are safe to ship to the
 * browser. An origin is not a secret — but keeping the name in one place means the CSP
 * allowlist, the build, and this client cannot drift apart.
 */
export const API_ORIGIN_ENV_VAR = 'PUBLIC_API_ORIGIN';

export type RepoLensClient = Client<paths>;

export interface RepoLensClientOptions extends Omit<ClientOptions, 'baseUrl'> {
	/**
	 * API origin, e.g. `https://api.repolens.example`.
	 *
	 * Inside the SvelteKit app this is passed explicitly from `$env/static/public`, which
	 * keeps the SvelteKit-specific import on the app side of the boundary. Omit it and the
	 * client falls back to reading `PUBLIC_API_ORIGIN` from the ambient environment, which
	 * is what makes the package usable from Node scripts and tests too.
	 */
	baseUrl?: string;
}

function readEnv(name: string): string | undefined {
	// `import.meta.env` covers Vite/Vitest; `process.env` covers plain Node. Both are
	// probed defensively because this package must run in either without a bundler shim.
	const meta = import.meta as unknown as {
		env?: Record<string, string | undefined>;
	};
	const fromImportMeta = meta.env?.[name];
	if (fromImportMeta) return fromImportMeta;

	const runtime = globalThis as unknown as {
		process?: { env?: Record<string, string | undefined> };
	};
	return runtime.process?.env?.[name];
}

/**
 * Normalize an origin so `baseUrl + '/api/v1/...'` never produces a double slash and never
 * silently drops a path prefix.
 *
 * Deliberately not a regex. `/\/+$/` is a polynomial ReDoS (CodeQL `js/polynomial-redos`):
 * an anchored `+` over a repeated character makes the engine retry from every position, so
 * a string of many trailing slashes costs quadratic time. Scanning backwards is linear and
 * needs no backtracking at all.
 *
 * The input here is build-time configuration rather than user input, so this was never
 * reachable by an attacker — but a pattern that is only safe because of where it happens to
 * sit is one copy-paste away from somewhere it is not, and this repository's own product is
 * finding exactly that class of problem.
 */
function normalizeOrigin(origin: string): string {
	const SLASH = 47; // '/'
	let end = origin.length;
	while (end > 0 && origin.charCodeAt(end - 1) === SLASH) {
		end -= 1;
	}
	return origin.slice(0, end);
}

/**
 * Resolve the API origin, preferring an explicitly supplied value.
 *
 * Throws rather than defaulting. A client that quietly falls back to the current page
 * origin produces same-origin requests that fail in a way that looks like a CORS or
 * network problem, and the actual cause — a missing environment variable — never surfaces.
 */
export function resolveApiOrigin(explicit?: string): string {
	const candidate = explicit ?? readEnv(API_ORIGIN_ENV_VAR);

	if (!candidate) {
		throw new Error(
			`${API_ORIGIN_ENV_VAR} is not set. The RepoLens API origin must be explicit — ` +
				'set it in a git-ignored .env.local for local development, or in the deployment ' +
				'environment. It is also baked into the CSP connect-src allowlist at build time.'
		);
	}

	return normalizeOrigin(candidate);
}

/**
 * Create a typed RepoLens API client.
 *
 * Deliberately not a module-level singleton: a singleton would have to resolve the origin
 * at import time, which turns a missing environment variable into an import-time crash in
 * unrelated tests. Callers own the instance's lifetime.
 */
export function createRepoLensClient(options: RepoLensClientOptions = {}): RepoLensClient {
	const { baseUrl, ...rest } = options;

	return createClient<paths>({
		baseUrl: resolveApiOrigin(baseUrl),
		...rest
	});
}
