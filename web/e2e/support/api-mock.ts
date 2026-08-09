import type { Page } from '@playwright/test';
import {
	ADMIN_FIXTURES,
	COMPLETED_REPORT_FIXTURE,
	type AdminFixtureName,
	type AnalysisFixture,
	type AnalysisFixtureName,
	type ApiError,
	type ErrorCode,
	type SystemProbeResponse,
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

const ADMIN_OVERVIEW_PATH = '/api/v1/admin/overview';
const SYSTEM_PROBE_PATH = '/api/v1/system/probe';

/**
 * The probe body `/admin` reads its deployment facts from.
 *
 * Typed as the generated DTO, so a probe field that changes shape breaks this at compile
 * time. The values are a healthy deployment: the point of the admin baselines is the
 * operational sections, not the probe's own failure modes, which `system-probe.spec.ts`
 * already owns.
 */
const HEALTHY_PROBE: SystemProbeResponse = {
	api: 'OK',
	database: 'OK',
	build_sha: '0584a2df65968a4e9e6859ef46bbed430408a3f1',
	schema_version: 1
};

/**
 * Serve one `admin-v1` fixture, plus the probe the page reads alongside it.
 *
 * The body comes from `@repolens/api-client`, generated from
 * `contracts/fixtures/admin-v1/*.json` under a `satisfies` assertion — so a change to the
 * Rust `AdminOverview` breaks this file at compile time rather than leaving a green suite
 * asserting against a shape the API no longer serves. There is no hand-written snapshot
 * here and there must not be one.
 */
export async function serveAdminOverview(page: Page, name: AdminFixtureName): Promise<void> {
	await page.route(`${API_ORIGIN}${ADMIN_OVERVIEW_PATH}`, (route) =>
		route.fulfill({ json: ADMIN_FIXTURES[name], headers: CORS_HEADERS })
	);
	await page.route(`${API_ORIGIN}${SYSTEM_PROBE_PATH}`, (route) =>
		route.fulfill({ json: HEALTHY_PROBE, headers: CORS_HEADERS })
	);
}

/**
 * Refuse the operational snapshot with one error code.
 *
 * The envelope is built here rather than read from a fixture, and that is deliberate:
 * `admin-v1` publishes no refusal fixtures, because a refusal's wire shape is `ApiError` —
 * already published, already generated, already rendered everywhere else. What is *not*
 * hand-written is the shape: `ApiError` and `ErrorCode` come from the generated client, so
 * a code removed from the Rust enum fails this file at compile time.
 *
 * The message is arbitrary on purpose. **No behavioural test may depend on it.** The
 * contract the page implements is `ErrorCode → remedy`, so these strings exist only to
 * prove the server's sentence is what gets rendered — never to be matched against.
 */
export async function refuseAdminOverview(
	page: Page,
	status: number,
	code: ErrorCode,
	message: string
): Promise<void> {
	const body: ApiError = { code, message, retry_after_seconds: null };

	await page.route(`${API_ORIGIN}${ADMIN_OVERVIEW_PATH}`, (route) =>
		route.fulfill({ status, json: body, headers: CORS_HEADERS })
	);
	await page.route(`${API_ORIGIN}${SYSTEM_PROBE_PATH}`, (route) =>
		route.fulfill({ json: HEALTHY_PROBE, headers: CORS_HEADERS })
	);
}

/** One request the page made to the API. */
export interface ApiRequest {
	readonly method: string;
	readonly url: string;
}

/**
 * Record every request the page sends to the API origin.
 *
 * Standing guard over the mutation boundary. The report and progress routes are read-only
 * surfaces: a `POST`, `PUT`, `PATCH` or `DELETE` from either of them is an authenticated
 * operation the contract has not defined yet, and the whole failure mode is that it
 * *succeeds* — starting duplicate paid work rather than raising an error anyone would see.
 * Asserting on the wire is the only place that distinguishes "no button" from "no request".
 *
 * Registered before `page.goto`, so nothing can slip through before the listener exists.
 */
export function recordApiRequests(page: Page): () => ApiRequest[] {
	const seen: ApiRequest[] = [];

	page.on('request', (request) => {
		if (request.url().startsWith(API_ORIGIN)) {
			seen.push({ method: request.method(), url: request.url() });
		}
	});

	return () => [...seen];
}

/**
 * Record every Content Security Policy violation the page reports.
 *
 * Standing guard over the LOC bars. `--proportion` reaches CSS through `style.setProperty`,
 * because an inline `style` attribute is governed by `style-src-attr` — which falls back to
 * our `style-src 'self'` and blocks it. That failure is invisible locally and silent in
 * production: every bar would render at zero width while the numbers still looked right.
 *
 * The assertion is about *this* policy in *these* browsers, not a general claim that CSP
 * leaves CSSOM alone — it does gate some CSSOM parsing entry points. What the suite
 * establishes is the narrower fact the implementation depends on, and it establishes it by
 * measuring the rendered bar rather than by reasoning about the spec.
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
