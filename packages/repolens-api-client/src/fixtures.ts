/**
 * GENERATED FILE — DO NOT EDIT BY HAND.
 *
 * The executable `analysis-v1` fixtures, bound to TypeScript. Produced from
 * `contracts/fixtures/analysis-v1/*.json` — themselves generated from the Rust DTOs — by:
 *
 *     pnpm --filter @repolens/api-client fixtures:update
 *
 * Each fixture is emitted as a literal under `satisfies AnalysisFixture` so the compiler
 * checks it against the generated schema. A JSON import could not: TypeScript widens string
 * literals in JSON modules, which would type every enum value as `string` and silently
 * retire the check that matters most.
 *
 * This is a binding, not a copy. `fixtures.test.ts` regenerates it and fails if the result
 * differs, so fixture content is authored in exactly one place — the JSON.
 */

import type { AnalysisFixture } from './contract';

/** Fixture `completed-report.json`. */
export const COMPLETED_REPORT_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		state: "COMPLETED",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "the analysis succeeded"
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: true
	},
	report: {
		analysis_id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		tree_sha: "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
		analyzer_version: "0.1.0",
		ruleset_version: "1",
		completed_at: "2026-08-06T09:00:04Z",
		overview: [
			{
				statement: "Rust workspace with an Axum backend and a static SvelteKit frontend.",
				supporting_rule_ids: [
					"rust.workspace.detected"
				],
				confidence: "HIGH"
			}
		],
		findings: [
			{
				id: "0193a5c0-0000-7000-8000-000000000010",
				rule_id: "rust.workspace.detected",
				ruleset_version: "1",
				category: "TECHNOLOGY",
				state: "DETECTED",
				severity: "INFO",
				confidence: "HIGH",
				title: "Rust workspace detected",
				explanation: "The repository root declares a Cargo workspace.",
				evidence: [
					{
						kind: "FILE_EXCERPT",
						path: "Cargo.toml",
						excerpt: "[workspace]\nmembers = [\"crates/*\"]",
						truncated: true,
						digest: "sha256:6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b",
						line_range: {
							start: 1,
							end: 2
						}
					}
				],
				limitations: []
			},
			{
				id: "0193a5c0-0000-7000-8000-000000000011",
				rule_id: "docs.architecture.missing",
				ruleset_version: "1",
				category: "SOURCE_AND_DOCUMENTATION",
				state: "MISSING",
				severity: "LOW",
				confidence: "MEDIUM",
				title: "No architecture document found",
				explanation: "No docs/ARCHITECTURE file was found at the analyzed commit. Absence here means the boundaries a reader would need are not written down, not that the architecture is poor.",
				evidence: [
					{
						kind: "FILE_PRESENCE",
						path: "docs/",
						truncated: false
					}
				],
				limitations: [
					{
						code: "TREE_TRUNCATED",
						explanation: "The repository tree exceeded the traversal bound, so a document outside the collected paths would not have been seen."
					}
				],
				recommended_action: "Confirm by hand whether an architecture document exists outside the collected paths."
			},
			{
				id: "0193a5c0-0000-7000-8000-000000000012",
				rule_id: "ci.tests.unverifiable",
				ruleset_version: "1",
				category: "CI_CD",
				state: "UNABLE_TO_VERIFY",
				severity: "INFO",
				confidence: "LOW",
				title: "Could not determine whether CI runs tests",
				explanation: "Workflow files were present but exceeded the per-file size bound, so their steps were not read.",
				evidence: [],
				limitations: [
					{
						code: "FILE_TOO_LARGE",
						explanation: "One or more workflow files exceeded the per-file byte cap."
					}
				]
			}
		],
		composition: {
			counter: "tokei",
			counter_version: "14.0.0",
			exclusion_policy_version: "1",
			classification_policy_version: "1",
			total_files: 842,
			total_lines: 91204,
			code_lines: 78310,
			comment_lines: 8120,
			blank_lines: 4774,
			languages: [
				{
					language: "Rust",
					files: 512,
					code_lines: 48210,
					comment_lines: 6420,
					blank_lines: 3180
				},
				{
					language: "TypeScript",
					files: 214,
					code_lines: 19430,
					comment_lines: 1060,
					blank_lines: 1010
				}
			],
			areas: [
				{
					area: "crates/",
					code_lines: 51800
				},
				{
					area: "web/",
					code_lines: 26510
				}
			],
			exclusions: [
				{
					path_or_rule: "**/node_modules/**",
					reason: "Vendored dependencies are not this repository's code.",
					matched_rule: "vendor.node_modules",
					file_count: 126,
					bytes: 4182004
				}
			],
			roles: [
				{
					role: "PRODUCTION",
					files: 604,
					code_lines: 63400
				},
				{
					role: "TEST",
					files: 178,
					code_lines: 11710
				},
				{
					role: "GENERATED",
					files: 34,
					code_lines: 3200
				}
			],
			largest_files: [
				{
					path: "src/publication.rs",
					language: "Rust",
					code_lines: 2410,
					role: "PRODUCTION"
				},
				{
					path: "packages/api-client/src/schema.ts",
					language: "TypeScript",
					code_lines: 1980,
					role: "GENERATED"
				}
			],
			unclassified_files: 7
		},
		limitations: []
	}
} satisfies AnalysisFixture;

/** Fixture `failed-inaccessible.json`. */
export const FAILED_INACCESSIBLE_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		state: "FAILED_RETRIABLE",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: true
		},
		error: {
			code: "REPOSITORY_INACCESSIBLE",
			message: "The repository could not be read. This is usually temporary."
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `failed-permanent.json`. */
export const FAILED_PERMANENT_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		state: "FAILED_PERMANENT",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "This failure is deterministic: the same commit and ruleset will fail again."
		},
		error: {
			code: "ANALYZER_FAILED_PERMANENT",
			message: "The analyzer failed deterministically at this commit. Retrying would fail identically."
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `failed-repository-archived.json`. */
export const FAILED_REPOSITORY_ARCHIVED_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: null,
		state: "FAILED_PERMANENT",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "This failure is deterministic: the same commit and ruleset will fail again."
		},
		error: {
			code: "REPOSITORY_ARCHIVED",
			message: "This repository is archived. It can still be read, but it is not under active development, which is worth knowing before drawing conclusions from it."
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `failed-repository-not-found.json`. */
export const FAILED_REPOSITORY_NOT_FOUND_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: null,
		state: "FAILED_PERMANENT",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "This failure is deterministic: the same commit and ruleset will fail again."
		},
		error: {
			code: "REPOSITORY_NOT_FOUND",
			message: "No public repository was found at that address. Check the owner and name, and note that private repositories are not supported."
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `failed-repository-too-large.json`. */
export const FAILED_REPOSITORY_TOO_LARGE_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: null,
		state: "FAILED_PERMANENT",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "This failure is deterministic: the same commit and ruleset will fail again."
		},
		error: {
			code: "REPOSITORY_TOO_LARGE",
			message: "This repository is larger than the limits this analysis is allowed to spend. The limits are ours, not a judgement about the repository."
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `failed-retriable.json`. */
export const FAILED_RETRIABLE_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		state: "FAILED_RETRIABLE",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: true
		},
		error: {
			code: "RATE_LIMITED",
			message: "The GitHub rate limit is exhausted. The analysis will resume automatically.",
			retry_after_seconds: 900
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `failed-worker-retriable.json`. */
export const FAILED_WORKER_RETRIABLE_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		state: "FAILED_RETRIABLE",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: true
		},
		error: {
			code: "WORKER_FAILED_RETRIABLE",
			message: "The worker stopped before finishing. The analysis can be retried."
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `loc-unavailable.json`. */
export const LOC_UNAVAILABLE_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		state: "COMPLETED",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "the analysis succeeded"
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		report_available: true
	},
	report: {
		analysis_id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		tree_sha: "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
		analyzer_version: "0.1.0",
		ruleset_version: "1",
		completed_at: "2026-08-06T09:00:04Z",
		overview: [
			{
				statement: "Rust workspace with an Axum backend and a static SvelteKit frontend.",
				supporting_rule_ids: [
					"rust.workspace.detected"
				],
				confidence: "HIGH"
			}
		],
		findings: [
			{
				id: "0193a5c0-0000-7000-8000-000000000010",
				rule_id: "rust.workspace.detected",
				ruleset_version: "1",
				category: "TECHNOLOGY",
				state: "DETECTED",
				severity: "INFO",
				confidence: "HIGH",
				title: "Rust workspace detected",
				explanation: "The repository root declares a Cargo workspace.",
				evidence: [
					{
						kind: "FILE_EXCERPT",
						path: "Cargo.toml",
						excerpt: "[workspace]\nmembers = [\"crates/*\"]",
						truncated: true,
						digest: "sha256:6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b",
						line_range: {
							start: 1,
							end: 2
						}
					}
				],
				limitations: []
			},
			{
				id: "0193a5c0-0000-7000-8000-000000000011",
				rule_id: "docs.architecture.missing",
				ruleset_version: "1",
				category: "SOURCE_AND_DOCUMENTATION",
				state: "MISSING",
				severity: "LOW",
				confidence: "MEDIUM",
				title: "No architecture document found",
				explanation: "No docs/ARCHITECTURE file was found at the analyzed commit. Absence here means the boundaries a reader would need are not written down, not that the architecture is poor.",
				evidence: [
					{
						kind: "FILE_PRESENCE",
						path: "docs/",
						truncated: false
					}
				],
				limitations: [
					{
						code: "TREE_TRUNCATED",
						explanation: "The repository tree exceeded the traversal bound, so a document outside the collected paths would not have been seen."
					}
				],
				recommended_action: "Confirm by hand whether an architecture document exists outside the collected paths."
			},
			{
				id: "0193a5c0-0000-7000-8000-000000000012",
				rule_id: "ci.tests.unverifiable",
				ruleset_version: "1",
				category: "CI_CD",
				state: "UNABLE_TO_VERIFY",
				severity: "INFO",
				confidence: "LOW",
				title: "Could not determine whether CI runs tests",
				explanation: "Workflow files were present but exceeded the per-file size bound, so their steps were not read.",
				evidence: [],
				limitations: [
					{
						code: "FILE_TOO_LARGE",
						explanation: "One or more workflow files exceeded the per-file byte cap."
					}
				]
			}
		],
		composition: null,
		limitations: [
			{
				code: "EXTRACTION_STORAGE_LIMIT",
				explanation: "Archive extraction exceeded the configured storage limit, so no line counts were produced. This is not a claim that the repository has no code."
			}
		]
	}
} satisfies AnalysisFixture;

/** Fixture `queued.json`. */
export const QUEUED_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: null,
		state: "QUEUED",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "the analysis has not failed"
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		poll_after_ms: 2000,
		report_available: false
	}
} satisfies AnalysisFixture;

/** Fixture `resolving.json`. */
export const RESOLVING_FIXTURE = {
	analysis: {
		id: "0193a5c0-0000-7000-8000-000000000001",
		repository: {
			owner: "rust-lang",
			name: "crates.io"
		},
		commit_sha: null,
		state: "RESOLVING",
		execution: {
			trigger_status: "SUCCEEDED",
			execution_id: "exec-0193a5c0",
			triggered_at: "2026-08-06T09:00:00Z"
		},
		retry: {
			allowed: false,
			reason: "the analysis has not failed"
		},
		created_at: "2026-08-06T09:00:00Z",
		updated_at: "2026-08-06T09:00:04Z",
		poll_after_ms: 2000,
		report_available: false
	}
} satisfies AnalysisFixture;

/** Names of the available fixtures, for exhaustive scenario handling. */
export type AnalysisFixtureName =
	| "completed-report"
	| "failed-inaccessible"
	| "failed-permanent"
	| "failed-repository-archived"
	| "failed-repository-not-found"
	| "failed-repository-too-large"
	| "failed-retriable"
	| "failed-worker-retriable"
	| "loc-unavailable"
	| "queued"
	| "resolving";

/**
 * Every fixture, keyed by its file name.
 *
 * Keyed by file name rather than by an invented scenario label so that the map and the
 * directory listing can be compared without a translation table in between.
 *
 * Annotated `AnalysisFixture` rather than left to inference. The constants above keep
 * their exact literal types, which is what makes the `satisfies` on each of them a real
 * check; but a lookup into an inferred map would return a union of six unrelated shapes,
 * and reading `.report` off it would not compile for the fixtures that have no report.
 * The annotation hands consumers the contract type instead of the shape of the sample.
 */
export const ANALYSIS_FIXTURES: Readonly<Record<AnalysisFixtureName, AnalysisFixture>> = {
	"completed-report": COMPLETED_REPORT_FIXTURE,
	"failed-inaccessible": FAILED_INACCESSIBLE_FIXTURE,
	"failed-permanent": FAILED_PERMANENT_FIXTURE,
	"failed-repository-archived": FAILED_REPOSITORY_ARCHIVED_FIXTURE,
	"failed-repository-not-found": FAILED_REPOSITORY_NOT_FOUND_FIXTURE,
	"failed-repository-too-large": FAILED_REPOSITORY_TOO_LARGE_FIXTURE,
	"failed-retriable": FAILED_RETRIABLE_FIXTURE,
	"failed-worker-retriable": FAILED_WORKER_RETRIABLE_FIXTURE,
	"loc-unavailable": LOC_UNAVAILABLE_FIXTURE,
	"queued": QUEUED_FIXTURE,
	"resolving": RESOLVING_FIXTURE
};

/**
 * The same names as a value.
 *
 * Emitted as a literal rather than `Object.keys(ANALYSIS_FIXTURES)`, which would be typed
 * `string[]` and force every consumer into a cast back to `AnalysisFixtureName`.
 */
export const ANALYSIS_FIXTURE_NAMES = [
	"completed-report",
	"failed-inaccessible",
	"failed-permanent",
	"failed-repository-archived",
	"failed-repository-not-found",
	"failed-repository-too-large",
	"failed-retriable",
	"failed-worker-retriable",
	"loc-unavailable",
	"queued",
	"resolving"
] as const satisfies readonly AnalysisFixtureName[];
