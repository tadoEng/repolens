/**
 * Named aliases for the `analysis-v1` contract types.
 *
 * Every alias resolves to `components['schemas'][...]` in the generated `schema.ts`, so
 * nothing here is a second definition — rename a Rust DTO and this file stops compiling
 * rather than quietly describing a type the API no longer serves.
 *
 * The aliases exist because `components['schemas']['Finding']` is unreadable at the call
 * site and, worse, is easy to typo into a `never` that silently accepts anything. A named
 * import cannot be misspelled without a compile error.
 */

import type { components } from './schema';

type Schemas = components['schemas'];

export type Analysis = Schemas['Analysis'];
export type ApiError = Schemas['ApiError'];
export type AreaLineCount = Schemas['AreaLineCount'];
export type CompositionExclusion = Schemas['CompositionExclusion'];
export type Evidence = Schemas['Evidence'];
export type EvidenceSource = Schemas['EvidenceSource'];
export type ExecutionMetadata = Schemas['ExecutionMetadata'];
export type Finding = Schemas['Finding'];
export type LanguageLineCount = Schemas['LanguageLineCount'];
export type Limitation = Schemas['Limitation'];
export type LineCountSummary = Schemas['LineCountSummary'];
export type LineRange = Schemas['LineRange'];
export type OverviewStatement = Schemas['OverviewStatement'];
export type Report = Schemas['Report'];
export type RepositoryIdentity = Schemas['RepositoryIdentity'];
export type RetryPolicy = Schemas['RetryPolicy'];
export type SystemProbeResponse = Schemas['SystemProbeResponse'];

export type AnalysisState = Schemas['AnalysisState'];
/** How a counted file was classified by role. Structural evidence, not a quality score. */
export type CodeRole = Schemas['CodeRole'];
export type Confidence = Schemas['Confidence'];
export type ErrorCode = Schemas['ErrorCode'];
export type EvidenceKind = Schemas['EvidenceKind'];
export type EvidenceProvider = Schemas['EvidenceProvider'];
export type FindingCategory = Schemas['FindingCategory'];
export type FindingState = Schemas['FindingState'];
export type LargestSourceFile = Schemas['LargestSourceFile'];
export type RoleLineCount = Schemas['RoleLineCount'];
export type ProbeStatus = Schemas['ProbeStatus'];
export type Severity = Schemas['Severity'];
export type TriggerStatus = Schemas['TriggerStatus'];

/**
 * One executable fixture from `contracts/fixtures/analysis-v1/`.
 *
 * `report` is optional because most states genuinely have no report: it exists only once
 * an analysis reaches `COMPLETED`. Making it optional rather than nullable mirrors the
 * fixture files, where the key is absent rather than `null` — and a consumer that forgets
 * the missing case gets a type error instead of a runtime `undefined`.
 */
export interface AnalysisFixture {
	analysis: Analysis;
	report?: Report;
}
