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
	describeCodeRole,
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

/**
 * How a counted file was classified. Structural evidence, never a quality score.
 *
 * `GENERATED` in particular is the reason this is rendered at all: a 1,980-line generated
 * client at the top of the largest-files list is not the same fact as a 1,980-line
 * hand-written module, and a list that omits the role invites exactly that misreading.
 */
export function codeRole(raw: string | null | undefined): EnumDisplay {
	return present(describeCodeRole, raw);
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
/* The analysis-state partition                                                */
/* -------------------------------------------------------------------------- */

/**
 * What one `AnalysisState` *is*: a numbered step of the pipeline, or a terminal outcome.
 *
 * A discriminated union rather than two optional fields, so the two impossible shapes —
 * a step that is also terminal, and a terminal state carrying a step number — are not
 * merely discouraged but unrepresentable.
 */
export type AnalysisStatePhase =
	| { readonly phase: 'step'; readonly order: number }
	| { readonly phase: 'terminal'; readonly failure: boolean };

/**
 * Every `AnalysisState`, partitioned. **This is the single source of step order.**
 *
 * ## Why a `Record`, and not two arrays
 *
 * The obvious shape is `['QUEUED', …] as const satisfies readonly AnalysisState[]`, and it
 * is not enough. `satisfies` proves every value *listed* is a valid state; it proves
 * nothing about the states that are not listed. A state added to the Rust enum flows
 * through utoipa into `schema.ts`, gets a correct label from the contract package's
 * `Record<AnalysisState, string>` gate — and is still absent from both arrays, while this
 * file compiles and the timeline silently stops advancing at the state before it.
 *
 * A total `Record<AnalysisState, …>` closes that. The compiler then rejects all four ways
 * the partition can go wrong:
 *
 *   - a **missing** state — the object literal lacks a required key;
 *   - an **extra** state, left behind after a rename — the key is not in `AnalysisState`;
 *   - a **duplicated** state — TypeScript refuses repeated keys in an object literal;
 *   - a **miscategorised** state — the union above makes a terminal step unwritable.
 *
 * `src/tests/analysis-state-partition.test.ts` re-checks the same properties at run time
 * against the contract package's own variant list, because the type-level gate is defeated
 * by one person widening the annotation to unblock themselves.
 *
 * `AnalysisState` is documented as "ordered as the work actually proceeds", so `order`
 * restates the contract's own sequence rather than inventing one. The terminal states have
 * no order because neither failure reports how far it got, and `COMPLETED` means every
 * step is behind us.
 */
export const ANALYSIS_STATE_PARTITION: Readonly<Record<AnalysisState, AnalysisStatePhase>> = {
	QUEUED: { phase: 'step', order: 1 },
	RESOLVING: { phase: 'step', order: 2 },
	COLLECTING: { phase: 'step', order: 3 },
	ANALYZING: { phase: 'step', order: 4 },
	BUILDING_REPORT: { phase: 'step', order: 5 },
	COMPLETED: { phase: 'terminal', failure: false },
	FAILED_RETRIABLE: { phase: 'terminal', failure: true },
	FAILED_PERMANENT: { phase: 'terminal', failure: true }
};

/**
 * The pipeline steps, in order — derived, never written down a second time.
 *
 * A hand-maintained copy of this list is precisely the drift the partition exists to
 * prevent, so the timeline reads it from the partition and sorts by the declared order
 * rather than trusting object key order.
 */
export const ANALYSIS_STEPS: readonly AnalysisState[] = Object.entries(ANALYSIS_STATE_PARTITION)
	.flatMap(([state, phase]) =>
		phase.phase === 'step' ? [{ state: state as AnalysisState, order: phase.order }] : []
	)
	.sort((left, right) => left.order - right.order)
	.map((entry) => entry.state);

/**
 * The partition entry for a wire value, or `null` when this build has never seen it.
 *
 * `Object.hasOwn` rather than a bare lookup: `constructor` and `toString` are inherited
 * from `Object.prototype`, so a bare lookup would report them as recognised states and
 * hand a *function* to the caller. Only a misbehaving server sends one, which is exactly
 * the case the unknown-variant policy exists for.
 */
function phaseOf(raw: string | null | undefined): AnalysisStatePhase | null {
	if (raw === null || raw === undefined) return null;
	return Object.hasOwn(ANALYSIS_STATE_PARTITION, raw)
		? ANALYSIS_STATE_PARTITION[raw as AnalysisState]
		: null;
}

/**
 * Whether polling should stop.
 *
 * An unrecognised value is **not** terminal. A newer pipeline stage must not make this
 * build declare an analysis finished that is still running — the safe error is to keep
 * asking, which the server's `poll_after_ms` bounds anyway.
 */
export function isTerminal(raw: string | null | undefined): boolean {
	return phaseOf(raw)?.phase === 'terminal';
}

/** Whether the analysis failed. Unrecognised values are not failures, for the same reason. */
export function isFailure(raw: string | null | undefined): boolean {
	const phase = phaseOf(raw);
	return phase !== null && phase.phase === 'terminal' && phase.failure;
}

/** 1-based step position, or `null` for terminal and unrecognised states. */
export function analysisStepNumber(raw: string | null | undefined): number | null {
	const phase = phaseOf(raw);
	return phase !== null && phase.phase === 'step' ? phase.order : null;
}
