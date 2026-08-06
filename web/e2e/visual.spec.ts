import { existsSync } from 'node:fs';

import { expect, test, type TestInfo } from '@playwright/test';

import { ANALYSIS_ID, serveScenario } from './support/api-mock';

/**
 * Visual baselines — small, canonical, and deliberately not one per permutation.
 *
 * ## What this set is for
 *
 * This design system is carried almost entirely by typography, spacing and border weight:
 * one accent, no shadows, no gauges. Those are exactly the properties that assertions do
 * not catch. A token edit that halves the line height, a section that loses its rhythm, a
 * table that stops aligning its numbers — every functional test in this suite stays green.
 *
 * ## Why four, and not forty
 *
 * A screenshot per permutation produces baseline noise, and baseline noise gets approved
 * without being read, which is worse than having none. Four cover the design's real axes:
 *
 *   1. the completed report at desktop — the densest surface, and the reading measure;
 *   2. the completed report at 360px — where the tables and nav have to reflow;
 *   3. a retriable failure — the failure notice, chips and the progress timeline;
 *   4. a report with no line counts — the `UNABLE_TO_VERIFY` state, which must never
 *      resemble the populated one.
 *
 * Everything else about those pages is asserted by name in `report.spec.ts` and
 * `analysis.spec.ts`, where a failure says *what* broke rather than showing a picture.
 *
 * ## Determinism
 *
 * Locale and time zone are pinned, because the report renders `completed_at` through
 * `toLocaleString` in the reader's own zone — a machine in another zone would otherwise
 * fail on a difference that is not a regression. `reducedMotion` neutralises the progress
 * timeline's pulse, and the layout's system probe is waited for so a settling footer cannot
 * race the capture.
 *
 * ## Baselines are per platform
 *
 * Chromium rasterises text differently on Linux, macOS and Windows, so Playwright keys
 * snapshots by platform and there is no shared baseline that could be honest. A platform
 * with no committed baseline **skips with the exact command to produce one**, rather than
 * failing CI on an absence or — far worse — writing whatever it happens to render and
 * calling it approved.
 */

test.use({ locale: 'en-US', timezoneId: 'UTC', reducedMotion: 'reduce' });

const UPDATE = 'pnpm --filter @repolens/web exec playwright test visual.spec.ts --update-snapshots';

/**
 * Skip unless this platform and project have a committed baseline.
 *
 * `snapshotPath` resolves the same path `toHaveScreenshot` will use, platform suffix and
 * all, so this cannot disagree with the assertion it guards.
 *
 * The `updateSnapshots` check is what makes the guard compatible with creating a baseline
 * in the first place: `playwright.config.ts` pins it to `none`, and only an explicit
 * `--update-snapshots` — a person deciding to look at a new capture and approve it —
 * overrides that and lets the assertion through to write one.
 */
function requireBaseline(testInfo: TestInfo, name: string): void {
	if (testInfo.config.updateSnapshots !== 'none') return;

	test.skip(
		!existsSync(testInfo.snapshotPath(name)),
		`No visual baseline for ${process.platform}/${testInfo.project.name}. Generate one on this platform: ${UPDATE}`
	);
}

/** Everything a stable capture needs to have finished. */
async function settle(page: import('@playwright/test').Page): Promise<void> {
	// The footer probe resolves after paint and would otherwise race the screenshot. It
	// fails deterministically here: no API is running behind the preview server.
	await expect(page.getByText('API unavailable', { exact: false })).toBeVisible();
	await page.evaluate(() => document.fonts.ready);
}

const SHOT = { fullPage: true, animations: 'disabled' } as const;

test('completed report, desktop', async ({ page }, testInfo) => {
	test.skip(testInfo.project.name !== 'chromium', 'The desktop baseline is the 1280 project.');
	requireBaseline(testInfo, 'report-completed.png');

	await serveScenario(page, 'completed-report');
	await page.goto(`/reports/${ANALYSIS_ID}`);
	await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();
	await settle(page);

	await expect(page).toHaveScreenshot('report-completed.png', SHOT);
});

test('completed report, 360px', async ({ page }, testInfo) => {
	test.skip(testInfo.project.name !== 'mobile-360', 'The narrow baseline is the 360 project.');
	requireBaseline(testInfo, 'report-completed-narrow.png');

	await serveScenario(page, 'completed-report');
	await page.goto(`/reports/${ANALYSIS_ID}`);
	await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();
	await settle(page);

	await expect(page).toHaveScreenshot('report-completed-narrow.png', SHOT);
});

test('retriable failure progress', async ({ page }, testInfo) => {
	test.skip(testInfo.project.name !== 'chromium', 'One canonical capture, at desktop.');
	requireBaseline(testInfo, 'progress-failed-retriable.png');

	await serveScenario(page, 'failed-retriable');
	await page.goto(`/analyses/${ANALYSIS_ID}`);
	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();
	await settle(page);

	await expect(page).toHaveScreenshot('progress-failed-retriable.png', SHOT);
});

test('report with no line counts', async ({ page }, testInfo) => {
	test.skip(testInfo.project.name !== 'chromium', 'One canonical capture, at desktop.');
	requireBaseline(testInfo, 'report-loc-unavailable.png');

	await serveScenario(page, 'loc-unavailable');
	await page.goto(`/reports/${ANALYSIS_ID}`);
	await expect(page.getByRole('heading', { name: 'Composition' })).toBeVisible();
	await settle(page);

	await expect(page).toHaveScreenshot('report-loc-unavailable.png', SHOT);
});
