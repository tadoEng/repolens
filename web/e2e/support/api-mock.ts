import type { Page } from '@playwright/test';
import {
	COMPLETED_REPORT_FIXTURE,
	type AnalysisFixture,
	type AnalysisFixtureName,
	ANALYSIS_FIXTURES
} from '@repolens/api-client';

/**
 * Serving the `analysis-v1` fixtures to a real browser.
 *
 * Playwright's `page.route` rather than MSW: the app under test is a *production build*
 * served by `vite preview`, so there is no module graph to inject a mock transport into.
 * Interception at the network layer is the only thing that works against the artifact we
 * actually deploy — and it is also the only thing that exercises the CSP, since a request
 * blocked by `connect-src` never reaches an interceptor either.
 *
 * **Nothing here is a hand-written response body.** The fixtures come from
 * `@repolens/api-client`, which generates them from `contracts/fixtures/analysis-v1/*.json`
 * under a `satisfies` assertion. A DTO change breaks this file at compile time rather than
 * leaving a green end-to-end suite asserting against a shape the API no longer serves.
 */

/**
 * The origin the built bundle was compiled against.
 *
 * Must match the `PUBLIC_API_ORIGIN` used for the build, because that value is baked into
 * both the client's base URL and the CSP `connect-src` allowlist. The fallback mirrors
 * `svelte.config.js`'s development default so the two cannot drift.
 */
export const API_ORIGIN = process.env.PUBLIC_API_ORIGIN ?? 'http://localhost:8080';

/** Every fixture carries this analysis id; the URL and the body must agree. */
export const ANALYSIS_ID = COMPLETED_REPORT_FIXTURE.analysis.id;

/**
 * Cross-origin by construction — the API is a different origin from the static site — so
 * fulfilled responses need the header a real API would send. Without it the browser
 * rejects the response and the failure looks like an application bug.
 */
const CORS_HEADERS = { 'access-control-allow-origin': '*' };

/**
 * Intercept both endpoints for one fixture.
 *
 * The report path is a distinct pattern rather than a suffix of the analysis one: `*` does
 * not cross a `/` in Playwright's glob syntax, so `/analyses/:id` cannot swallow
 * `/analyses/:id/report` and registration order is not load-bearing.
 */
export async function serveFixture(page: Page, fixture: AnalysisFixture): Promise<void> {
	await page.route(`${API_ORIGIN}/api/v1/analyses/*`, (route) =>
		route.fulfill({ json: fixture.analysis, headers: CORS_HEADERS })
	);

	await page.route(`${API_ORIGIN}/api/v1/analyses/*/report`, (route) =>
		fixture.report
			? route.fulfill({ json: fixture.report, headers: CORS_HEADERS })
			: // 404 with an empty body, matching the shared MSW handler: the contract declares
				// no error schema for this path, and inventing one here is the drift the whole
				// pipeline exists to prevent.
				route.fulfill({ status: 404, body: '', headers: CORS_HEADERS })
	);
}

/** Intercept both endpoints for a named fixture. */
export function serveScenario(page: Page, name: AnalysisFixtureName): Promise<void> {
	return serveFixture(page, ANALYSIS_FIXTURES[name]);
}

/**
 * Record every Content Security Policy violation the page reports.
 *
 * Standing guard over the LOC bars. `--proportion` reaches CSS through `style.setProperty`,
 * because an inline `style` attribute is governed by `style-src-attr` — which falls back to
 * our `style-src 'self'` and blocks it. That failure is invisible locally and silent in
 * production: every bar would render at zero width while the numbers still looked right.
 */
export function recordCspViolations(page: Page): Promise<void> {
	// `addInitScript` runs before any page script, so nothing can violate the policy before
	// the listener exists.
	return page.addInitScript(() => {
		const store: string[] = [];
		(window as unknown as { __cspViolations: string[] }).__cspViolations = store;
		document.addEventListener('securitypolicyviolation', (event) => {
			store.push(`${event.violatedDirective} ${event.blockedURI}`);
		});
	});
}

/** Read back the violations recorded by `collectCspViolations`. */
export function readCspViolations(page: Page): Promise<string[]> {
	return page.evaluate(
		() => (window as unknown as { __cspViolations?: string[] }).__cspViolations ?? []
	);
}
