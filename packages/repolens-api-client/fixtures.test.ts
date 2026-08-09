/**
 * The fixture type-check.
 *
 * `contracts/fixtures/<family>/*.json` is generated from the Rust DTOs and is the
 * authoritative wire contract. This test binds it into TypeScript by generating
 * `src/fixtures.ts`, where every fixture is emitted as a literal under a `satisfies`
 * clause naming its family's type. Two failures follow, and both are wanted:
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
const FIXTURE_ROOT = fileURLToPath(new URL('../../contracts/fixtures/', import.meta.url));

const FIXTURES_MODULE_PATH = fileURLToPath(new URL('./src/fixtures.ts', import.meta.url));

/**
 * One published contract family.
 *
 * Two families exist and they are deliberately not merged: `analysis-v1` is the report a
 * reader came for, `admin-v1` is an operational snapshot behind an authorisation gate. They
 * share this gate because the rule is identical, and share nothing else — each is checked
 * against its own type, and neither versions the other.
 */
interface Family {
	/** Directory under `contracts/fixtures/`, and the name used in failure messages. */
	readonly directory: string;
	/** The type each fixture in this family is emitted under `satisfies`. */
	readonly type: string;
	/**
	 * Prefix on the generated constant names.
	 *
	 * Empty for `analysis-v1`, whose constants are imported by name across the workspace
	 * and predate the second family. Prefixed elsewhere, so two families cannot claim the
	 * same identifier by both having, say, an `overview.json`.
	 */
	readonly constantPrefix: string;
	/** Base name of the registry constant and its name union. */
	readonly registry: string;
	/**
	 * The scenarios this family is required to cover.
	 *
	 * Listed rather than derived so that *deleting* a fixture fails too. A generated module
	 * regenerated from an emptied directory would otherwise match its own snapshot perfectly
	 * and prove nothing.
	 */
	readonly required: readonly string[];
}

const FAMILIES: readonly Family[] = [
	{
		directory: 'analysis-v1',
		type: 'AnalysisFixture',
		constantPrefix: '',
		registry: 'ANALYSIS',
		required: [
			'completed-report',
			'evidence-source-absent',
			'failed-permanent',
			'failed-retriable',
			'loc-unavailable',
			'queued',
			'resolving'
		]
	},
	{
		directory: 'admin-v1',
		type: 'AdminFixture',
		constantPrefix: 'ADMIN_',
		registry: 'ADMIN',
		// The pair is the point: one snapshot with a memory figure and one without. Losing
		// the second would quietly retire the case the null exists for.
		required: ['overview', 'overview-memory-unavailable']
	}
];

const GENERATED_HEADER = `/**
 * GENERATED FILE — DO NOT EDIT BY HAND.
 *
 * The executable fixtures, bound to TypeScript. Produced from the JSON under
 * \`contracts/fixtures/\` — itself generated from the Rust DTOs — by:
 *
 *     pnpm --filter @repolens/api-client fixtures:update
 *
 * Each fixture is emitted as a literal under a \`satisfies\` clause so the compiler checks it
 * against the generated schema. A JSON import could not: TypeScript widens string literals
 * in JSON modules, which would type every enum value as \`string\` and silently retire the
 * check that matters most.
 *
 * This is a binding, not a copy. \`fixtures.test.ts\` regenerates it and fails if the result
 * differs, so fixture content is authored in exactly one place — the JSON.
 */

import type { AdminFixture, AnalysisFixture } from './contract';
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

/** `completed-report` → `COMPLETED_REPORT_FIXTURE`; `overview` → `ADMIN_OVERVIEW_FIXTURE`. */
function constantName(family: Family, fixtureName: string): string {
	return `${family.constantPrefix}${fixtureName.replace(/-/g, '_').toUpperCase()}_FIXTURE`;
}

function readFixtureNames(family: Family): string[] {
	return readdirSync(`${FIXTURE_ROOT}${family.directory}/`)
		.filter((entry) => entry.endsWith('.json'))
		.map((entry) => entry.slice(0, -'.json'.length))
		.sort(); // readdir order is filesystem-dependent; the generated file must not be.
}

function generateFamilySection(family: Family, fixtureNames: readonly string[]): string {
	const declarations = fixtureNames.map((name) => {
		const parsed: unknown = JSON.parse(
			readFileSync(`${FIXTURE_ROOT}${family.directory}/${name}.json`, 'utf8')
		);
		return `/** Fixture \`${family.directory}/${name}.json\`. */\nexport const ${constantName(family, name)} = ${printValue(parsed, '')} satisfies ${family.type};\n`;
	});

	// Double-quoted throughout, matching `JSON.stringify` above. A generated file that mixed
	// quote styles would read as hand-written and invite hand edits.
	const entries = fixtureNames
		.map((name) => `\t${JSON.stringify(name)}: ${constantName(family, name)}`)
		.join(',\n');
	const names = fixtureNames.map((name) => `\t${JSON.stringify(name)}`).join(',\n');

	const union = fixtureNames.map((name) => `\n\t| ${JSON.stringify(name)}`).join('');
	const nameType = `${family.registry.charAt(0)}${family.registry.slice(1).toLowerCase()}FixtureName`;

	const registry = `/** Names of the available \`${family.directory}\` fixtures, for exhaustive scenario handling. */
export type ${nameType} =${union};

/**
 * Every \`${family.directory}\` fixture, keyed by its file name.
 *
 * Keyed by file name rather than by an invented scenario label so that the map and the
 * directory listing can be compared without a translation table in between.
 *
 * Annotated \`${family.type}\` rather than left to inference. The constants above keep
 * their exact literal types, which is what makes the \`satisfies\` on each of them a real
 * check; but a lookup into an inferred map would return a union of unrelated shapes, and
 * reading an optional field off it would not compile for the fixtures that lack it. The
 * annotation hands consumers the contract type instead of the shape of the sample.
 */
export const ${family.registry}_FIXTURES: Readonly<Record<${nameType}, ${family.type}>> = {
${entries}
};

/**
 * The same names as a value.
 *
 * Emitted as a literal rather than \`Object.keys(${family.registry}_FIXTURES)\`, which would be
 * typed \`string[]\` and force every consumer into a cast back to \`${nameType}\`.
 */
export const ${family.registry}_FIXTURE_NAMES = [
${names}
] as const satisfies readonly ${nameType}[];
`;

	return `${declarations.join('\n')}\n${registry}`;
}

function generateFixturesModule(): string {
	const sections = FAMILIES.map((family) =>
		generateFamilySection(family, readFixtureNames(family))
	);
	return `${GENERATED_HEADER}\n${sections.join('\n')}`;
}

describe('executable fixtures', () => {
	test('src/fixtures.ts is regenerated from the committed fixtures', async () => {
		const generated = generateFixturesModule();

		// The snapshot file IS the generated module, so `--update` regenerates in place.
		await expect(normalizeNewlines(generated)).toMatchFileSnapshot(FIXTURES_MODULE_PATH);
	});

	for (const family of FAMILIES) {
		describe(family.directory, () => {
			const fixtureNames = readFixtureNames(family);

			test('every contract scenario is present on disk', () => {
				// Guards the generator against the one failure that would look like success: a
				// directory that stopped matching, producing an empty module that agrees with an
				// equally empty snapshot.
				expect(fixtureNames).toEqual(expect.arrayContaining([...family.required]));
			});

			test('every fixture is emitted under a compile-time type assertion', () => {
				// The whole point of generating TypeScript rather than importing JSON is the
				// `satisfies`. A generator that stopped emitting it would still round-trip
				// through the snapshot above, and `pnpm -r check` would then be verifying
				// nothing.
				//
				// Asserted against the generated source rather than the committed file, which
				// is being rewritten by the snapshot test in the very same run under
				// `--update`. Reading the file there made regeneration fail once and pass on
				// the second attempt — a workflow nobody would trust and everybody would
				// work around. The committed file is still covered: outside `--update` the
				// snapshot test is what proves it equals this string.
				const generated = generateFixturesModule();

				for (const name of fixtureNames) {
					expect(generated).toContain(`export const ${constantName(family, name)} = {`);
				}

				// Counted per family, so a fixture emitted under the *other* family's type
				// cannot be absorbed by a single total that happens to add up.
				const assertions =
					generated.match(new RegExp(`\\bsatisfies ${family.type};`, 'g')) ?? [];
				expect(assertions).toHaveLength(fixtureNames.length);
			});
		});
	}
});
