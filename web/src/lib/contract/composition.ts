/**
 * Derivations over `LineCountSummary`.
 *
 * Everything here is arithmetic on numbers the server sent. Nothing is invented, and one
 * thing is deliberately *not* hidden: **a listed breakdown need not sum to the reported
 * total.** `languages` and `areas` are "server-ordered" breakdowns, and the contract does
 * not promise they are exhaustive.
 *
 * That matters more than it sounds. If proportions were computed against the sum of the
 * listed rows, a language holding 61% of the codebase would render as 71% merely because
 * the other 14% was not itemised — a number the reader cannot check, presented as if they
 * could. So every proportion is taken against the server's own total, and any shortfall is
 * surfaced as its own labelled row rather than being distributed silently across the rest.
 */

import type { LineCountSummary } from '@repolens/api-client';

export type { LineCountSummary };

/** One row of a metric table, with the bar proportion already resolved. */
export interface MetricRow {
	readonly label: string;
	readonly value: number;
	/** Share of the server-reported total, clamped to `[0, 1]`. */
	readonly proportion: number;
	/**
	 * True for the synthesised shortfall row. The table marks it, because it is arithmetic
	 * done here rather than a figure the analyzer reported.
	 */
	readonly derived?: boolean;
}

function share(value: number, total: number): number {
	if (!Number.isFinite(value) || !Number.isFinite(total) || total <= 0) return 0;
	return Math.min(1, Math.max(0, value / total));
}

/**
 * Append the shortfall between a breakdown and its total, when there is one.
 *
 * Only when it is a whole line or more: floating point noise is not a finding.
 */
function withRemainder(rows: MetricRow[], total: number, label: string): MetricRow[] {
	const listed = rows.reduce((sum, row) => sum + row.value, 0);
	const remainder = total - listed;
	if (remainder < 1) return rows;

	return [...rows, { label, value: remainder, proportion: share(remainder, total), derived: true }];
}

/**
 * Lines of code per language, as a share of the repository's total code lines.
 *
 * Chart one of two. LOC is genuinely comparative across a handful of languages, which is
 * what earns a bar here where the rest of the report uses prose and tables.
 */
export function languageCodeRows(composition: LineCountSummary): MetricRow[] {
	const rows = composition.languages.map((language) => ({
		label: language.language,
		value: language.code_lines,
		proportion: share(language.code_lines, composition.code_lines)
	}));

	return withRemainder(rows, composition.code_lines, 'Not listed individually');
}

/**
 * Lines of code per top-level area.
 *
 * Chart two of two, and the more architecturally useful of the pair: it answers
 * frontend-heavy, test-heavy, or tooling-dominated at a glance.
 */
export function areaCodeRows(composition: LineCountSummary): MetricRow[] {
	const rows = composition.areas.map((area) => ({
		label: area.area,
		value: area.code_lines,
		proportion: share(area.code_lines, composition.code_lines)
	}));

	return withRemainder(rows, composition.code_lines, 'Not listed individually');
}

/**
 * Code, comment and blank lines for the repository as a whole.
 *
 * A single composition rather than a comparison, which is why it is a table with one
 * proportion bar rather than a third chart.
 */
export function lineKindRows(composition: LineCountSummary): MetricRow[] {
	const total = composition.total_lines;
	return [
		{
			label: 'Code',
			value: composition.code_lines,
			proportion: share(composition.code_lines, total)
		},
		{
			label: 'Comments',
			value: composition.comment_lines,
			proportion: share(composition.comment_lines, total)
		},
		{
			label: 'Blank',
			value: composition.blank_lines,
			proportion: share(composition.blank_lines, total)
		}
	];
}

/**
 * The exclusion ledger: counted, excluded, unable to classify.
 *
 * First-class rather than a footnote, because LOC is where composition reporting usually
 * lies, and it lies by omission. `unclassified` maps onto the existing `UNABLE_TO_VERIFY`
 * state — there is no sixth state for it.
 */
export interface ExclusionLedger {
	readonly counted: number;
	readonly excluded: number;
	readonly excludedBytes: number;
	readonly unclassified: number;
}

export function exclusionLedger(composition: LineCountSummary): ExclusionLedger {
	const excluded = composition.exclusions.reduce((sum, rule) => sum + rule.file_count, 0);
	const excludedBytes = composition.exclusions.reduce((sum, rule) => sum + rule.bytes, 0);

	return {
		counted: composition.total_files,
		excluded,
		excludedBytes,
		unclassified: composition.unclassified_files
	};
}
