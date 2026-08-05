import { defineConfig } from 'vitest/config';

export default defineConfig({
	test: {
		// Node, not browser: this package is a contract gate. It reads the OpenAPI document
		// off disk and runs a code generator — nothing here touches the DOM. (The *app's*
		// tests run in a real browser; see web/vite.config.ts.)
		environment: 'node',
		include: ['**/*.test.ts'],
		exclude: ['node_modules/**']
	}
});
