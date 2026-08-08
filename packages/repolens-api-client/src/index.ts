export {
	API_ORIGIN_ENV_VAR,
	createRepoLensClient,
	resolveApiOrigin,
	type RepoLensClient,
	type RepoLensClientOptions
} from './client';

export type { $defs, components, operations, paths, webhooks } from './schema';

export type {
	Analysis,
	AnalysisFixture,
	AnalysisState,
	ApiError,
	AreaLineCount,
	CompositionExclusion,
	CodeRole,
	Confidence,
	ErrorCode,
	Evidence,
	EvidenceKind,
	EvidenceSource,
	ExecutionMetadata,
	Finding,
	FindingCategory,
	FindingState,
	LanguageLineCount,
	LargestSourceFile,
	RoleLineCount,
	Limitation,
	LineCountSummary,
	LineRange,
	OverviewStatement,
	ProbeStatus,
	Report,
	RepositoryIdentity,
	RetryPolicy,
	Severity,
	SystemProbeResponse,
	TriggerStatus
} from './contract';

export {
	ANALYSIS_FIXTURES,
	ANALYSIS_FIXTURE_NAMES,
	COMPLETED_REPORT_FIXTURE,
	EVIDENCE_SOURCE_ABSENT_FIXTURE,
	FAILED_PERMANENT_FIXTURE,
	FAILED_RETRIABLE_FIXTURE,
	LOC_UNAVAILABLE_FIXTURE,
	QUEUED_FIXTURE,
	RESOLVING_FIXTURE,
	type AnalysisFixtureName
} from './fixtures';

export {
	ANALYSIS_STATE_LABELS,
	CODE_ROLE_LABELS,
	CONFIDENCE_LABELS,
	ERROR_CODE_LABELS,
	EVIDENCE_KIND_LABELS,
	FINDING_CATEGORY_LABELS,
	FINDING_STATE_LABELS,
	HANDLED_VARIANTS,
	PROBE_STATUS_LABELS,
	SEVERITY_LABELS,
	TRIGGER_STATUS_LABELS,
	describeAnalysisState,
	describeCodeRole,
	describeConfidence,
	describeErrorCode,
	describeEvidenceKind,
	describeFindingCategory,
	describeFindingState,
	describeProbeStatus,
	describeSeverity,
	describeTriggerStatus,
	unknownVariantLabel,
	type VariantDescriptor
} from './unknown-variant';
