import AxeBuilder from '@axe-core/playwright';
import { COMPLETED_REPORT_FIXTURE } from '@repolens/api-client';
import { expect, test } from '@playwright/test';

import {
	ANALYSIS_ID,
	readCspViolations,
	recordApiRequests,
	recordCspViolations,
	serveScenario
} from './support/api-mock';

/**
 * The report route, end to end, against a production build.
 *
 * Runs in all three viewport projects (1280 / 768 / 360), so every assertion below —
 * including the axe scan and the no-horizontal-scroll check — is made at each breakpoint
 * without three copies of the test.
 */

const REPORT_URL = `/reports/${ANALYSIS_ID}`;
const REPORT = COMPLETED_REPORT_FIXTURE.report;

test.describe('a completed report', () => {
	test.beforeEach(async ({ page }) => {
		await recordCspViolations(page);
		await serveScenario(page, 'completed-report');
	});

	test('renders identity, findings and composition from the fixture', async ({ page }) => {
		await page.goto(REPORT_URL);

		await expect(page.getByRole('heading', { level: 1 })).toHaveText('Architecture report');
		await expect(page.getByText('rust-lang/crates.io').first()).toBeVisible();

		// The exact analyzed commit, short on screen and complete in `title`.
		await expect(page.locator(`[title="${REPORT.commit_sha}"]`)).toHaveText(
			REPORT.commit_sha.slice(0, 7)
		);

		// Analyzer and ruleset versions are first-class, not a footnote.
		await expect(page.getByText(`version ${REPORT.analyzer_version}`)).toBeVisible();

		for (const finding of REPORT.findings) {
			await expect(page.getByRole('heading', { name: finding.title })).toBeVisible();
		}

		// Composition, with the counter that produced it.
		await expect(page.getByText('tokei', { exact: false }).first()).toBeVisible();
		await expect(page.getByText('Counted: 842', { exact: false })).toBeVisible();
	});

	test('renders the accepted section hierarchy, not a flat findings block', async ({ page }) => {
		await page.goto(REPORT_URL);
		// The report resolves asynchronously; without this the DOM query below runs against
		// the loading state and reports an empty hierarchy as a passing one.
		await expect(page.getByRole('heading', { name: 'Evidence' })).toBeVisible();

		// Four category-led sections are first-class, between Overview and Findings.
		const headings = await page
			.locator('section[aria-labelledby] > h2')
			.evaluateAll((nodes) => nodes.map((node) => node.id));

		expect(headings).toEqual([
			'overview',
			'technology',
			'architecture',
			'composition',
			'engineering-system',
			'maintenance',
			'findings',
			'evidence'
		]);

		// Populated from the server's own categories: TECHNOLOGY reads under Technology,
		// and CI_CD and SOURCE_AND_DOCUMENTATION under Engineering system.
		const technology = page.locator('section[aria-labelledby="technology"]');
		await expect(technology.getByText('rust.workspace.detected')).toBeVisible();

		const engineering = page.locator('section[aria-labelledby="engineering-system"]');
		await expect(engineering.getByText('ci.tests.unverifiable')).toBeVisible();
		await expect(engineering.getByText('docs.architecture.missing')).toBeVisible();

		// A category the ruleset produced nothing for says so, honestly and without
		// borrowing a FindingState it did not earn.
		const architecture = page.locator('section[aria-labelledby="architecture"]');
		await expect(architecture.getByText('No finding in this report is categorised')).toBeVisible();

		// Every finding is a card exactly once, so no `finding-…` anchor is duplicated.
		const anchors = await page
			.locator('[id^="finding-"]')
			.evaluateAll((nodes) => nodes.map((node) => node.id));
		expect(anchors).toHaveLength(REPORT.findings.length);
		expect(new Set(anchors).size).toBe(anchors.length);
	});

	test('the findings index links to each card and moves focus to it', async ({ page }) => {
		await page.goto(REPORT_URL);

		const finding = REPORT.findings[0];
		const index = page.locator('section[aria-labelledby="findings"]');
		await index.getByRole('link', { name: finding?.title ?? '' }).click();

		const focused = await page.evaluate(() => document.activeElement?.id ?? null);
		expect(focused).toBe(`finding-${finding?.id}`);
	});

	test('the report route sends no mutation', async ({ page }) => {
		// Reading a report is anonymous by design and changes nothing. Asserted on the wire,
		// because "no button" and "no request" are different claims.
		const requests = recordApiRequests(page);
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		expect(requests().every((request) => request.method === 'GET')).toBe(true);
		expect(requests().length).toBeGreaterThan(0);
	});

	test('composition draws exactly two bars, and names the role of every large file', async ({
		page
	}) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const composition = page.locator('section[aria-labelledby="composition"]');

		// "Two charts, three tables" is meant literally. Asserted against the rendered
		// artifact rather than the component, because this is where the CSP applies.
		await expect(composition.locator('table.metric-table--bars')).toHaveCount(2);

		// Production / test / generated, from `LineCountSummary.roles`.
		await expect(composition.locator('[data-role="PRODUCTION"]').first()).toBeVisible();
		await expect(composition.locator('[data-role="TEST"]').first()).toBeVisible();

		// Largest files, with the role that stops a generated file reading as hand-written.
		const generated = REPORT.composition?.largest_files.find((file) => file.role === 'GENERATED');
		expect(generated, 'the fixture must carry a GENERATED largest file').toBeDefined();
		const files = composition.locator('.composition__table--files');
		await expect(files.getByText(generated?.path ?? '')).toBeVisible();
		await expect(files.locator('[data-role="GENERATED"]')).toHaveCount(1);

		// The exclusion ledger survives alongside them; it is required in addition to the
		// five views, never in place of largest files.
		await expect(composition.getByText('Excluded: 126', { exact: false })).toBeVisible();
	});

	test('has exactly one h1 and skips no heading levels', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Findings' })).toBeVisible();

		await expect(page.locator('h1')).toHaveCount(1);

		const levels = await page.evaluate(() =>
			[...document.querySelectorAll('h1, h2, h3, h4, h5, h6')].map((heading) =>
				Number(heading.tagName.slice(1))
			)
		);

		expect(levels[0]).toBe(1);
		for (let index = 1; index < levels.length; index += 1) {
			expect((levels[index] ?? 1) - (levels[index - 1] ?? 1)).toBeLessThanOrEqual(1);
		}
	});

	test('has no accessibility violations', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const results = await new AxeBuilder({ page })
			.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
			.analyze();

		expect(results.violations).toEqual([]);
	});

	test('does not scroll the body horizontally', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		// Wide content is allowed — it scrolls inside its own container, never the page.
		const overflows = await page.evaluate(
			() => document.documentElement.scrollWidth > document.documentElement.clientWidth
		);
		expect(overflows).toBe(false);
	});

	test('the LOC bars survive the Content Security Policy', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		/*
		 * The class of bug this catches: `--proportion` delivered as an inline `style`
		 * attribute. `style-src-attr` falls back to `style-src 'self'`, so the attribute is
		 * blocked — every LOC bar would silently render at zero width in production while
		 * passing every local check. `proportionBar` uses CSSOM instead, which the policy
		 * does not govern, and this asserts that it actually worked.
		 */
		const bar = await page
			.locator('.metric-cell')
			.first()
			.evaluate((node) => ({
				proportion: getComputedStyle(node).getPropertyValue('--proportion').trim(),
				width: Number.parseFloat(getComputedStyle(node, '::before').inlineSize)
			}));

		expect(Number(bar.proportion)).toBeGreaterThan(0);
		expect(bar.width).toBeGreaterThan(0);

		/*
		 * The only violation this app is allowed to report, and it is not ours: SvelteKit's
		 * router injects `#svelte-announcer` with a hardcoded inline `style` attribute from
		 * client JavaScript, which no CSP hash can cover. global.css restores the intended
		 * presentation from a stylesheet the policy permits; the report is left visible here
		 * deliberately, so the day it is fixed upstream this test tells us.
		 */
		const violations = await readCspViolations(page);
		expect(violations.every((entry) => entry === 'style-src-attr inline')).toBe(true);
	});

	test('a nav link moves focus to the section heading, not just the viewport', async ({ page }) => {
		await page.goto(REPORT_URL);

		await page.getByRole('link', { name: 'Composition' }).click();

		// The most commonly shipped accessibility bug of its kind: the page scrolls, focus
		// stays at the document root, and the next Tab starts from the top again.
		const focused = await page.evaluate(() => document.activeElement?.id ?? null);
		expect(focused).toBe('composition');
	});

	test('expanding all evidence reveals the excerpts', async ({ page }) => {
		await page.goto(REPORT_URL);

		const excerpt = page.getByText('members = ["crates/*"]');
		await expect(excerpt).toBeHidden();

		await page.getByRole('button', { name: 'Expand all evidence' }).click();

		// Visible content is content the browser's find-in-page can reach — which is the
		// whole reason this control exists.
		await expect(excerpt).toBeVisible();
	});

	test('the LOC bar is a background layer, and forced colours drop it', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const cell = page.locator('.metric-cell').first();

		const normal = await cell.evaluate((node) => {
			const bar = getComputedStyle(node, '::before');
			return { position: bar.position, display: bar.display };
		});
		// A pseudo-element behind the number, never a box that sizes it.
		expect(normal.position).toBe('absolute');
		expect(normal.display).not.toBe('none');

		await page.emulateMedia({ forcedColors: 'active' });

		const forced = await cell.evaluate((node) => getComputedStyle(node, '::before').display);
		// Windows High Contrast strips background colours anyway; dropping the bar
		// explicitly is the difference between degrading and breaking. The number stays.
		expect(forced).toBe('none');
		await expect(cell).not.toBeEmpty();

		await page.emulateMedia({ forcedColors: null });
	});
});

test.describe('a report with no line counts', () => {
	test.beforeEach(async ({ page }) => {
		await serveScenario(page, 'loc-unavailable');
	});

	test('renders UNABLE_TO_VERIFY and the report-level limitation, never zeros', async ({
		page
	}) => {
		await page.goto(REPORT_URL);

		const composition = page.locator('section[aria-labelledby="composition"]');
		await expect(composition).toBeVisible();

		await expect(composition.locator('[data-state="UNABLE_TO_VERIFY"]')).toBeVisible();
		await expect(composition.getByText('EXTRACTION_STORAGE_LIMIT')).toBeVisible();

		// A designed state, not an error and not a zero: "we could not count" must stay
		// distinguishable from "there is nothing to count".
		await expect(composition).not.toContainText(/\d/);
	});

	test('has no accessibility violations', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const results = await new AxeBuilder({ page })
			.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
			.analyze();

		expect(results.violations).toEqual([]);
	});
});
