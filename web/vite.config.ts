import { sveltekit } from '@sveltejs/kit/vite';
import { playwright } from '@vitest/browser-playwright';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit()],

	/*
	 * Environment files live at the repository root, not in `web/`.
	 *
	 * The repository keeps every real value in a single git-ignored root
	 * `.env.local` — the Rust binaries load it, `.env.example` documents it, and
	 * `AGENTS.md` states it. Without `envDir` Vite would look in `web/` instead,
	 * find nothing, and `$env/static/public` would resolve every `PUBLIC_*`
	 * variable to empty.
	 *
	 * That failure is quiet and specifically dangerous here: absent Firebase
	 * configuration deliberately degrades into read-only mode, so a developer
	 * with a correctly populated root `.env.local` would see "sign-in is not
	 * available in this deployment" and reasonably conclude the feature was
	 * broken rather than that Vite was reading the wrong directory.
	 *
	 * `svelte.config.js` reads the same directory through `loadEnv` so the CSP
	 * it bakes and the values the app sees cannot disagree.
	 */
	envDir: '..',

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
