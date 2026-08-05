import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end configuration.
 *
 * Runs against `vite preview` on a production build, not the dev server — the dev server
 * has different module graph, routing, and header behaviour, so testing it would test
 * something we never deploy.
 *
 * **`vite preview` is still not Cloudflare, and that limit is worth being precise about.**
 * SvelteKit's preview server *renders* responses, so it delivers the CSP as an HTTP header
 * rather than as the `<meta http-equiv>` tag a static host has to rely on, and it resolves
 * unmatched routes itself instead of via `not_found_handling`. It is a faithful harness for
 * routing, focus, and accessibility; it is not evidence about static hosting. Assertions
 * about the deployed artifact therefore read `build/` directly (see foundation.spec.ts),
 * and the real Cloudflare behaviour is verified at deploy time (issue #11).
 *
 * Requires a one-time browser download: `pnpm exec playwright install chromium`.
 */
export default defineConfig({
	testDir: 'e2e',
	// `real-api.spec.ts` needs a real Axum server and lives in
	// playwright.integration.config.ts. Excluded here so the ordinary frontend loop needs
	// no Rust toolchain and stays fast.
	testIgnore: 'real-api.spec.ts',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	reporter: process.env.CI ? 'github' : 'list',

	webServer: {
		command: 'pnpm run build && pnpm run preview --port 4173 --strictPort',
		url: 'http://localhost:4173',
		reuseExistingServer: !process.env.CI,
		timeout: 120_000
	},

	use: {
		baseURL: 'http://localhost:4173',
		trace: 'on-first-retry'
	},

	projects: [
		{ name: 'chromium', use: { ...devices['Desktop Chrome'] } },

		// The plan requires verification at 360 / 768 / 1280px with no horizontal body
		// scroll. Two extra viewport projects are cheaper than three bespoke tests.
		{
			name: 'mobile-360',
			use: { ...devices['Desktop Chrome'], viewport: { width: 360, height: 800 } }
		},
		{
			name: 'tablet-768',
			use: { ...devices['Desktop Chrome'], viewport: { width: 768, height: 1024 } }
		}
	]
});
