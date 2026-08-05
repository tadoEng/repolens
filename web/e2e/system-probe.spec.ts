import { Buffer } from 'node:buffer';

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Locator, type Page } from '@playwright/test';
import {
	HEALTHY_PROBE,
	createMockFetch,
	systemProbeHealthy,
	type RequestHandler
} from '@repolens/msw';

/**
 * The walking-skeleton probe, on the real deployment shape.
 *
 * There is no API behind `vite preview`, so the probe's responses are served by
 * Playwright's own network interception — driven by the shared MSW handlers rather than by
 * literals written here, so these specs and the component tests cannot disagree about what
 * the API returns. MSW's Service Worker integration is deliberately *not* wired up: the
 * worker script would have to live in `web/static/`, which is copied wholesale into
 * `build/` and shipped.
 *
 * What that leaves unproven is real connectivity — that the deployed origin, the CSP
 * `connect-src` allowlist and Cloud Run actually agree. Only a deploy can show that, which
 * is why the probe exists in the first place.
 */

const ANALYSIS_ID = '00000000-0000-7000-8000-000000000000';
const ROUTES = ['/', `/analyses/${ANALYSIS_ID}`, `/reports/${ANALYSIS_ID}`];

const PROBE_ROUTE = '**/api/v1/system/probe';

/** The probe's live region, addressed through the footer landmark rather than a class. */
function probe(page: Page): Locator {
	return page.getByRole('contentinfo').locator('[aria-live]');
}

/** Answer the probe request from MSW handlers, without a Service Worker. */
async function serveProbe(page: Page, ...handlers: RequestHandler[]): Promise<void> {
	const mockFetch = createMockFetch(handlers);

	await page.route(PROBE_ROUTE, async (route) => {
		const request = route.request();

		try {
			const response = await mockFetch(new Request(request.url(), { method: request.method() }));
			await route.fulfill({
				status: response.status,
				headers: Object.fromEntries(response.headers),
				body: Buffer.from(await response.arrayBuffer())
			});
		} catch {
			// `createMockFetch` rejects where a browser's `fetch` would: a transport failure.
			await route.abort('failed');
		}
	});
}

for (const route of ROUTES) {
	test.describe(`route ${route}`, () => {
		test('renders the system probe in the footer', async ({ page }) => {
			await serveProbe(page, systemProbeHealthy());
			await page.goto(route);

			// In the footer on *every* route is the point: one request path exercised
			// everywhere, without spending a fourth route on a diagnostic.
			await expect(probe(page)).toContainText('API OK');
			await expect(probe(page)).toContainText('database OK');
			await expect(probe(page)).toContainText(HEALTHY_PROBE.build_sha.slice(0, 7));
			await expect(probe(page)).toContainText('schema v1');
		});

		test('has no detectable accessibility violations with the probe resolved', async ({ page }) => {
			await serveProbe(page, systemProbeHealthy());
			await page.goto(route);
			await expect(probe(page)).toContainText('API OK');

			// The foundation suite already scans these routes, but only ever with the probe
			// in its failed state. A live region that is fine while empty can still be wrong
			// once it has content, so the resolved state needs its own scan.
			const results = await new AxeBuilder({ page })
				.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
				.analyze();
			expect(results.violations).toEqual([]);
		});
	});
}

test('the page is usable while the probe is still loading', async ({ page }) => {
	// Held open rather than delayed by a timer: the assertions below have to run while the
	// request is genuinely in flight, and a race against a stopwatch would be flaky.
	let release!: () => void;
	const held = new Promise<void>((resolve) => {
		release = resolve;
	});

	await page.route(PROBE_ROUTE, async (route) => {
		await held;
		await route.fulfill({ json: HEALTHY_PROBE });
	});

	await page.goto('/');

	await expect(probe(page)).toContainText('checking…');

	// The probe is a diagnostic, not a gate: content, navigation and keyboard entry all
	// have to work before anything has answered.
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(page.getByRole('link', { name: 'RepoLens' })).toBeVisible();
	await page.keyboard.press('Tab');
	await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused();

	release();
	await expect(probe(page)).toContainText('API OK');
});

test('has no detectable accessibility violations while the probe is loading', async ({ page }) => {
	let release!: () => void;
	const held = new Promise<void>((resolve) => {
		release = resolve;
	});

	await page.route(PROBE_ROUTE, async (route) => {
		await held;
		await route.fulfill({ json: HEALTHY_PROBE });
	});

	await page.goto('/');
	await expect(probe(page)).toContainText('checking…');

	const results = await new AxeBuilder({ page })
		.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
		.analyze();
	expect(results.violations).toEqual([]);

	release();
});
