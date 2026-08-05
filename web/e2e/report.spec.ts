import AxeBuilder from '@axe-core/playwright';
import { COMPLETED_REPORT_FIXTURE } from '@repolens/api-client';
import { expect, test } from '@playwright/test';

import {
	ANALYSIS_ID,
	readCspViolations,
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
