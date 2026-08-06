/**
 * The report's sections, in order, and which findings belong to each.
 *
 * One definition, consumed by the nav and by the page that renders the sections. Two lists
 * would be two chances for a nav link to point at an anchor that no longer exists — a dead
 * link that scrolls nowhere and, worse, moves focus nowhere.
 */

import type { Finding, FindingCategory } from '@repolens/api-client';

export type ReportSectionId =
	| 'overview'
	| 'technology'
	| 'architecture'
	| 'composition'
	| 'engineering-system'
	| 'maintenance'
	| 'findings'
	| 'evidence';

export interface ReportSectionLink {
	readonly id: ReportSectionId;
	readonly label: string;
}

/**
 * The accepted reading hierarchy.
 *
 * **Not a flat findings dump.** A report whose only structure is "Findings" makes the
 * reader do the grouping the ruleset already did, and it buries the two questions someone
 * opens an architecture report to answer — what is this built with, and how is it put
 * together — inside a list sorted by something else entirely.
 *
 * Composition sits directly after Architecture because it is evidence for the same
 * question: what the repository is made of. Findings and Evidence come last, as indexes
 * over material the reader has already met in context.
 */
export const REPORT_SECTIONS: readonly ReportSectionLink[] = [
	{ id: 'overview', label: 'Overview' },
	{ id: 'technology', label: 'Technology' },
	{ id: 'architecture', label: 'Architecture' },
	{ id: 'composition', label: 'Composition' },
	{ id: 'engineering-system', label: 'Engineering system' },
	{ id: 'maintenance', label: 'Maintenance' },
	{ id: 'findings', label: 'Findings' },
	{ id: 'evidence', label: 'Evidence' }
];

/** The four sections whose contents are findings, selected by category. */
export type CategorySectionId =
	'technology' | 'architecture' | 'engineering-system' | 'maintenance';

/**
 * Which section each `FindingCategory` reads under.
 *
 * **Nothing is invented here.** The contract carries eight categories and the accepted
 * design names four category-led sections, so this is a grouping of data the server already
 * sent — not a second taxonomy, and not a place to synthesise findings a section would
 * otherwise lack.
 *
 * `Readonly<Record<FindingCategory, …>>` is the gate, and it is the same one the contract
 * package uses for its label maps: a category added to the Rust enum leaves this literal
 * missing a key and fails `pnpm -r check`, rather than quietly vanishing from the reading
 * hierarchy while still appearing in the Findings index.
 *
 * The two judgement calls, stated rather than buried:
 *
 *   - `SOURCE_AND_DOCUMENTATION` reads under **Engineering system**. How a repository is
 *     laid out and documented is part of how it is worked on, alongside its build, tests
 *     and CI — not part of what it is architecturally.
 *   - `OPERATIONS` reads under **Maintenance**, with `SECURITY_AND_MAINTENANCE`. Both
 *     answer "what does keeping this alive cost", which is the question that section exists
 *     for.
 */
export const FINDING_CATEGORY_SECTION: Readonly<Record<FindingCategory, CategorySectionId>> = {
	TECHNOLOGY: 'technology',
	ARCHITECTURE: 'architecture',
	SOURCE_AND_DOCUMENTATION: 'engineering-system',
	BUILD_AND_DEPENDENCIES: 'engineering-system',
	TESTING: 'engineering-system',
	CI_CD: 'engineering-system',
	OPERATIONS: 'maintenance',
	SECURITY_AND_MAINTENANCE: 'maintenance'
};

/**
 * The section a finding reads under, or `null` when this build does not recognise its
 * category.
 *
 * `Object.hasOwn` rather than a bare lookup, for the reason the contract package gives:
 * `constructor` and `toString` are inherited from `Object.prototype`, and a bare lookup
 * would report them as recognised categories.
 */
export function sectionForCategory(category: string): CategorySectionId | null {
	return Object.hasOwn(FINDING_CATEGORY_SECTION, category)
		? FINDING_CATEGORY_SECTION[category as FindingCategory]
		: null;
}

/**
 * The findings that read under one section, in the server's order.
 *
 * Filtered, never sorted. "Ordering is part of the contract. A report that listed findings
 * differently on each load would contradict the determinism it claims."
 */
export function findingsForSection(
	findings: readonly Finding[],
	section: CategorySectionId
): Finding[] {
	return findings.filter((finding) => sectionForCategory(finding.category) === section);
}

/**
 * Findings whose category this build has never seen.
 *
 * A statically hosted bundle outlives the build it was compiled against, so a browser can
 * hold a cached copy for months while the API gains categories. Such a finding has no
 * section to read under — and dropping it would be exactly the silent omission the
 * unknown-variant policy forbids. It is rendered in full under Findings instead, so every
 * finding in the report appears as a card exactly once, wherever it belongs.
 */
export function unplacedFindings(findings: readonly Finding[]): Finding[] {
	return findings.filter((finding) => sectionForCategory(finding.category) === null);
}
