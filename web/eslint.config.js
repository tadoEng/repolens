import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

export default ts.config(
	{
		ignores: ['build/', '.svelte-kit/', 'node_modules/', 'test-results/', 'playwright-report/']
	},
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	// `prettier` and `svelte.configs.prettier` must stay last: they turn off every rule
	// that formatting owns, so that lint and format can never disagree with each other.
	prettier,
	...svelte.configs.prettier,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node }
		},
		rules: {
			// TypeScript already reports undefined identifiers, and the ESLint rule
			// misfires on type-only references.
			'no-undef': 'off',
			// A leading underscore marks an argument as intentionally unused — the same
			// convention tsconfig's noUnusedParameters follows.
			'@typescript-eslint/no-unused-vars': [
				'error',
				{ argsIgnorePattern: '^_', varsIgnorePattern: '^_' }
			]
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: {
			parserOptions: {
				parser: ts.parser,
				extraFileExtensions: ['.svelte'],
				svelteConfig
			}
		}
	}
);
