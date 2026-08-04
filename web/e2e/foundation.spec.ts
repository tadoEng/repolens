import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

/**
 * Foundation smoke tests.
 *
 * These assert properties of the *deployment shape*, not of any UI — because the UI is
 * blocked on the API contract but the deployment shape is settled now and is exactly the
 * kind of thing that regresses silently.
 *
 * The twelve required user-flow tests arrive with the screens they exercise.
 */

const ANALYSIS_ID = '00000000-0000-7000-8000-000000000000';
const ROUTES = ['/', `/analyses/${ANALYSIS_ID}`, `/reports/${ANALYSIS_ID}`];

for (const route of ROUTES) {
	test.describe(`route ${route}`, () => {
		test('renders with exactly one h1', async ({ page }) => {
			await page.goto(route);
			await expect(page.locator('h1')).toHaveCount(1);
		});

		test('has no horizontal body scroll', async ({ page }) => {
			await page.goto(route);
			const overflows = await page.evaluate(
				() => document.documentElement.scrollWidth > document.documentElement.clientWidth
			);
			expect(overflows).toBe(false);
		});

		test('has no detectable accessibility violations', async ({ page }) => {
			await page.goto(route);
			const results = await new AxeBuilder({ page })
				.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
				.analyze();
			expect(results.violations).toEqual([]);
		});
	});
}

test('a nested route is served with 200 and the app shell on direct navigation', async ({
	page
}) => {
	// The client half of the Cloudflare `not_found_handling: "single-page-application"`
	// contract: a deep link that matches no built asset must still answer 200 with a
	// working application shell, not a 404. The *artifact* half — that the fallback
	// document is named index.html — is asserted below against `build/`.
	const response = await page.goto(`/reports/${ANALYSIS_ID}`);
	expect(response?.status()).toBe(200);
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
});

test('the CSP pins the API origin in connect-src', async ({ page }) => {
	const response = await page.goto('/');

	// SvelteKit delivers the policy by whichever transport the response allows: an HTTP
	// header when something is rendering the response (the preview server does), and a
	// <meta http-equiv> on prerendered/fallback documents — which is what Cloudflare
	// serves, since a static host cannot add the header for us. Accept either, and assert
	// the directive that actually matters.
	const header = response?.headers()['content-security-policy'];
	// Read the DOM directly rather than via a locator: a locator would wait out the full
	// timeout for a tag that legitimately does not exist on this transport.
	const meta = await page.evaluate(
		() =>
			document
				.querySelector('meta[http-equiv="content-security-policy"]')
				?.getAttribute('content') ?? null
	);

	const policy = header ?? meta;

	expect(policy, 'no Content-Security-Policy was delivered').toBeTruthy();
	expect(policy).toContain("default-src 'self'");
	expect(policy).toContain('connect-src');
	// The origin is injected from PUBLIC_API_ORIGIN, never hardcoded, so assert the shape
	// rather than a literal value.
	expect(policy).toMatch(/connect-src 'self' https?:\/\/\S+/);
});

test.describe('built artifact', () => {
	// These read `build/` straight off disk rather than going through a server, because
	// the preview server is not Cloudflare: it renders responses, so it can paper over
	// exactly the properties that make the output deployable as plain static assets.
	const buildDir = fileURLToPath(new URL('../build/', import.meta.url));

	test('the fallback document is index.html and carries the meta CSP', () => {
		const html = readFileSync(`${buildDir}index.html`, 'utf8');

		expect(html).toContain('<meta http-equiv="content-security-policy"');
		expect(html).toContain("connect-src 'self'");
		// The SPA bootstrap, i.e. the fallback really is the app shell.
		expect(html).toContain('kit.start(app, element)');
	});

	test('no server bundle is emitted into the deployed output', () => {
		// adapter-static must not have written anything server-shaped. If this ever fails,
		// the hosting assumption has changed and the Cloudflare deploy is no longer valid.
		const html = readFileSync(`${buildDir}index.html`, 'utf8');
		expect(html).not.toContain('data-sveltekit-hydrate');

		for (const forbidden of ['index.js', 'handler.js', 'server.js', 'manifest.js']) {
			expect(() => readFileSync(`${buildDir}${forbidden}`, 'utf8')).toThrow();
		}
	});
});
