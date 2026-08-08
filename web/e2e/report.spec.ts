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

		// The exact analyzed commit, short on screen and complete in `title`. Scoped to the
		// report header because Composition now repeats commit and tree in its own provenance —
		// deliberately, so a screenshot of that section carries what makes it checkable.
		await expect(
			page.locator('.report-header').locator(`[title="${REPORT.commit_sha}"]`)
		).toHaveText(REPORT.commit_sha.slice(0, 7));

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

	test('the role share bar renders with real width under the production CSP', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		/*
		 * The failure this catches is production-only. A template-authored
		 * `style="--proportion: …"` is stripped by `style-src 'self'` with no
		 * `style-src-attr`, so every bar renders at zero width while passing every
		 * component test. Measuring the ::before box is the only assertion that can tell
		 * the difference.
		 */
		const measured = await page
			.locator('.composition__share')
			.first()
			.evaluate((node) => ({
				proportion: getComputedStyle(node).getPropertyValue('--proportion').trim(),
				width: Number.parseFloat(getComputedStyle(node, '::before').inlineSize),
				text: node.textContent?.trim() ?? ''
			}));

		expect(Number(measured.proportion)).toBeGreaterThan(0);
		expect(measured.width).toBeGreaterThan(0);
		// The percentage survives beside the bar rather than being sized by it.
		expect(measured.text).toMatch(/%/);
	});

	test('the role share bar degrades rather than breaks in forced colours', async ({ page }) => {
		// Windows High Contrast strips background colours wholesale, so the bar vanishes.
		// The percentage has to remain, which is what makes dropping the bar a degradation
		// rather than a loss of information.
		await page.emulateMedia({ forcedColors: 'active' });
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const cell = page.locator('.composition__share').first();
		const display = await cell.evaluate((node) => getComputedStyle(node, '::before').display);
		expect(display).toBe('none');
		await expect(cell).toContainText('%');
	});

	test('composition draws exactly two bars, and names the role of every large file', async ({
		page
	}) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const composition = page.locator('section[aria-labelledby="composition"]');

		// Exactly two *comparative* bar views. Asserted against the rendered artifact
		// rather than the component, because this is where the CSP applies. The role
		// table's embedded share bar is a different thing and is checked below.
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

	test('composition leads with its provenance, not with the chart', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		const composition = page.locator('section[aria-labelledby="composition"]');
		const key = composition.locator('.composition__key');

		/*
		 * Five values, and all five have to be there: commit and tree say what was counted,
		 * counter version and the two policy versions say how. Any four of them describe a
		 * number nobody else can reproduce, which is the difference between evidence and a
		 * claim.
		 *
		 * The count is asserted as well as the contents, because this test owns the property
		 * "the panel states its whole provenance". Without it, a fact could be deleted from
		 * the component and every remaining assertion would still pass.
		 *
		 * Each policy version is matched inside its own `.composition__key-fact` rather than
		 * against the panel. Both currently render `version 1`, so a panel-wide
		 * `toContainText('version 1')` is satisfied by either one — it would survive deleting
		 * the other, which is exactly the kind of assertion that reports a property it does
		 * not hold.
		 *
		 * What this still cannot catch: the two versions are both `1` today, so swapping the
		 * *values* between the two facts leaves every assertion here satisfied. Closing that
		 * would mean a fixture whose versions differ from the real policies, which trades a
		 * true fixture for a stronger test. The label-to-value pairing is asserted; the
		 * values being distinguishable is not, and is recorded rather than implied.
		 */
		await expect(key.locator('.composition__key-fact')).toHaveCount(5);

		await expect(key.locator(`[title="${REPORT.commit_sha}"]`)).toHaveText(
			REPORT.commit_sha.slice(0, 7)
		);
		await expect(key.locator(`[title="${REPORT.tree_sha}"]`)).toHaveText(
			REPORT.tree_sha.slice(0, 7)
		);
		await expect(key).toContainText(REPORT.composition?.counter ?? '');
		await expect(key).toContainText(REPORT.composition?.counter_version ?? '');

		await expect(
			key.locator('.composition__key-fact', { hasText: 'Exclusion policy' })
		).toContainText(`version ${REPORT.composition?.exclusion_policy_version}`);
		await expect(
			key.locator('.composition__key-fact', { hasText: 'Classification policy' })
		).toContainText(`version ${REPORT.composition?.classification_policy_version}`);

		// And it reads before the first table, because the ask was totals-then-detail and a
		// key that arrives after the numbers it qualifies has already failed to qualify them.
		const order = await composition.evaluate((section) => {
			const nodes = [...section.querySelectorAll('.composition__key, table')];
			return nodes.map((node) => (node.tagName === 'TABLE' ? 'table' : 'key'));
		});
		expect(order[0]).toBe('key');
	});

	test('every LOC bar is drawn against a visible track', async ({ page }) => {
		await page.goto(REPORT_URL);
		await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();

		/*
		 * A bar states a part. Without the unfilled rail behind it there is nothing on screen
		 * saying what the whole would be, so a 4% row and a 62% row are two lengths a reader
		 * has to calibrate by eye against the column edge. The track is the other half of the
		 * reading, and it is drawn from a token rather than implied.
		 */
		const measured = await page
			.locator('.metric-cell')
			.first()
			.evaluate((node) => {
				const fill = getComputedStyle(node, '::before');
				const track = getComputedStyle(node, '::after');
				return {
					fill: Number.parseFloat(fill.inlineSize),
					track: Number.parseFloat(track.inlineSize),
					fillColor: fill.backgroundColor,
					trackColor: track.backgroundColor
				};
			});

		expect(measured.track).toBeGreaterThan(measured.fill);
		expect(measured.fill).toBeGreaterThan(0);
		// Two different steps: a track painted in the fill's colour states 100% everywhere.
		expect(measured.trackColor).not.toBe(measured.fillColor);
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
