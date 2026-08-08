/**
 * The exhaustive-enum gate.
 *
 * Reads the enum variants straight out of `contracts/openapi.json` and compares them with
 * the set `unknown-variant.ts` claims to handle. Adding a variant to a Rust enum without
 * handling it in TypeScript fails here.
 *
 * This duplicates a check the compiler already performs — every label map is annotated
 * `Record<Enum, string>`, so a missing key fails `pnpm -r check` first. The duplication is
 * the point. The type-level gate is only as strong as the annotation, and the annotation is
 * the exact thing somebody widens to `Record<string, string>` when they are in a hurry.
 * This test reads the contract itself, so it cannot be relaxed from inside the frontend.
 *
 * The comparison runs in both directions. A variant in the document that the frontend does
 * not handle is the case the policy is written for; a variant the frontend handles that the
 * document no longer contains is dead code that will quietly outlive its meaning.
 */

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, test } from 'vitest';

import {
	ANALYSIS_FIXTURES,
	type AnalysisFixtureName
} from './src/fixtures';
import {
	describeCodeRole,
	HANDLED_VARIANTS,
	describeAnalysisState,
	describeConfidence,
	describeErrorCode,
	describeEvidenceKind,
	describeEvidenceProvider,
	describeFindingCategory,
	describeFindingState,
	describeProbeStatus,
	describeSeverity,
	describeTriggerStatus,
	unknownVariantLabel,
	type VariantDescriptor
} from './src/unknown-variant';

const OPENAPI_DOCUMENT_PATH = fileURLToPath(
	new URL('../../contracts/openapi.json', import.meta.url)
);

/**
 * Enum name → the `describe*` function that handles it.
 *
 * Annotated `Record<string, ...>` deliberately: the keys have to be checkable against the
 * document at runtime, and a key typed as a union of the nine known names could never
 * disagree with it.
 */
const DESCRIBERS: Readonly<Record<string, (raw: string) => VariantDescriptor<string>>> = {
	AnalysisState: describeAnalysisState,
	CodeRole: describeCodeRole,
	Confidence: describeConfidence,
	ErrorCode: describeErrorCode,
	EvidenceKind: describeEvidenceKind,
	EvidenceProvider: describeEvidenceProvider,
	FindingCategory: describeFindingCategory,
	FindingState: describeFindingState,
	ProbeStatus: describeProbeStatus,
	Severity: describeSeverity,
	TriggerStatus: describeTriggerStatus
};

interface EnumSchema {
	type?: string;
	enum?: unknown;
}

/** Every `type: string` enum declared in the OpenAPI document, by component name. */
function readSchemaEnums(): Map<string, string[]> {
	const document: unknown = JSON.parse(readFileSync(OPENAPI_DOCUMENT_PATH, 'utf8'));
	const schemas = (document as { components?: { schemas?: Record<string, EnumSchema> } })
		.components?.schemas;

	const found = new Map<string, string[]>();
	for (const [name, schema] of Object.entries(schemas ?? {})) {
		if (schema.type === 'string' && Array.isArray(schema.enum)) {
			found.set(name, schema.enum.map(String));
		}
	}
	return found;
}

describe('unknown enum variants', () => {
	test('the OpenAPI document is present', () => {
		// Asserted rather than skipped. A gate that silently passes when its input goes
		// missing is worse than no gate: the run stays green and the contract stops being
		// compared to anything at all.
		expect(
			existsSync(OPENAPI_DOCUMENT_PATH),
			`${OPENAPI_DOCUMENT_PATH} must exist — it is the source this gate compares against`
		).toBe(true);
	});

	const schemaEnums = readSchemaEnums();

	test('every enum in the contract is handled by the frontend', () => {
		// Catches a whole new enum, not just a new variant. An enum nobody wired up renders
		// as raw SCREAMING_SNAKE_CASE in the UI, which is a slower and more embarrassing
		// way to find out.
		expect([...schemaEnums.keys()].sort()).toEqual(Object.keys(HANDLED_VARIANTS).sort());
	});

	test('the gate is comparing a non-empty set of enums', () => {
		// If the document's shape changed and `readSchemaEnums` matched nothing, the
		// assertion above would compare two empty-ish sets and pass while checking nothing.
		expect(schemaEnums.size).toBeGreaterThanOrEqual(Object.keys(DESCRIBERS).length);
	});

	for (const [name, variants] of schemaEnums) {
		describe(name, () => {
			test('the handled variants are exactly the contract variants', () => {
				const handled = HANDLED_VARIANTS[name] ?? [];

				expect(
					[...handled].sort(),
					`${name} in contracts/openapi.json and its label map in ` +
						'src/unknown-variant.ts disagree. Add the missing variant to the label ' +
						'map — do not relax this test.'
				).toEqual([...variants].sort());
			});

			test('every contract variant is recognised at runtime', () => {
				const describe_ = DESCRIBERS[name];
				expect(describe_, `no describe* function registered for ${name}`).toBeDefined();
				if (!describe_) return;

				for (const variant of variants) {
					const described = describe_(variant);
					expect(described.known).toBe(variant);
					expect(described.raw).toBe(variant);
					expect(described.label.length).toBeGreaterThan(0);
					// A label that is still the wire value means nobody wrote one.
					expect(described.label).not.toBe(unknownVariantLabel(variant));
				}
			});

			test('an unseen variant degrades instead of crashing', () => {
				const describe_ = DESCRIBERS[name];
				if (!describe_) return;

				// Stands in for a variant a future backend adds after this bundle was built
				// and cached in someone's browser — the case CI cannot retroactively fail.
				const described = describe_('VARIANT_FROM_A_LATER_RELEASE');

				expect(described.known).toBeNull();
				expect(described.raw).toBe('VARIANT_FROM_A_LATER_RELEASE');
				// Rule 2: never silently dropped. The raw value has to survive into the UI.
				expect(described.label).toContain('VARIANT_FROM_A_LATER_RELEASE');
			});
		});
	}

	test('inherited object properties are not mistaken for variants', () => {
		// `raw in labels` or `labels[raw]` would resolve these against Object.prototype and
		// hand a function to the UI as a label. Only a misbehaving server sends them, which
		// is exactly the situation rule 1 covers.
		for (const inherited of ['constructor', 'toString', 'valueOf', '__proto__', '']) {
			for (const describe_ of Object.values(DESCRIBERS)) {
				const described = describe_(inherited);
				expect(described.known).toBeNull();
				expect(typeof described.label).toBe('string');
			}
		}
	});

	test('the fixtures contain only variants the frontend handles', () => {
		// Ties the two gates together: the fixtures are the contract made executable, so a
		// value in one that the frontend cannot name is a hole in either the fixture or the
		// label maps, and this says which.
		for (const name of Object.keys(ANALYSIS_FIXTURES) as AnalysisFixtureName[]) {
			const { analysis, report } = ANALYSIS_FIXTURES[name];

			const described: VariantDescriptor<string>[] = [
				describeAnalysisState(analysis.state),
				describeTriggerStatus(analysis.execution.trigger_status)
			];

			if (analysis.error) described.push(describeErrorCode(analysis.error.code));

			for (const statement of report?.overview ?? []) {
				described.push(describeConfidence(statement.confidence));
			}

			for (const finding of report?.findings ?? []) {
				described.push(
					describeFindingCategory(finding.category),
					describeFindingState(finding.state),
					describeSeverity(finding.severity),
					describeConfidence(finding.confidence)
				);
				for (const evidence of finding.evidence) {
					described.push(describeEvidenceKind(evidence.kind));
				}
			}

			for (const { raw, known } of described) {
				expect(known, `fixture ${name} carries unhandled enum value ${raw}`).not.toBeNull();
			}
		}
	});
});
