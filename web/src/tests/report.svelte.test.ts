import {
	COMPLETED_REPORT_FIXTURE,
	HANDLED_VARIANTS,
	LOC_UNAVAILABLE_FIXTURE,
	type Finding,
	type LargestSourceFile,
	type LineCountSummary,
	type RoleLineCount
} from '@repolens/api-client';
import { createRawSnippet } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';
import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';

import CategoryFindings from '$lib/components/report/CategoryFindings.svelte';
import CompositionSection from '$lib/components/report/CompositionSection.svelte';
import EvidenceAppendix from '$lib/components/report/EvidenceAppendix.svelte';
import EvidenceExpander from '$lib/components/report/EvidenceExpander.svelte';
import FindingsIndex from '$lib/components/report/FindingsIndex.svelte';
import OverviewSection from '$lib/components/report/OverviewSection.svelte';
import ReportHeader from '$lib/components/report/ReportHeader.svelte';
import ReportNav from '$lib/components/report/ReportNav.svelte';
import ReportSection from '$lib/components/report/ReportSection.svelte';
import {
	FINDING_CATEGORY_SECTION,
	findingsForSection,
	REPORT_SECTIONS,
	sectionForCategory,
	unplacedFindings,
	type CategorySectionId
} from '$lib/components/report/sections';
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

/** All findings in one shared expanded set, exactly as the route wires them. */
function renderCategory(findings: Finding[], emptyLabel = 'this category') {
	return render(CategoryFindings, {
		props: { findings, expanded: new SvelteSet<string>(), emptyLabel }
	});
}

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

test('the accepted hierarchy is present, in order, and every link has a section', async () => {
	/*
	 * The blocker this locks down: an earlier head exposed only Overview, Findings,
	 * Composition and Evidence, collapsing the whole report taxonomy into one flat findings
	 * block. Technology, Architecture, Engineering system and Maintenance are first-class
	 * sections in the accepted design, with Findings and Evidence after them.
	 */
	expect(REPORT_SECTIONS.map((section) => section.id)).toEqual([
		'overview',
		'technology',
		'architecture',
		'composition',
		'engineering-system',
		'maintenance',
		'findings',
		'evidence'
	]);

	// The nav renders exactly the sections, and nothing it links to is invented here.
	const nav = await render(ReportNav, { props: { sections: REPORT_SECTIONS } });
	const links = [...nav.container.querySelectorAll('a')];
	expect(links.map((link) => link.getAttribute('href'))).toEqual(
		REPORT_SECTIONS.map((section) => `#${section.id}`)
	);
});

test('every finding category maps to exactly one section, with none left over', () => {
	/*
	 * The same discipline as the contract package's label maps, and for the same reason: a
	 * category added to the Rust enum must not vanish from the reading hierarchy while still
	 * compiling. `Readonly<Record<FindingCategory, …>>` is the compile-time half; this is the
	 * half that survives someone widening the annotation.
	 *
	 * The list is read from `HANDLED_VARIANTS`, which `unknown-variant.test.ts` asserts
	 * against `contracts/openapi.json`. Nothing in the chain is typed out here.
	 */
	const categories = HANDLED_VARIANTS.FindingCategory ?? [];
	expect(categories.length).toBeGreaterThan(0);

	expect([...Object.keys(FINDING_CATEGORY_SECTION)].sort()).toEqual([...categories].sort());

	const sections = new Set<CategorySectionId>(Object.values(FINDING_CATEGORY_SECTION));
	// Four category-led sections, each with at least one category feeding it. A section that
	// no category can ever reach would be a permanently empty heading.
	expect([...sections].sort()).toEqual([
		'architecture',
		'engineering-system',
		'maintenance',
		'technology'
	]);
});

test('a category from a newer backend is placed nowhere and dropped from nothing', () => {
	// A statically hosted bundle outlives the build it was compiled against, so this is the
	// realistic case, not a hypothetical one.
	const future = {
		...(REPORT.findings[0] as Finding),
		category: 'SUPPLY_CHAIN'
	} as unknown as Finding;

	expect(sectionForCategory('SUPPLY_CHAIN')).toBeNull();
	// Inherited Object.prototype keys are not categories either.
	expect(sectionForCategory('constructor')).toBeNull();

	for (const section of [
		'technology',
		'architecture',
		'engineering-system',
		'maintenance'
	] as const) {
		expect(findingsForSection([future], section)).toEqual([]);
	}

	// Not dropped: it is routed to the Findings section instead, where it renders in full.
	expect(unplacedFindings([future, ...REPORT.findings])).toEqual([future]);
	expect(unplacedFindings(REPORT.findings)).toEqual([]);
});

test('the four category sections partition the findings, each card appearing once', async () => {
	const placed = (['technology', 'architecture', 'engineering-system', 'maintenance'] as const)
		.flatMap((section) => findingsForSection(REPORT.findings, section))
		.map((finding) => finding.id);

	// Exactly once, everywhere: a finding rendered in two sections would duplicate its
	// `finding-…` anchor id, which is both an accessibility failure and a link that lands
	// on whichever copy the browser reached first.
	expect(placed.sort()).toEqual(REPORT.findings.map((finding) => finding.id).sort());
	expect(new Set(placed).size).toBe(placed.length);

	// The fixture exercises both halves: a populated section and an empty one.
	expect(findingsForSection(REPORT.findings, 'technology')).toHaveLength(1);
	expect(findingsForSection(REPORT.findings, 'engineering-system')).toHaveLength(2);
	expect(findingsForSection(REPORT.findings, 'architecture')).toHaveLength(0);
});

test('an empty category section says so without borrowing a FindingState', async () => {
	const screen = await renderCategory([], 'architecture');
	const text = screen.container.textContent ?? '';

	expect(text).toContain('No finding in this report is categorised under architecture');
	// `MISSING` and `UNABLE_TO_VERIFY` are the analyzer's conclusions about a checked
	// property. "This ruleset produced nothing here" is a fact about the ruleset, and
	// dressing it in the contract's vocabulary would state something the analysis never did.
	expect(screen.container.querySelector('[data-state]')).toBeNull();
	expect(text).toContain('not a conclusion about the repository');
});

test('the findings index lists every finding once and links to its card', async () => {
	const screen = await render(FindingsIndex, { props: { findings: REPORT.findings } });

	const rows = [...screen.container.querySelectorAll('tbody tr')];
	expect(rows).toHaveLength(REPORT.findings.length);

	// Server order, preserved. Ordering is part of the contract.
	REPORT.findings.forEach((finding, index) => {
		const row = rows[index] as HTMLElement;
		expect(row.textContent).toContain(finding.title);
		expect(row.querySelector('a')?.getAttribute('href')).toBe(`#finding-${finding.id}`);
		// Three axes, three cells. An index that merged any two would be the merged-badge
		// defect, just wider.
		expect(row.querySelector(`[data-state="${finding.state}"]`)).not.toBeNull();
		expect(row.querySelector(`[data-severity="${finding.severity}"]`)).not.toBeNull();
		expect(row.querySelector(`[data-confidence="${finding.confidence}"]`)).not.toBeNull();
	});

	// An index, not a second set of cards: no `finding-…` anchor is defined here, so
	// nothing it links to is a duplicate of itself.
	expect(screen.container.querySelector('[id^="finding-"]')).toBeNull();
});

test('every finding shows state, severity and confidence as three separate values', async () => {
	const screen = await renderCategory(REPORT.findings);

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
	const screen = await renderCategory(REPORT.findings);

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
	const screen = await renderCategory(REPORT.findings);

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

test('"Expand all evidence" opens disclosures across sections it does not contain', async () => {
	/*
	 * The control now sits above four category sections rather than inside one, so the
	 * state it drives has to live outside every component that reads it. Two separately
	 * mounted trees sharing one `SvelteSet` is exactly how the route wires it, and the only
	 * arrangement that proves the expander reaches cards it does not render itself.
	 */
	const expanded = new SvelteSet<string>();
	const findings = findingsForSection(REPORT.findings, 'engineering-system');
	expect(findings.length).toBeGreaterThan(0);

	const section = await render(CategoryFindings, {
		props: { findings, expanded, emptyLabel: 'the engineering system' }
	});
	const controls = await render(EvidenceExpander, {
		props: { findings: REPORT.findings, expanded }
	});

	const closed = [...section.container.querySelectorAll('details')];
	expect(closed.length).toBeGreaterThan(0);
	expect(closed.every((details) => !details.open)).toBe(true);

	await controls.getByRole('button', { name: 'Expand all evidence' }).click();

	// The reason this control exists: content inside a closed <details> is invisible to
	// browser find-in-page in several engines, and a file path is what people search for.
	expect([...section.container.querySelectorAll('details')].every((d) => d.open)).toBe(true);

	await controls.getByRole('button', { name: 'Collapse all evidence' }).click();
	expect([...section.container.querySelectorAll('details')].every((d) => !d.open)).toBe(true);
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

test('exactly two views draw bars, and they are the two comparative ones', async () => {
	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	// The exclusion table lives inside a closed disclosure; open it so every table is in
	// the DOM and the count means what it says.
	for (const details of screen.container.querySelectorAll('details')) details.open = true;

	const tables = [...screen.container.querySelectorAll('table')];
	const withBars = tables.filter((table) => table.classList.contains('metric-table--bars'));

	/*
	 * "Two charts, three tables" is meant literally, and an earlier head drew three. Bars
	 * are earned by comparison across items, not by a value having a denominator: a single
	 * composition of a known whole is stated exactly by a share column, and a bar only
	 * approximates it.
	 */
	expect(withBars).toHaveLength(2);
	expect(withBars.map((table) => table.querySelector('caption')?.textContent?.trim())).toEqual([
		'Lines of code by language, as a share of all counted code lines.',
		'Lines of code by top-level area, as a share of all counted code lines.'
	]);

	// Five composition views plus the exclusion ledger's own table, which is required in
	// addition to them rather than in place of any.
	expect(tables).toHaveLength(6);

	// The per-language code/comment/blank table specifically: three series at once, where a
	// bar hides the small values and reads poorly aloud.
	const kinds = tables.find((table) =>
		table.querySelector('caption')?.textContent?.includes('Code, comment and blank lines')
	);
	expect(kinds?.classList.contains('metric-table--bars')).toBe(false);
	expect(kinds?.querySelector('.metric-cell')).toBeNull();
});

test('the role table renders production, test and generated from the contract', async () => {
	// Guards the fixture: an empty `roles` array would make every assertion below vacuous.
	expect(COMPOSITION.roles.length).toBeGreaterThan(0);

	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	const table = [...screen.container.querySelectorAll('table')].find((candidate) =>
		candidate.querySelector('caption')?.textContent?.includes('Code lines by role')
	) as HTMLTableElement;
	expect(table).toBeDefined();

	const rows = [...table.querySelectorAll('tbody tr')];
	expect(rows).toHaveLength(COMPOSITION.roles.length);

	COMPOSITION.roles.forEach((role, index) => {
		const row = rows[index] as HTMLElement;
		// The raw wire value is carried on the row, so an unrecognised role stays reportable.
		expect(row.querySelector(`[data-role="${role.role}"]`)).not.toBeNull();
		expect(row.textContent).toContain(role.code_lines.toLocaleString('en'));
		expect(row.textContent).toContain(role.files.toLocaleString('en'));
	});

	// PRODUCTION is 63,400 of 78,310 code lines.
	expect(rows[0]?.textContent).toContain('81%');
	// A single composition, stated by its share column. No bar.
	expect(table.classList.contains('metric-table--bars')).toBe(false);
});

test('the largest-file list names each role, so generated code is not read as hand-written', async () => {
	const generated = COMPOSITION.largest_files.find((file) => file.role === 'GENERATED');
	// Guards the fixture: this is the exact misreading the column exists to prevent, and
	// without a GENERATED row the assertion would pass while proving nothing.
	expect(generated, 'the fixture must contain a GENERATED largest file').toBeDefined();

	const screen = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});

	const table = [...screen.container.querySelectorAll('table')].find((candidate) =>
		candidate.querySelector('caption')?.textContent?.includes('largest source files')
	) as HTMLTableElement;
	expect(table).toBeDefined();

	const rows = [...table.querySelectorAll('tbody tr')];
	expect(rows).toHaveLength(COMPOSITION.largest_files.length);

	// Server order, descending, preserved rather than re-sorted here.
	COMPOSITION.largest_files.forEach((file, index) => {
		const row = rows[index] as HTMLElement;
		expect(row.textContent).toContain(file.path);
		expect(row.textContent).toContain(file.language);
		expect(row.querySelector(`[data-role="${file.role}"]`)).not.toBeNull();
	});

	const row = rows.find((candidate) => candidate.textContent?.includes(generated?.path ?? ''));
	expect(row?.textContent).toContain('Generated');
	// Long paths, not magnitudes: a bar would handle the label badly and add nothing.
	expect(table.querySelector('.metric-cell')).toBeNull();
});

test('an unrecognised role is named rather than dropped or crashed on', async () => {
	/*
	 * A statically hosted bundle outlives the build it was compiled against, so a browser
	 * can hold this one for months while `CodeRole` gains a variant. Both rows are annotated
	 * with the generated types and widened *only* at the enum, so everything except the
	 * unknown variant is still checked against the contract.
	 */
	const role = { role: 'VENDORED', files: 3, code_lines: 40 } as unknown as RoleLineCount;
	const file = {
		path: 'vendor/thing.rs',
		language: 'Rust',
		code_lines: 900,
		role: 'VENDORED'
	} as unknown as LargestSourceFile;

	const future: LineCountSummary = {
		...COMPOSITION,
		roles: [...COMPOSITION.roles, role],
		largest_files: [...COMPOSITION.largest_files, file]
	};

	const screen = await render(CompositionSection, {
		props: { composition: future, limitations: REPORT.limitations }
	});
	const text = screen.container.textContent ?? '';

	// Named, in a fallback that cannot be mistaken for a role this build understands.
	expect(text).toContain('Unrecognised (VENDORED)');
	expect(screen.container.querySelectorAll('[data-role="VENDORED"]')).toHaveLength(2);
	expect(text).toContain('vendor/thing.rs');
});

test('a role breakdown that leaves code unattributed says so, without inventing a role', async () => {
	const partial: LineCountSummary = {
		...COMPOSITION,
		roles: [{ role: 'PRODUCTION', files: 604, code_lines: 63400 }]
	};

	const screen = await render(CompositionSection, {
		props: { composition: partial, limitations: REPORT.limitations }
	});
	const text = (screen.container.textContent ?? '').replace(/\s+/g, ' ');

	expect(text).toContain('The listed roles account for 63,400 of 78,310 counted code lines');
	// `CodeRole` is a closed contract enum. A synthesised sixth row would put a value on
	// screen that the API can never send — the fabrication the pipeline exists to prevent.
	expect(screen.container.querySelector('[data-role="OTHER"]')).toBeNull();
	expect(text).not.toContain('Unattributed');
});

test('a breakdown summing to more than its own total is reported, not clamped away', async () => {
	/*
	 * The quiet failure: every proportion is clamped to [0, 1] independently, which is right
	 * for one bar and wrong for a set of them. Three rows at 60% of the same total render as
	 * three plausible bars whose sum is impossible, with nothing on screen saying so.
	 */
	const impossible: LineCountSummary = {
		...COMPOSITION,
		areas: [
			{ area: 'crates/', code_lines: 51800 },
			{ area: 'web/', code_lines: 51800 }
		]
	};

	const screen = await render(CompositionSection, {
		props: { composition: impossible, limitations: REPORT.limitations }
	});
	const text = (screen.container.textContent ?? '').replace(/\s+/g, ' ');

	expect(text).toContain('does not add up');
	expect(text).toContain('The per-area rows total 103,600 code lines, more than the 78,310');

	// And a consistent response says nothing of the sort.
	const clean = await render(CompositionSection, {
		props: { composition: COMPOSITION, limitations: REPORT.limitations }
	});
	expect(clean.container.textContent).not.toContain('does not add up');
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
