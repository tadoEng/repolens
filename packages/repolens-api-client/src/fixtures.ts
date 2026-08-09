/**
 * GENERATED FILE — DO NOT EDIT BY HAND.
 *
 * The executable fixtures, bound to TypeScript. Produced from the JSON under
 * `contracts/fixtures/` — itself generated from the Rust DTOs — by:
 *
 *     pnpm --filter @repolens/api-client fixtures:update
 *
 * Each fixture is emitted as a literal under a `satisfies` clause so the compiler checks it
 * against the generated schema. A JSON import could not: TypeScript widens string literals
 * in JSON modules, which would type every enum value as `string` and silently retire the
 * check that matters most.
 *
 * This is a binding, not a copy. `fixtures.test.ts` regenerates it and fails if the result
 * differs, so fixture content is authored in exactly one place — the JSON.
 */

import type { AdminFixture, AnalysisFixture } from './contract';

/** Fixture `analysis-v1/completed-report.json`. */
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
		evidence_source: {
			provider: "GITHUB_REST",
			api_version: "2026-03-10"
		},
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
					path: "packages/api-client/src/generated/schema.ts",
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

/** Fixture `analysis-v1/evidence-source-absent.json`. */
export const EVIDENCE_SOURCE_ABSENT_FIXTURE = {
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
		evidence_source: null,
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
					path: "packages/api-client/src/generated/schema.ts",
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

/** Fixture `analysis-v1/failed-inaccessible.json`. */
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

/** Fixture `analysis-v1/failed-permanent.json`. */
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

/** Fixture `analysis-v1/failed-repository-archived.json`. */
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

/** Fixture `analysis-v1/failed-repository-not-found.json`. */
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

/** Fixture `analysis-v1/failed-repository-too-large.json`. */
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

/** Fixture `analysis-v1/failed-retriable.json`. */
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

/** Fixture `analysis-v1/failed-worker-retriable.json`. */
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

/** Fixture `analysis-v1/loc-unavailable.json`. */
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
		evidence_source: {
			provider: "GITHUB_REST",
			api_version: "2026-03-10"
		},
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

/** Fixture `analysis-v1/queued.json`. */
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

/** Fixture `analysis-v1/resolving.json`. */
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

/** Names of the available `analysis-v1` fixtures, for exhaustive scenario handling. */
export type AnalysisFixtureName =
	| "completed-report"
	| "evidence-source-absent"
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
 * Every `analysis-v1` fixture, keyed by its file name.
 *
 * Keyed by file name rather than by an invented scenario label so that the map and the
 * directory listing can be compared without a translation table in between.
 *
 * Annotated `AnalysisFixture` rather than left to inference. The constants above keep
 * their exact literal types, which is what makes the `satisfies` on each of them a real
 * check; but a lookup into an inferred map would return a union of unrelated shapes, and
 * reading an optional field off it would not compile for the fixtures that lack it. The
 * annotation hands consumers the contract type instead of the shape of the sample.
 */
export const ANALYSIS_FIXTURES: Readonly<Record<AnalysisFixtureName, AnalysisFixture>> = {
	"completed-report": COMPLETED_REPORT_FIXTURE,
	"evidence-source-absent": EVIDENCE_SOURCE_ABSENT_FIXTURE,
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
 * Emitted as a literal rather than `Object.keys(ANALYSIS_FIXTURES)`, which would be
 * typed `string[]` and force every consumer into a cast back to `AnalysisFixtureName`.
 */
export const ANALYSIS_FIXTURE_NAMES = [
	"completed-report",
	"evidence-source-absent",
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

/** Fixture `admin-v1/overview.json`. */
export const ADMIN_OVERVIEW_FIXTURE = {
	process: {
		build_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		uptime_seconds: 93784,
		resident_bytes: 61849600
	},
	http: {
		in_flight: 1,
		tracked_routes: 5,
		max_tracked_routes: 64,
		routes: [
			{
				route: "/api/v1/analyses",
				method: "POST",
				requests: 412,
				responses: {
					informational: 0,
					success: 389,
					redirection: 0,
					client_error: 22,
					server_error: 1,
					other: 0
				},
				latency: {
					total_micros: 64070000,
					p50: {
						micros: 9800,
						lower_bound_micros: 5000,
						upper_bound_micros: 10000
					},
					p95: {
						micros: 31200,
						lower_bound_micros: 25000,
						upper_bound_micros: 50000
					},
					p99: {
						micros: 10000000,
						lower_bound_micros: 10000000,
						upper_bound_micros: null
					}
				}
			},
			{
				route: "/api/v1/analyses/{analysis_id}",
				method: "GET",
				requests: 7102,
				responses: {
					informational: 0,
					success: 7041,
					redirection: 0,
					client_error: 60,
					server_error: 1,
					other: 0
				},
				latency: {
					total_micros: 63918000,
					p50: {
						micros: 7900,
						lower_bound_micros: 5000,
						upper_bound_micros: 10000
					},
					p95: {
						micros: 22800,
						lower_bound_micros: 10000,
						upper_bound_micros: 25000
					},
					p99: {
						micros: 41500,
						lower_bound_micros: 25000,
						upper_bound_micros: 50000
					}
				}
			},
			{
				route: "/api/v1/system/probe",
				method: "GET",
				requests: 1241,
				responses: {
					informational: 0,
					success: 1241,
					redirection: 0,
					client_error: 0,
					server_error: 0,
					other: 0
				},
				latency: {
					total_micros: 9928000,
					p50: {
						micros: 6800,
						lower_bound_micros: 5000,
						upper_bound_micros: 10000
					},
					p95: {
						micros: 17400,
						lower_bound_micros: 10000,
						upper_bound_micros: 25000
					},
					p99: {
						micros: 33100,
						lower_bound_micros: 25000,
						upper_bound_micros: 50000
					}
				}
			},
			{
				route: "/healthz",
				method: "GET",
				requests: 6814,
				responses: {
					informational: 0,
					success: 6814,
					redirection: 0,
					client_error: 0,
					server_error: 0,
					other: 0
				},
				latency: {
					total_micros: 6132600,
					p50: {
						micros: 720,
						lower_bound_micros: 500,
						upper_bound_micros: 1000
					},
					p95: {
						micros: 2100,
						lower_bound_micros: 1000,
						upper_bound_micros: 2500
					},
					p99: {
						micros: 4400,
						lower_bound_micros: 2500,
						upper_bound_micros: 5000
					}
				}
			},
			{
				route: "<unmatched>",
				method: "GET",
				requests: 37,
				responses: {
					informational: 0,
					success: 0,
					redirection: 0,
					client_error: 37,
					server_error: 0,
					other: 0
				},
				latency: {
					total_micros: 11100,
					p50: {
						micros: 260,
						lower_bound_micros: 0,
						upper_bound_micros: 500
					},
					p95: {
						micros: 420,
						lower_bound_micros: 0,
						upper_bound_micros: 500
					},
					p99: {
						micros: 480,
						lower_bound_micros: 0,
						upper_bound_micros: 500
					}
				}
			}
		]
	}
} satisfies AdminFixture;

/** Fixture `admin-v1/overview-memory-unavailable.json`. */
export const ADMIN_OVERVIEW_MEMORY_UNAVAILABLE_FIXTURE = {
	process: {
		build_sha: "0584a2df65968a4e9e6859ef46bbed430408a3f1",
		uptime_seconds: 93784,
		resident_bytes: null
	},
	http: {
		in_flight: 1,
		tracked_routes: 5,
		max_tracked_routes: 64,
		routes: [
			{
				route: "/api/v1/analyses",
				method: "POST",
				requests: 412,
				responses: {
					informational: 0,
					success: 389,
					redirection: 0,
					client_error: 22,
					server_error: 1,
					other: 0
				},
				latency: {
					total_micros: 64070000,
					p50: {
						micros: 9800,
						lower_bound_micros: 5000,
						upper_bound_micros: 10000
					},
					p95: {
						micros: 31200,
						lower_bound_micros: 25000,
						upper_bound_micros: 50000
					},
					p99: {
						micros: 10000000,
						lower_bound_micros: 10000000,
						upper_bound_micros: null
					}
				}
			},
			{
				route: "/api/v1/analyses/{analysis_id}",
				method: "GET",
				requests: 7102,
				responses: {
					informational: 0,
					success: 7041,
					redirection: 0,
					client_error: 60,
					server_error: 1,
					other: 0
				},
				latency: {
					total_micros: 63918000,
					p50: {
						micros: 7900,
						lower_bound_micros: 5000,
						upper_bound_micros: 10000
					},
					p95: {
						micros: 22800,
						lower_bound_micros: 10000,
						upper_bound_micros: 25000
					},
					p99: {
						micros: 41500,
						lower_bound_micros: 25000,
						upper_bound_micros: 50000
					}
				}
			},
			{
				route: "/api/v1/system/probe",
				method: "GET",
				requests: 1241,
				responses: {
					informational: 0,
					success: 1241,
					redirection: 0,
					client_error: 0,
					server_error: 0,
					other: 0
				},
				latency: {
					total_micros: 9928000,
					p50: {
						micros: 6800,
						lower_bound_micros: 5000,
						upper_bound_micros: 10000
					},
					p95: {
						micros: 17400,
						lower_bound_micros: 10000,
						upper_bound_micros: 25000
					},
					p99: {
						micros: 33100,
						lower_bound_micros: 25000,
						upper_bound_micros: 50000
					}
				}
			},
			{
				route: "/healthz",
				method: "GET",
				requests: 6814,
				responses: {
					informational: 0,
					success: 6814,
					redirection: 0,
					client_error: 0,
					server_error: 0,
					other: 0
				},
				latency: {
					total_micros: 6132600,
					p50: {
						micros: 720,
						lower_bound_micros: 500,
						upper_bound_micros: 1000
					},
					p95: {
						micros: 2100,
						lower_bound_micros: 1000,
						upper_bound_micros: 2500
					},
					p99: {
						micros: 4400,
						lower_bound_micros: 2500,
						upper_bound_micros: 5000
					}
				}
			},
			{
				route: "<unmatched>",
				method: "GET",
				requests: 37,
				responses: {
					informational: 0,
					success: 0,
					redirection: 0,
					client_error: 37,
					server_error: 0,
					other: 0
				},
				latency: {
					total_micros: 11100,
					p50: {
						micros: 260,
						lower_bound_micros: 0,
						upper_bound_micros: 500
					},
					p95: {
						micros: 420,
						lower_bound_micros: 0,
						upper_bound_micros: 500
					},
					p99: {
						micros: 480,
						lower_bound_micros: 0,
						upper_bound_micros: 500
					}
				}
			}
		]
	}
} satisfies AdminFixture;

/** Names of the available `admin-v1` fixtures, for exhaustive scenario handling. */
export type AdminFixtureName =
	| "overview"
	| "overview-memory-unavailable";

/**
 * Every `admin-v1` fixture, keyed by its file name.
 *
 * Keyed by file name rather than by an invented scenario label so that the map and the
 * directory listing can be compared without a translation table in between.
 *
 * Annotated `AdminFixture` rather than left to inference. The constants above keep
 * their exact literal types, which is what makes the `satisfies` on each of them a real
 * check; but a lookup into an inferred map would return a union of unrelated shapes, and
 * reading an optional field off it would not compile for the fixtures that lack it. The
 * annotation hands consumers the contract type instead of the shape of the sample.
 */
export const ADMIN_FIXTURES: Readonly<Record<AdminFixtureName, AdminFixture>> = {
	"overview": ADMIN_OVERVIEW_FIXTURE,
	"overview-memory-unavailable": ADMIN_OVERVIEW_MEMORY_UNAVAILABLE_FIXTURE
};

/**
 * The same names as a value.
 *
 * Emitted as a literal rather than `Object.keys(ADMIN_FIXTURES)`, which would be
 * typed `string[]` and force every consumer into a cast back to `AdminFixtureName`.
 */
export const ADMIN_FIXTURE_NAMES = [
	"overview",
	"overview-memory-unavailable"
] as const satisfies readonly AdminFixtureName[];
