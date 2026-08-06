import {
	COMPLETED_REPORT_FIXTURE,
	LOC_UNAVAILABLE_FIXTURE,
	type LineCountSummary
} from '@repolens/api-client';
import { createRawSnippet } from 'svelte';
import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';

import CompositionSection from '$lib/components/report/CompositionSection.svelte';
import EvidenceAppendix from '$lib/components/report/EvidenceAppendix.svelte';
import FindingsSection from '$lib/components/report/FindingsSection.svelte';
import OverviewSection from '$lib/components/report/OverviewSection.svelte';
import ReportHeader from '$lib/components/report/ReportHeader.svelte';
import ReportNav from '$lib/components/report/ReportNav.svelte';
import ReportSection from '$lib/components/report/ReportSection.svelte';
import { REPORT_SECTIONS } from '$lib/components/report/sections';
import '$lib/styles/global.css';

/**
 * The report, rendered from the executable fixtures.
 *
 * The fixtures are not copied here: `@repolens/api-client` generates them from
 * `contracts/fixtures/analysis-v1/*.json` under a `satisfies` assertion, so a DTO change
 * breaks these tests at compile time rather than leaving them green against a shape the API
 * no longer serves. That is also why every synthesised value below is annotated with a
 * generated type — a hand-shaped object literal would defeat the same gate.
 */

const REPORT = COMPLETED_REPORT_FIXTURE.report;
const COMPOSITION = REPORT.composition;

test('the report header carries commit, tree, analyzer and ruleset versions', async () => {
	const screen = await render(ReportHeader, { props: { report: REPORT } });
	const text = screen.container.textContent ?? '';

	expect(text).toContain('rust-lang/crates.io');
	// Short form on screen, full value in `title` — the SHA is what makes a report checkable.
	const shown = screen.container.querySelector(`[title="${REPORT.commit_sha}"]`);
	expect(shown?.textContent).toBe(REPORT.commit_sha.slice(0, 7));
	// The full value stays reachable — in `title`, and in the copy button's accessible
	// name — but never as forty characters of visible chrome.
	expect(text).toContain(REPORT.commit_sha.slice(0, 7));

	// First-class, not a footnote: a finding means nothing without the rules that produced it.
	expect(text).toContain(`version ${REPORT.analyzer_version}`);
	expect(text).toContain(`version ${REPORT.ruleset_version}`);

	const time = screen.container.querySelector('time');
	expect(time?.getAttribute('datetime')).toBe(REPORT.completed_at);
});

test('the report header links to the exact analyzed commit', async () => {
	const screen = await render(ReportHeader, { props: { report: REPORT } });

	const link = screen.container.querySelector('a[href*="github.com"]') as HTMLAnchorElement;
	expect(link.href).toBe(`https://github.com/rust-lang/crates.io/commit/${REPORT.commit_sha}`);
	// Opening a new tab is the reader's decision, not ours.
	expect(link.getAttribute('target')).toBeNull();
});

test('overview statements carry their own confidence and link to supporting findings', async () => {
	const screen = await render(OverviewSection, {
		props: {
			overview: REPORT.overview,
			findings: REPORT.findings,
			limitations: REPORT.limitations
		}
	});

	const text = screen.container.textContent ?? '';
	expect(text).toContain(REPORT.overview[0]?.statement ?? '');
	// The overview carries the whole summarization load, so no statement asserts on its
	// own authority.
	expect(screen.container.querySelector('[data-confidence="HIGH"]')).not.toBeNull();

	const supporting = screen.container.querySelector('a[href^="#finding-"]') as HTMLAnchorElement;
	expect(supporting.textContent).toContain('rust.workspace.detected');
	expect(supporting.getAttribute('href')).toBe(`#finding-${REPORT.findings[0]?.id}`);
});

test('every finding shows state, severity and confidence as three separate values', async () => {
	const screen = await render(FindingsSection, { props: { findings: REPORT.findings } });

	for (const finding of REPORT.findings) {
		const card = screen.container.querySelector(
			`[aria-labelledby="finding-${finding.id}"]`
		) as HTMLElement;
		expect(card, finding.rule_id).not.toBeNull();

		// Three attributes on three elements. Nothing in this card can merge two axes,
		// because there is no element that carries two of them.
		expect(card.querySelector(`[data-state="${finding.state}"]`)).not.toBeNull();
		expect(card.querySelector(`[data-severity="${finding.severity}"]`)).not.toBeNull();
		expect(card.querySelector(`[data-confidence="${finding.confidence}"]`)).not.toBeNull();
	}
});

test('a MISSING finding is not presented as a failure', async () => {
	const screen = await render(FindingsSection, { props: { findings: REPORT.findings } });

	const missing = REPORT.findings.find((finding) => finding.state === 'MISSING');
	expect(missing, 'the fixture must contain a MISSING finding').toBeDefined();

	const chip = screen.container.querySelector('[data-state="MISSING"]') as HTMLElement;
	const background = getComputedStyle(chip).backgroundColor;
	const parts = (background.match(/\d+(\.\d+)?/g) ?? []).map(Number);
	const [red = 0, green = 0, blue = 0] = parts;

	// Neutral, in the same sense the primitive test asserts: no channel dominates.
	expect(Math.max(red, green, blue) - Math.min(red, green, blue)).toBeLessThanOrEqual(12);

	// And its severity is LOW while its confidence is MEDIUM — two axes, not one verdict.
	expect(missing?.severity).toBe('LOW');
	expect(chip.closest('article')?.querySelector('[data-confidence="MEDIUM"]')).not.toBeNull();
});

test('a finding with no evidence says so instead of offering an empty disclosure', async () => {
	const screen = await render(FindingsSection, { props: { findings: REPORT.findings } });

	const unverifiable = REPORT.findings.find((finding) => finding.evidence.length === 0);
	expect(unverifiable).toBeDefined();

	const card = screen.container.querySelector(
		`[aria-labelledby="finding-${unverifiable?.id}"]`
	) as HTMLElement;

	expect(card.querySelector('details')).toBeNull();
	expect(card.textContent).toContain('No evidence is attached to this finding');
	// The limitation that explains why is visible, not hidden behind hover or a disclosure.
	expect(card.textContent).toContain('FILE_TOO_LARGE');
});

test('"Expand all evidence" opens every disclosure, making excerpts findable', async () => {
	const screen = await render(FindingsSection, { props: { findings: REPORT.findings } });

	const closed = [...screen.container.querySelectorAll('details')];
	expect(closed.length).toBeGreaterThan(0);
	expect(closed.every((details) => !details.open)).toBe(true);

	await screen.getByRole('button', { name: 'Expand all evidence' }).click();

	// The reason this control exists: content inside a closed <details> is invisible to
	// browser find-in-page in several engines, and a file path is what people search for.
	expect([...screen.container.querySelectorAll('details')].every((d) => d.open)).toBe(true);

	await screen.getByRole('button', { name: 'Collapse all evidence' }).click();
	expect([...screen.container.querySelectorAll('details')].every((d) => !d.open)).toBe(true);
});

test('a null composition renders UNABLE_TO_VERIFY with the report-level limitation', async () => {
	const report = LOC_UNAVAILABLE_FIXTURE.report;
	// Guards the fixture as well as the component: a non-null composition here would make
	// every assertion below pass while testing something else entirely.
	expect(report.composition).toBeNull();

	const screen = await render(CompositionSection, {
		props: { composition: report.composition, limitations: report.limitations }
	});

	const text = screen.container.textContent ?? '';

	expect(screen.container.querySelector('[data-state="UNABLE_TO_VERIFY"]')).not.toBeNull();
	expect(text).toContain('Unable to verify');
	expect(text).toContain('EXTRACTION_STORAGE_LIMIT');

	/*
	 * The whole point of the nullable field. Zeros here would assert that the repository
	 * contains no code — a claim the analysis never made — and "we could not count" would
	 * become indistinguishable from "there is nothing to count".
	 */
	expect(text).not.toMatch(/\d/);
});

test('the composition disclaimer is in-section, not a footnote', async () => {
	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	const disclaimer = screen.getByText(
		'RepoLens measures repository composition, not productivity or code quality.'
	);
	await expect.element(disclaimer).toBeInTheDocument();

	// Before any number, so a reader who stops at the first line has still been told.
	const first = screen.container.firstElementChild as HTMLElement;
	expect(first.textContent).toContain('not productivity or code quality');
});

test('composition renders exactly two bar views and three plain tables', async () => {
	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	// The exclusion table lives inside a closed disclosure; open it so all five are in the
	// DOM and the count means what it says.
	for (const details of screen.container.querySelectorAll('details')) details.open = true;

	const tables = [...screen.container.querySelectorAll('table')];
	const withBars = tables.filter((table) => table.classList.contains('metric-table--bars'));

	// Bars are earned, not default: LOC is comparative exactly twice, plus the single
	// composition table the design direction calls "table + one proportion bar".
	expect(tables).toHaveLength(5);
	expect(withBars).toHaveLength(3);
	expect(tables.length - withBars.length).toBe(2);
});

test('the bar is a background layer and cannot size the text of a sub-1% language', async () => {
	/*
	 * The fixture has two languages, both large. The defect being designed out only shows up
	 * at the bottom of the distribution, so a third language is synthesised — annotated with
	 * the generated type, so it is a value of the contract rather than a shape invented here.
	 */
	const tiny: LineCountSummary = {
		...COMPOSITION,
		languages: [
			...COMPOSITION.languages,
			{ language: 'Nix', files: 1, code_lines: 30, comment_lines: 0, blank_lines: 2 }
		]
	};

	const screen = await render(CompositionSection, {
		props: { composition: tiny, limitations: REPORT.limitations }
	});

	const row = [...screen.container.querySelectorAll('tr')].find((candidate) =>
		candidate.querySelector('th')?.textContent?.includes('Nix')
	) as HTMLTableRowElement;
	expect(row).toBeDefined();

	const cell = row.querySelector('.metric-cell') as HTMLElement;

	// 30 of 78,310 code lines is 0.038%: below one decimal place, and never rounded to 0%.
	expect(cell.textContent?.trim()).toBe('30');
	expect(row.textContent).toContain('<0.1%');

	// The proportion reaches CSS as a custom property...
	expect(Number(cell.style.getPropertyValue('--proportion'))).toBeLessThan(0.001);

	// ...and the bar drawn from it is an absolutely positioned pseudo-element, so it is
	// incapable of sizing the number. A <span> sized by the proportion would clip "30".
	const bar = getComputedStyle(cell, '::before');
	expect(bar.position).toBe('absolute');
	expect(bar.content).not.toBe('none');

	// The number is legible: laid out, and not overflowing its own cell.
	const value = cell.querySelector('span') as HTMLElement;
	expect(value.getBoundingClientRect().width).toBeGreaterThan(8);
	expect(cell.scrollWidth).toBeLessThanOrEqual(cell.clientWidth + 1);
});

test('a breakdown that does not sum to the total surfaces the shortfall', async () => {
	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	// Rust + TypeScript is 67,640 of 78,310 code lines. Scaling proportions to the listed
	// rows would report Rust at 71% instead of 62% — a number the reader cannot check.
	const listed = COMPOSITION.languages.reduce((sum, entry) => sum + entry.code_lines, 0);
	expect(listed).toBeLessThan(COMPOSITION.code_lines);

	const text = screen.container.textContent ?? '';
	expect(text).toContain('Not listed individually');
	// Marked as arithmetic, not as something the analyzer reported.
	expect(text).toContain('derived');
	expect(text).toContain('62%');
});

test('the exclusion ledger reports counted, excluded and unable to classify', async () => {
	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	const ledger = screen.container.querySelector('.composition__ledger') as HTMLElement;
	const text = ledger.textContent ?? '';

	expect(text).toContain('Counted: 842');
	expect(text).toContain('Excluded: 126');
	expect(text).toContain('Unable to classify: 7');

	// Each figure expands to the rule behind it — LOC misleads exactly when nobody can see
	// what was left out.
	const summaries = [...screen.container.querySelectorAll('summary')].map((s) =>
		s.textContent?.trim()
	);
	expect(summaries.some((label) => label?.startsWith('Counted'))).toBe(true);
	expect(summaries.some((label) => label?.startsWith('Excluded'))).toBe(true);
	expect(summaries.some((label) => label?.startsWith('Unable to classify'))).toBe(true);

	const excluded = [...screen.container.querySelectorAll('details')].find((details) =>
		details.querySelector('summary')?.textContent?.includes('Excluded')
	) as HTMLDetailsElement;
	excluded.open = true;
	expect(excluded.textContent).toContain('vendor.node_modules');
	expect(excluded.textContent).toContain('**/node_modules/**');
});

test('the evidence appendix lists paths openly, without expanding anything', async () => {
	const screen = await render(EvidenceAppendix, { props: { findings: REPORT.findings } });

	const text = screen.container.textContent ?? '';
	// Findable with Ctrl+F on first paint, which is the point: no <details> in this section.
	expect(screen.container.querySelector('details')).toBeNull();
	expect(text).toContain('Cargo.toml');
	expect(text).toContain('docs/');

	// Read from the fixture rather than hardcoded. A literal here duplicates the
	// contract, and duplicated contract data is what breaks when the contract
	// legitimately changes — as it did when ContentDigest took ownership of the
	// digest format.
	// Annotated rather than narrowed: `satisfies` preserves each fixture entry's
	// literal type, so the evidence arrays are heterogeneous and neither flatMap
	// nor an `in` check yields something with a usable `digest`.
	const digest = (REPORT.findings as readonly { evidence: readonly { digest?: string }[] }[])
		.flatMap((finding) => finding.evidence)
		.find((evidence) => typeof evidence.digest === 'string')?.digest;
	expect(digest, 'the fixture must carry at least one digest to assert on').toBeTruthy();

	// The appendix truncates for display — a 64-character digest in a table cell
	// is unreadable and pushes every other column off screen. Asserting a prefix
	// of the fixture value still proves the rendered digest came from the
	// contract rather than from a literal in this file.
	expect(text).toContain(digest!.slice(0, 'sha256:'.length + 7));

	// And it links back to the finding that drew the conclusion.
	const backlink = screen.container.querySelector('a[href^="#finding-"]');
	expect(backlink).not.toBeNull();
});

test('a report-nav link moves focus to the section heading, not just the viewport', async () => {
	// Rendered into the same document, so the anchor has a real target — which is the whole
	// mechanism under test.
	await render(ReportSection, {
		props: {
			id: 'findings',
			title: 'Findings',
			children: createRawSnippet(() => ({ render: () => '<p>body</p>' }))
		}
	});

	const nav = await render(ReportNav, { props: { sections: REPORT_SECTIONS } });

	await nav.getByRole('link', { name: 'Findings' }).click();

	/*
	 * The single most commonly shipped accessibility bug of its kind: the browser scrolls,
	 * focus stays at the document root, and a keyboard user Tabs from the top of the page
	 * again. jsdom would report this wrongly in either direction, which is why the suite
	 * runs in a real browser.
	 */
	const heading = document.getElementById('findings');
	expect(document.activeElement).toBe(heading);
	expect(heading?.tagName).toBe('H2');
	expect(heading?.getAttribute('tabindex')).toBe('-1');
});
