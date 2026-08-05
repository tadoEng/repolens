/**
 * The fixture type-check.
 *
 * `contracts/fixtures/analysis-v1/*.json` is generated from the Rust DTOs and is the
 * authoritative wire contract. This test binds it into TypeScript by generating
 * `src/fixtures.ts`, where every fixture is emitted as a literal under
 * `satisfies AnalysisFixture`. Two failures follow, and both are wanted:
 *
 *   - A fixture whose shape the frontend cannot consume fails **`pnpm -r check`**, not
 *     merely an assertion here. `satisfies` on a fresh object literal checks missing
 *     fields, wrong types, unrecognised enum values *and* excess properties — a runtime
 *     `JSON.parse` in a test checks none of them, because `JSON.parse` returns `any`.
 *   - A fixture edited without regenerating fails **`pnpm -r test`**, below.
 *
 * # Why generate a module instead of importing the JSON
 *
 * TypeScript widens string literals in JSON modules: `"state": "QUEUED"` is typed `string`,
 * not `"QUEUED"`. Verified against this workspace's TypeScript — the direct import fails
 * with `Type 'string' is not assignable to type '"QUEUED"'`. So a JSON import can never
 * check an enum value, which is precisely the check the unknown-variant policy depends on.
 *
 * The generated module is a *binding*, not a second copy. Nobody authors it, and this test
 * fails the moment it stops matching the JSON — the same arrangement `schema.test.ts` uses
 * for `src/schema.ts`, and the reason MSW handlers can import fixture data without ever
 * re-declaring it.
 *
 * Regenerate with:
 *
 *     pnpm --filter @repolens/api-client fixtures:update
 */

import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, test } from 'vitest';

/** Owned by the API side of the workspace (#14); this path is the handshake. */
const FIXTURE_DIRECTORY = fileURLToPath(
	new URL('../../contracts/fixtures/analysis-v1/', import.meta.url)
);

const FIXTURES_MODULE_PATH = fileURLToPath(new URL('./src/fixtures.ts', import.meta.url));

/**
 * The scenarios the contract is required to cover.
 *
 * Listed rather than derived so that *deleting* a fixture fails too. A generated module
 * regenerated from an emptied directory would otherwise match its own snapshot perfectly
 * and prove nothing.
 */
const REQUIRED_FIXTURES = [
	'completed-report',
	'failed-permanent',
	'failed-retriable',
	'loc-unavailable',
	'queued',
	'resolving'
] as const;

const GENERATED_HEADER = `/**
 * GENERATED FILE — DO NOT EDIT BY HAND.
 *
 * The executable \`analysis-v1\` fixtures, bound to TypeScript. Produced from
 * \`contracts/fixtures/analysis-v1/*.json\` — themselves generated from the Rust DTOs — by:
 *
 *     pnpm --filter @repolens/api-client fixtures:update
 *
 * Each fixture is emitted as a literal under \`satisfies AnalysisFixture\` so the compiler
 * checks it against the generated schema. A JSON import could not: TypeScript widens string
 * literals in JSON modules, which would type every enum value as \`string\` and silently
 * retire the check that matters most.
 *
 * This is a binding, not a copy. \`fixtures.test.ts\` regenerates it and fails if the result
 * differs, so fixture content is authored in exactly one place — the JSON.
 */

import type { AnalysisFixture } from './contract';
`;

/** Git may check these files out with CRLF on Windows; the comparison must not care. */
function normalizeNewlines(source: string): string {
	return source.replace(/\r\n/g, '\n');
}

const IDENTIFIER = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

/**
 * Print a parsed JSON value as TypeScript source.
 *
 * `JSON.stringify` is used for every scalar so that escaping — newlines inside evidence
 * excerpts, quotes inside Cargo manifests — is handled by the same routine that produced
 * the fixture, rather than by a hand-rolled escape table that is wrong for one character.
 */
function printValue(value: unknown, indent: string): string {
	if (value === null || typeof value !== 'object') {
		return JSON.stringify(value);
	}

	const inner = `${indent}\t`;

	if (Array.isArray(value)) {
		if (value.length === 0) return '[]';
		const items = value.map((item) => `${inner}${printValue(item, inner)}`);
		return `[\n${items.join(',\n')}\n${indent}]`;
	}

	const entries = Object.entries(value);
	if (entries.length === 0) return '{}';
	const properties = entries.map(([key, item]) => {
		const name = IDENTIFIER.test(key) ? key : JSON.stringify(key);
		return `${inner}${name}: ${printValue(item, inner)}`;
	});
	return `{\n${properties.join(',\n')}\n${indent}}`;
}

/** `completed-report` → `COMPLETED_REPORT_FIXTURE`. */
function constantName(fixtureName: string): string {
	return `${fixtureName.replace(/-/g, '_').toUpperCase()}_FIXTURE`;
}

function readFixtureNames(): string[] {
	return readdirSync(FIXTURE_DIRECTORY)
		.filter((entry) => entry.endsWith('.json'))
		.map((entry) => entry.slice(0, -'.json'.length))
		.sort(); // readdir order is filesystem-dependent; the generated file must not be.
}

function generateFixturesModule(fixtureNames: readonly string[]): string {
	const declarations = fixtureNames.map((name) => {
		const parsed: unknown = JSON.parse(
			readFileSync(`${FIXTURE_DIRECTORY}${name}.json`, 'utf8')
		);
		return `/** Fixture \`${name}.json\`. */\nexport const ${constantName(name)} = ${printValue(parsed, '')} satisfies AnalysisFixture;\n`;
	});

	// Double-quoted throughout, matching `JSON.stringify` above. A generated file that mixed
	// quote styles would read as hand-written and invite hand edits.
	const entries = fixtureNames
		.map((name) => `\t${JSON.stringify(name)}: ${constantName(name)}`)
		.join(',\n');
	const names = fixtureNames.map((name) => `\t${JSON.stringify(name)}`).join(',\n');

	const union = fixtureNames.map((name) => `\n\t| ${JSON.stringify(name)}`).join('');

	const registry = `/** Names of the available fixtures, for exhaustive scenario handling. */
export type AnalysisFixtureName =${union};

/**
 * Every fixture, keyed by its file name.
 *
 * Keyed by file name rather than by an invented scenario label so that the map and the
 * directory listing can be compared without a translation table in between.
 *
 * Annotated \`AnalysisFixture\` rather than left to inference. The constants above keep
 * their exact literal types, which is what makes the \`satisfies\` on each of them a real
 * check; but a lookup into an inferred map would return a union of six unrelated shapes,
 * and reading \`.report\` off it would not compile for the fixtures that have no report.
 * The annotation hands consumers the contract type instead of the shape of the sample.
 */
export const ANALYSIS_FIXTURES: Readonly<Record<AnalysisFixtureName, AnalysisFixture>> = {
${entries}
};

/**
 * The same names as a value.
 *
 * Emitted as a literal rather than \`Object.keys(ANALYSIS_FIXTURES)\`, which would be typed
 * \`string[]\` and force every consumer into a cast back to \`AnalysisFixtureName\`.
 */
export const ANALYSIS_FIXTURE_NAMES = [
${names}
] as const satisfies readonly AnalysisFixtureName[];
`;

	return `${GENERATED_HEADER}\n${declarations.join('\n')}\n${registry}`;
}

describe('analysis-v1 fixtures', () => {
	const fixtureNames = readFixtureNames();

	test('every contract scenario is present on disk', () => {
		// Guards the generator against the one failure that would look like success: a
		// directory that stopped matching, producing an empty module that agrees with an
		// equally empty snapshot.
		expect(fixtureNames).toEqual(expect.arrayContaining([...REQUIRED_FIXTURES]));
	});

	test('src/fixtures.ts is regenerated from the committed fixtures', async () => {
		const generated = generateFixturesModule(fixtureNames);

		// The snapshot file IS the generated module, so `--update` regenerates in place.
		await expect(normalizeNewlines(generated)).toMatchFileSnapshot(FIXTURES_MODULE_PATH);
	});

	test('every fixture is emitted under a compile-time type assertion', () => {
		// The whole point of generating TypeScript rather than importing JSON is the
		// `satisfies`. Regenerating without it would still round-trip through the snapshot
		// above, and `pnpm -r check` would then be verifying nothing.
		const committed = normalizeNewlines(readFileSync(FIXTURES_MODULE_PATH, 'utf8'));

		for (const name of fixtureNames) {
			expect(committed).toContain(
				`export const ${constantName(name)} = {`
			);
		}

		const assertions = committed.match(/\bsatisfies AnalysisFixture;/g) ?? [];
		expect(assertions).toHaveLength(fixtureNames.length);
	});
});
