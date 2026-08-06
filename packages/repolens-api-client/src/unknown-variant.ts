/**
 * Exhaustive enum handling, and what to do with a variant this build has never seen.
 *
 * Implements `UNKNOWN_VARIANT_POLICY` from `crates/repolens-server/src/contract/mod.rs`:
 *
 *   1. **Never crash.** An unknown variant is data, not a bug in the client.
 *   2. **Never silently drop it.** Render it in a neutral fallback naming the raw value.
 *   3. **Fail the build, not the browser.** Adding a variant without handling it here
 *      breaks CI.
 *
 * Rule 3 is enforced twice, on purpose, because the two mechanisms fail differently:
 *
 *   - **At compile time**, by the explicit `Record<Enum, string>` annotation on every label
 *     map below. A variant added to the Rust enum flows through utoipa into `schema.ts`,
 *     and the map is then missing a key — `pnpm -r check` fails. This is the fast gate, and
 *     it also catches a *removed* variant, which would leave a key that no longer exists.
 *   - **At test time**, by `unknown-variant.test.ts`, which reads the variants straight out
 *     of `contracts/openapi.json` and compares them with `HANDLED_VARIANTS`. This is the
 *     gate that still works when the type-level one is defeated — someone widening an
 *     annotation to `Record<string, string>` to unblock themselves passes `tsc` and fails
 *     the test.
 *
 * Rules 1 and 2 are a safety net for a case rule 3 cannot cover: a statically hosted
 * frontend outlives the build it was compiled against, so a browser can hold a cached
 * bundle for months while the API gains variants. CI cannot fail retroactively in a tab
 * that is already open.
 *
 * This module is deliberately presentation-free — labels, not colours or icons. It is
 * imported by a contract package that must not depend on Svelte, and a UI that wants a
 * neutral badge style keys off `known === null` rather than string-matching the label.
 */

import type {
	AnalysisState,
	Confidence,
	CodeRole,
	ErrorCode,
	EvidenceKind,
	FindingCategory,
	FindingState,
	ProbeStatus,
	Severity,
	TriggerStatus
} from './contract';

/**
 * The result of interpreting one enum value received from the API.
 *
 * `raw` is always present, including for recognised variants, so a UI never has to
 * reconstruct what the server actually sent in order to report it.
 */
export interface VariantDescriptor<Variant extends string> {
	/** Exactly what the API sent. */
	readonly raw: string;
	/** The recognised variant, or `null` when this build has never seen it. */
	readonly known: Variant | null;
	/** Human-readable text. For an unrecognised value it names the raw value. */
	readonly label: string;
}

/**
 * How an unrecognised value is presented.
 *
 * It names the raw value rather than hiding it, and reads as an admission rather than a
 * category — "Unrecognised (CRITICAL)" cannot be mistaken for a severity the frontend
 * knows how to rank, whereas rendering a bare `CRITICAL` could.
 */
export function unknownVariantLabel(raw: string): string {
	return `Unrecognised (${raw})`;
}

function describeVariant<Variant extends string>(
	labels: Readonly<Record<Variant, string>>,
	raw: string
): VariantDescriptor<Variant> {
	// `Object.hasOwn` rather than `raw in labels` or a bare lookup: `constructor`,
	// `toString` and `valueOf` are inherited from Object.prototype, so both alternatives
	// report them as recognised variants and then hand a *function* to the UI as a label.
	// A server would have to be misbehaving to send one — which is exactly the case rule 1
	// exists for.
	if (Object.hasOwn(labels, raw)) {
		const known = raw as Variant;
		return { raw, known, label: labels[known] };
	}

	return { raw, known: null, label: unknownVariantLabel(raw) };
}

/**
 * Every label map is annotated, never inferred.
 *
 * `Readonly<Record<Enum, string>>` is what makes the compiler the first line of defence:
 * a missing key is an error, and so is a key that is not a member of the enum. Dropping
 * the annotation — or writing `satisfies` instead — would let TypeScript infer the object's
 * own shape and both failures would compile.
 */
export const ANALYSIS_STATE_LABELS: Readonly<Record<AnalysisState, string>> = {
	QUEUED: 'Queued',
	RESOLVING: 'Resolving commit',
	COLLECTING: 'Collecting files',
	ANALYZING: 'Analyzing',
	BUILDING_REPORT: 'Building report',
	COMPLETED: 'Completed',
	// Describes the *kind* of failure, not whether a retry is permitted. That is
	// `RetryPolicy.allowed`, which only the server can decide.
	FAILED_RETRIABLE: 'Failed (retriable)',
	FAILED_PERMANENT: 'Failed (permanent)'
};

export const CONFIDENCE_LABELS: Readonly<Record<Confidence, string>> = {
	LOW: 'Low',
	MEDIUM: 'Medium',
	HIGH: 'High'
};

export const ERROR_CODE_LABELS: Readonly<Record<ErrorCode, string>> = {
	INVALID_REPOSITORY_URL: 'Invalid repository URL',
	REPOSITORY_NOT_FOUND: 'Repository not found',
	REPOSITORY_INACCESSIBLE: 'Repository inaccessible',
	REPOSITORY_ARCHIVED: 'Repository archived',
	REPOSITORY_TOO_LARGE: 'Repository too large',
	RATE_LIMITED: 'Rate limited',
	WORKER_FAILED_RETRIABLE: 'Worker failed (retriable)',
	ANALYZER_FAILED_PERMANENT: 'Analyzer failed (permanent)'
};

export const EVIDENCE_KIND_LABELS: Readonly<Record<EvidenceKind, string>> = {
	FILE_PRESENCE: 'File presence',
	FILE_EXCERPT: 'File excerpt',
	DEPENDENCY_ENTRY: 'Dependency entry',
	WORKFLOW_DEFINITION: 'Workflow definition',
	STATISTIC: 'Statistic',
	REPOSITORY_METADATA: 'Repository metadata'
};

export const FINDING_CATEGORY_LABELS: Readonly<Record<FindingCategory, string>> = {
	TECHNOLOGY: 'Technology',
	ARCHITECTURE: 'Architecture',
	SOURCE_AND_DOCUMENTATION: 'Source and documentation',
	BUILD_AND_DEPENDENCIES: 'Build and dependencies',
	TESTING: 'Testing',
	CI_CD: 'CI/CD',
	OPERATIONS: 'Operations',
	SECURITY_AND_MAINTENANCE: 'Security and maintenance'
};

export const CODE_ROLE_LABELS: Readonly<Record<CodeRole, string>> = {
	PRODUCTION: 'Production',
	TEST: 'Test',
	// Named explicitly rather than folded into production: counting machine-produced
	// code as hand-written work overstates effort and understates how much of the
	// repository is derived.
	GENERATED: 'Generated',
	TOOLING: 'Tooling'
};

export const FINDING_STATE_LABELS: Readonly<Record<FindingState, string>> = {
	DETECTED: 'Detected',
	DOCUMENTED: 'Documented',
	MISSING: 'Missing',
	NOT_APPLICABLE: 'Not applicable',
	// Kept distinct from MISSING everywhere it is rendered: "we could not check" is not
	// "it is not there", and collapsing them is the single easiest way to make this
	// product lie.
	UNABLE_TO_VERIFY: 'Unable to verify'
};

export const PROBE_STATUS_LABELS: Readonly<Record<ProbeStatus, string>> = {
	OK: 'OK',
	DEGRADED: 'Degraded',
	UNAVAILABLE: 'Unavailable'
};

export const SEVERITY_LABELS: Readonly<Record<Severity, string>> = {
	INFO: 'Info',
	LOW: 'Low',
	MEDIUM: 'Medium',
	HIGH: 'High'
};

export const TRIGGER_STATUS_LABELS: Readonly<Record<TriggerStatus, string>> = {
	PENDING: 'Pending',
	SUCCEEDED: 'Succeeded',
	FAILED: 'Failed'
};

export function describeAnalysisState(raw: string): VariantDescriptor<AnalysisState> {
	return describeVariant(ANALYSIS_STATE_LABELS, raw);
}

export function describeCodeRole(raw: string): VariantDescriptor<CodeRole> {
	return describeVariant(CODE_ROLE_LABELS, raw);
}

export function describeConfidence(raw: string): VariantDescriptor<Confidence> {
	return describeVariant(CONFIDENCE_LABELS, raw);
}

export function describeErrorCode(raw: string): VariantDescriptor<ErrorCode> {
	return describeVariant(ERROR_CODE_LABELS, raw);
}

export function describeEvidenceKind(raw: string): VariantDescriptor<EvidenceKind> {
	return describeVariant(EVIDENCE_KIND_LABELS, raw);
}

export function describeFindingCategory(raw: string): VariantDescriptor<FindingCategory> {
	return describeVariant(FINDING_CATEGORY_LABELS, raw);
}

export function describeFindingState(raw: string): VariantDescriptor<FindingState> {
	return describeVariant(FINDING_STATE_LABELS, raw);
}

export function describeProbeStatus(raw: string): VariantDescriptor<ProbeStatus> {
	return describeVariant(PROBE_STATUS_LABELS, raw);
}

export function describeSeverity(raw: string): VariantDescriptor<Severity> {
	return describeVariant(SEVERITY_LABELS, raw);
}

export function describeTriggerStatus(raw: string): VariantDescriptor<TriggerStatus> {
	return describeVariant(TRIGGER_STATUS_LABELS, raw);
}

/**
 * Which variants the frontend claims to handle, keyed by OpenAPI component name.
 *
 * Derived from the label maps with `Object.keys` rather than listed again, so this cannot
 * disagree with the code that actually renders. The keys are component names because that
 * is what `contracts/openapi.json` uses, and the test asserts *both* directions: every
 * enum in the document appears here, and every entry here is still an enum in the
 * document. The first catches a new variant or a whole new enum; the second catches a
 * frontend still handling something the contract dropped.
 */
export const HANDLED_VARIANTS: Readonly<Record<string, readonly string[]>> = {
	AnalysisState: Object.keys(ANALYSIS_STATE_LABELS),
	CodeRole: Object.keys(CODE_ROLE_LABELS),
	Confidence: Object.keys(CONFIDENCE_LABELS),
	ErrorCode: Object.keys(ERROR_CODE_LABELS),
	EvidenceKind: Object.keys(EVIDENCE_KIND_LABELS),
	FindingCategory: Object.keys(FINDING_CATEGORY_LABELS),
	FindingState: Object.keys(FINDING_STATE_LABELS),
	ProbeStatus: Object.keys(PROBE_STATUS_LABELS),
	Severity: Object.keys(SEVERITY_LABELS),
	TriggerStatus: Object.keys(TRIGGER_STATUS_LABELS)
};
