/**
 * The report's sections, in order.
 *
 * One definition, consumed by the nav and by the page that renders the sections. Two lists
 * would be two chances for a nav link to point at an anchor that no longer exists — a dead
 * link that scrolls nowhere and, worse, moves focus nowhere.
 */
export interface ReportSectionLink {
	readonly id: string;
	readonly label: string;
}

export const REPORT_SECTIONS: readonly ReportSectionLink[] = [
	{ id: 'overview', label: 'Overview' },
	{ id: 'findings', label: 'Findings' },
	{ id: 'composition', label: 'Composition' },
	{ id: 'evidence', label: 'Evidence' }
];
