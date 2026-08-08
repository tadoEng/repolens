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
 *
 * A listed sum that *exceeds* the total gets no row here, because there is no honest one to
 * write — a negative remainder is not a category of code. That case is an inconsistency in
 * the response rather than a gap in the breakdown, and `overCountedBreakdowns` surfaces it
 * as one instead of letting each bar clamp independently and hide it.
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

/** One row of the production / test / generated table. */
export interface RoleRow {
	/** Raw wire value, so an unrecognised role can still be named on screen. */
	readonly role: string;
	readonly files: number;
	readonly codeLines: number;
	/** Share of the server-reported code lines, clamped to `[0, 1]`. */
	readonly proportion: number;
}

/**
 * Lines of code per role, in the server's order.
 *
 * Table three of three, and **no bar** — the acceptance criterion is two bar views, and the
 * two comparative ones earn them. This is a single composition of a known whole, which a
 * share column states exactly and a bar only approximates.
 *
 * No synthesised remainder row here, unlike the language and area breakdowns. `CodeRole` is
 * a closed contract enum, and appending a pseudo-role for whatever the policy did not
 * attribute would put a value on screen that the API can never send. A genuine shortfall is
 * reported as a sentence by `roleCoverage` instead.
 *
 * `UNCLASSIFIED` is dropped when it is zero, and only then. The other roles describe code —
 * `TEST` at zero says the repository has no tests, which is worth a row. `UNCLASSIFIED`
 * describes the *classifier*: at zero it says the policy accounted for everything, which is
 * the expected case and adds nothing to read. Above zero it is evidence, and it stays in
 * the table rather than moving to prose, because it participates in the denominator.
 *
 * **Shares are never renormalised over the remaining rows.** Every proportion stays against
 * `code_lines`, the total counted code. Rebasing them after dropping a row would let a
 * repository with 30% unattributed code display four roles summing to 100% — the same
 * uncertainty-hiding that making `Production` the residual used to do, moved into the
 * presentation layer.
 */
export function roleRows(composition: LineCountSummary): RoleRow[] {
	return composition.roles
		.filter((role) => role.role !== 'UNCLASSIFIED' || role.code_lines > 0)
		.map((role) => ({
			role: role.role,
			files: role.files,
			codeLines: role.code_lines,
			// Denominator is the server's own total, not the sum of the rows kept above.
			proportion: share(role.code_lines, composition.code_lines)
		}));
}

/** How much of the counted code the role breakdown accounts for. */
export interface RoleCoverage {
	readonly listed: number;
	readonly total: number;
	/** True when the listed roles account for every counted code line. */
	readonly complete: boolean;
}

export function roleCoverage(composition: LineCountSummary): RoleCoverage {
	const listed = composition.roles.reduce((sum, role) => sum + role.code_lines, 0);
	return {
		listed,
		total: composition.code_lines,
		// Whole lines: a sub-line discrepancy is floating point, not an unattributed file.
		complete: Math.abs(composition.code_lines - listed) < 1
	};
}

/** A breakdown whose listed rows add up to more than the server's own total. */
export interface OverCount {
	readonly label: string;
	readonly listed: number;
	readonly total: number;
}

/**
 * Breakdowns that over-count, so the response can say so rather than the bars hiding it.
 *
 * Each proportion is independently clamped to `[0, 1]`, which is the right behaviour for a
 * single bar and the wrong behaviour for a set of them: three rows at 60% of the same total
 * render as three plausible bars and one impossible sum. Clamping alone would leave the
 * reader with a chart that quietly does not add up. Naming the inconsistency costs one
 * sentence and is the difference between a limitation and a lie.
 */
export function overCountedBreakdowns(composition: LineCountSummary): OverCount[] {
	const total = composition.code_lines;
	const sums: OverCount[] = [
		{
			label: 'per-language',
			listed: composition.languages.reduce((sum, entry) => sum + entry.code_lines, 0),
			total
		},
		{
			label: 'per-area',
			listed: composition.areas.reduce((sum, entry) => sum + entry.code_lines, 0),
			total
		},
		{ label: 'per-role', listed: roleCoverage(composition).listed, total }
	];

	return sums.filter((entry) => entry.listed - entry.total >= 1);
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
