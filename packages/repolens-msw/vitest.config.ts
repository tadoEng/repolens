import { defineConfig } from 'vitest/config';

export default defineConfig({
	test: {
		// Node, not browser. The handlers themselves are browser-safe — they import fixture
		// data from `@repolens/api-client`, never `node:fs` — but resolving them through
		// `createMockFetch` needs no DOM, and a browser runner here would only slow the gate
		// down. (The *app's* component tests run these same handlers in a real browser; see
		// web/vite.config.ts.)
		environment: 'node',
		include: ['**/*.test.ts'],
		exclude: ['node_modules/**']
	}
});
