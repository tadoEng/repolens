import { expect, test } from '@playwright/test';

/**
 * The only test in this suite that talks to a **real Axum server**.
 *
 * Everything else mocks the probe with `page.route`, which proves how the component
 * renders a response but cannot prove the response is reachable. This exercises the chain
 * that actually breaks in production:
 *
 *     Svelte → generated openapi-fetch client → browser CORS enforcement
 *           → real Axum server → real probe response
 *
 * A mocked request never leaves the page, so it never consults the CSP `connect-src`
 * allowlist and never triggers a preflight or an origin comparison. Those are exactly the
 * failures that only appear after deploy, which is the class of bug the walking skeleton
 * exists to catch early.
 *
 * **No database and no secret.** The server is started without `DATABASE_URL`, so the
 * probe honestly reports `UNAVAILABLE` and the frontend must render the null
 * `schema_version`. That makes this runnable in CI on a fork, where Neon credentials do
 * not exist and should not be required.
 *
 * What it still cannot prove: Cloudflare's own static hosting and `not_found_handling`.
 * `vite preview` renders responses and resolves unmatched routes itself. That remains the
 * deployment half of #11.
 */

const PROBE_TEXT = /API OK/;

test.describe('real API, no mocking', () => {
	test('the browser reaches a real Axum server through the generated client', async ({ page }) => {
		// Deliberately no page.route(): if anything intercepts, this test is worthless.
		const probeRequests: string[] = [];
		page.on('request', (request) => {
			if (request.url().includes('/api/v1/system/probe')) {
				probeRequests.push(request.url());
			}
		});

		await page.goto('/');

		const probe = page.locator('.probe');
		await expect(probe).toContainText(PROBE_TEXT, { timeout: 15_000 });

		// With no DATABASE_URL the honest answer is UNAVAILABLE, and an unknown schema
		// version must not render as 0 — the distinction the nullable field exists for.
		await expect(probe).toContainText('database UNAVAILABLE');
		await expect(probe).toContainText('schema unknown');
		await expect(probe).not.toContainText('schema v0');

		// Proves the request genuinely crossed the network rather than being served from
		// a mock or a cached module.
		expect(probeRequests.length).toBeGreaterThan(0);
		expect(probeRequests[0]).toContain('/api/v1/system/probe');
	});

	test('the response survives browser CORS enforcement', async ({ page }) => {
		// A cross-origin response the browser rejects never reaches JavaScript, so the
		// component would fall into its `unreachable` branch. Asserting the *rendered*
		// success text therefore asserts that CORS was accepted end to end — a check no
		// amount of curl can make, because curl does not enforce CORS.
		const failures: string[] = [];
		page.on('requestfailed', (request) => {
			if (request.url().includes('/api/v1/system/probe')) {
				failures.push(request.failure()?.errorText ?? 'unknown');
			}
		});

		await page.goto('/');
		await expect(page.locator('.probe')).toContainText(PROBE_TEXT, { timeout: 15_000 });

		expect(
			failures,
			'a CORS or CSP rejection surfaces here before it surfaces as a blank page in production'
		).toEqual([]);
	});
});
