/**
 * Presentation adapter over the contract package's variant descriptors.
 *
 * **The labels are not here.** `@repolens/api-client` owns them, together with the
 * compile-time and test-time gates that make `UNKNOWN_VARIANT_POLICY` real: every label map
 * is annotated `Record<Enum, string>`, so a variant added to the Rust enum breaks
 * `pnpm -r check`, and `unknown-variant.test.ts` re-checks the same thing against
 * `contracts/openapi.json` in case someone widens an annotation to get unblocked.
 *
 * A second label table in the frontend would defeat both gates at once — it would compile,
 * it would pass, and it would quietly disagree with the contract package about what
 * `UNABLE_TO_VERIFY` means. So this module adds exactly two things the contract package
 * deliberately refuses to carry, because it must not depend on any UI:
 *
 *   1. a **styling token** per variant, with one neutral token for everything unrecognised;
 *   2. the **step order** and terminal/failure predicates the progress timeline needs.
 *
 * Everything else is forwarded.
 */

import {
	describeAnalysisState,
	describeConfidence,
	describeErrorCode,
	describeEvidenceKind,
	describeFindingCategory,
	describeFindingState,
	describeSeverity,
	describeTriggerStatus,
	type AnalysisState,
	type VariantDescriptor
} from '@repolens/api-client';

/**
 * One enum value, resolved for display.
 *
 * `known` is a boolean here rather than the descriptor's `known: Variant | null`, because a
 * template asks "is this recognised?" and never "which variant is it?" — the label already
 * answers the second question.
 */
export interface EnumDisplay {
	/** Whether this build knows the value. */
	readonly known: boolean;
	/** Words for a reader. For an unknown value, one that names the raw value. */
	readonly label: string;
	/** The wire value, verbatim. */
	readonly raw: string;
	/**
	 * Styling slug: the lowercased variant, or `unknown`.
	 *
	 * Unrecognised values collapse to one neutral token on purpose. A per-value token would
	 * let an unstyled variant inherit whatever the cascade happened to leave behind, which
	 * is how an unknown severity ends up looking like a known one.
	 */
	readonly token: string;
}

/** Shown when a value is absent entirely — distinct from one we merely do not recognise. */
const ABSENT: EnumDisplay = {
	known: false,
	label: 'not reported',
	raw: '',
	token: 'unknown'
};

function present<Variant extends string>(
	describe: (raw: string) => VariantDescriptor<Variant>,
	raw: string | null | undefined
): EnumDisplay {
	if (raw === null || raw === undefined || raw === '') return ABSENT;

	const descriptor = describe(raw);
	return {
		known: descriptor.known !== null,
		label: descriptor.label,
		raw: descriptor.raw,
		token:
			descriptor.known === null ? 'unknown' : descriptor.known.toLowerCase().replaceAll('_', '-')
	};
}

export function findingState(raw: string | null | undefined): EnumDisplay {
	return present(describeFindingState, raw);
}

/** Impact if valid. Rendered by `SeverityBadge`, never merged with confidence. */
export function severity(raw: string | null | undefined): EnumDisplay {
	return present(describeSeverity, raw);
}

/** Evidence strength. Rendered by `ConfidenceBadge`, never merged with severity. */
export function confidence(raw: string | null | undefined): EnumDisplay {
	return present(describeConfidence, raw);
}

export function findingCategory(raw: string | null | undefined): EnumDisplay {
	return present(describeFindingCategory, raw);
}

export function evidenceKind(raw: string | null | undefined): EnumDisplay {
	return present(describeEvidenceKind, raw);
}

export function triggerStatus(raw: string | null | undefined): EnumDisplay {
	return present(describeTriggerStatus, raw);
}

export function analysisState(raw: string | null | undefined): EnumDisplay {
	return present(describeAnalysisState, raw);
}

export function errorCode(raw: string | null | undefined): EnumDisplay {
	return present(describeErrorCode, raw);
}

/* -------------------------------------------------------------------------- */
/* Step order                                                                  */
/* -------------------------------------------------------------------------- */

/**
 * The five steps of the pipeline, in the order the contract documents.
 *
 * `AnalysisState` is "ordered as the work actually proceeds, so a UI can render a checklist
 * by position without a second table mapping states to steps" — so this array *is* that
 * order. The three terminal states are absent because they are not steps: `COMPLETED` means
 * every step is behind us, and neither failure says which step it reached.
 *
 * `satisfies readonly AnalysisState[]` rather than a bare array: a state renamed in the
 * Rust enum fails to compile here instead of producing a timeline that silently stops
 * advancing.
 */
export const ANALYSIS_STEPS = [
	'QUEUED',
	'RESOLVING',
	'COLLECTING',
	'ANALYZING',
	'BUILDING_REPORT'
] as const satisfies readonly AnalysisState[];

const TERMINAL_STATES = [
	'COMPLETED',
	'FAILED_RETRIABLE',
	'FAILED_PERMANENT'
] as const satisfies readonly AnalysisState[];

/**
 * Whether polling should stop.
 *
 * An unrecognised value is **not** terminal. A newer pipeline stage must not make this
 * build declare an analysis finished that is still running — the safe error is to keep
 * asking, which the server's `poll_after_ms` bounds anyway.
 */
export function isTerminal(raw: string | null | undefined): boolean {
	return TERMINAL_STATES.some((state) => state === raw);
}

/** Whether the analysis failed. Unrecognised values are not failures, for the same reason. */
export function isFailure(raw: string | null | undefined): boolean {
	return raw === 'FAILED_RETRIABLE' || raw === 'FAILED_PERMANENT';
}

/** 1-based step position, or `null` for terminal and unrecognised states. */
export function analysisStepNumber(raw: string | null | undefined): number | null {
	const index = ANALYSIS_STEPS.findIndex((step) => step === raw);
	return index === -1 ? null : index + 1;
}
