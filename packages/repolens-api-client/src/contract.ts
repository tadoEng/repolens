/**
 * Named aliases for the published contract types.
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

/**
 * The `admin-v1` operational snapshot.
 *
 * Every figure in it describes the single process that answered the request — there is no
 * aggregation across instances and no history, so a restart resets the counters. That is a
 * property of the contract rather than of the dashboard, which is why it is said here as
 * well as on any page that renders one.
 */
export type AdminOverview = Schemas['AdminOverview'];
export type HttpOverview = Schemas['HttpOverview'];
export type LatencyPercentile = Schemas['LatencyPercentile'];
export type LatencySummary = Schemas['LatencySummary'];
export type ProcessOverview = Schemas['ProcessOverview'];
export type RouteOverview = Schemas['RouteOverview'];
export type StatusClassCounts = Schemas['StatusClassCounts'];

export type AnalysisState = Schemas['AnalysisState'];
/** How a counted file was classified by role. Structural evidence, not a quality score. */
export type CodeRole = Schemas['CodeRole'];
export type Confidence = Schemas['Confidence'];
export type ErrorCode = Schemas['ErrorCode'];
export type EvidenceKind = Schemas['EvidenceKind'];
export type EvidenceProvider = Schemas['EvidenceProvider'];
export type FindingCategory = Schemas['FindingCategory'];
export type FindingState = Schemas['FindingState'];
/** The request method, folded into the closed set the server records. */
export type HttpMethodClass = Schemas['HttpMethodClass'];
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

/**
 * One executable fixture from `contracts/fixtures/admin-v1/`.
 *
 * An alias rather than a wrapper interface: an admin fixture *is* the response body, with
 * no companion resource to pair it with. The name exists so the generated fixture module
 * can say what each literal is checked against, and so this file stays the one place a
 * fixture's type is decided.
 */
export type AdminFixture = AdminOverview;
