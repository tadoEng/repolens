/**
 * Shared MSW request handlers for the RepoLens API.
 *
 * One source of truth for mocked API behaviour, consumed by three places at once:
 * component tests (Vitest browser mode), Playwright end-to-end runs, and a `dev:msw`
 * mode that lets the frontend be developed against a working API without the backend
 * running.
 *
 * **There are no handlers yet, and that is deliberate.**
 *
 * Handlers are typed against the generated schema in `@repolens/api-client` and return
 * the executable fixtures that land in `contracts/fixtures/analysis-v1/` at issue #14.
 * Writing a handler now would mean inventing the response shape it returns — the exact
 * failure the fixtures exist to prevent. An invented mock is worse than no mock: it lets
 * a UI be built, reviewed, and merged against a contract that never existed, and the
 * mismatch only surfaces against the real API.
 *
 * When #14 lands, this file grows handlers that read those fixtures. It does not grow
 * hand-written response literals.
 */

import type { RequestHandler } from 'msw';

export interface HandlerOptions {
	/**
	 * Origin the handlers intercept, matching `PUBLIC_API_ORIGIN`.
	 *
	 * Passed in rather than read from the environment so a test can point handlers at a
	 * different origin without mutating process state.
	 */
	apiOrigin?: string;
}

/**
 * Build the handler set for a given API origin.
 *
 * A factory rather than a constant, because Playwright and the browser-mode component
 * tests do not necessarily run against the same origin, and a module-level array would
 * bake one in at import time.
 */
export function createHandlers(_options: HandlerOptions = {}): RequestHandler[] {
	return [];
}

/** Default handler set, for consumers that do not need to override the origin. */
export const handlers: RequestHandler[] = createHandlers();
