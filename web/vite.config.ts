import { sveltekit } from '@sveltejs/kit/vite';
import { playwright } from '@vitest/browser-playwright';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit()],

	test: {
		// Browser mode, not jsdom (plan §3.5). jsdom misreports focus order and computed
		// ARIA, which are precisely the things the RepoLens accessibility contract asserts;
		// a green jsdom suite would be actively misleading.
		//
		// Requires a one-time browser download: `pnpm exec playwright install chromium`.
		browser: {
			enabled: true,
			headless: true,
			provider: playwright(),
			instances: [{ browser: 'chromium' }]
		},
		include: ['src/**/*.{test,spec}.{js,ts}'],
		// Playwright owns `e2e/`; Vitest must not try to run those specs.
		exclude: ['e2e/**', 'node_modules/**', 'build/**', '.svelte-kit/**']
	}
});
