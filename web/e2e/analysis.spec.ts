import AxeBuilder from '@axe-core/playwright';
import { FAILED_PERMANENT_FIXTURE, FAILED_RETRIABLE_FIXTURE } from '@repolens/api-client';
import { expect, test } from '@playwright/test';

import { ANALYSIS_ID, serveScenario } from './support/api-mock';

/**
 * The progress route, end to end, against a production build.
 *
 * Anonymous by design: none of these tests signs in, because holding the unguessable
 * analysis ID *is* the capability. A progress page that needed a session would not be
 * shareable, which is the point of the route.
 */

const PROGRESS_URL = `/analyses/${ANALYSIS_ID}`;

test('a queued analysis shows "resolving…" rather than a blank commit', async ({ page }) => {
	await serveScenario(page, 'queued');
	await page.goto(PROGRESS_URL);

	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Analysis progress');
	// `commit_sha` is null until RESOLVING completes. A blank here reads as a bug.
	await expect(page.getByText('resolving…')).toBeVisible();
	await expect(page.getByText('rust-lang/crates.io')).toBeVisible();

	await expect(page.getByText('Step 1 of 5')).toBeVisible();
	// Nothing has failed, so nothing offers a retry.
	await expect(page.getByRole('button', { name: /retry/i })).toHaveCount(0);
});

test('a retriable failure offers retry, because the server allowed it', async ({ page }) => {
	expect(FAILED_RETRIABLE_FIXTURE.analysis.retry.allowed).toBe(true);

	await serveScenario(page, 'failed-retriable');
	await page.goto(PROGRESS_URL);

	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();
	// The server's message, verbatim.
	await expect(
		page.getByText(FAILED_RETRIABLE_FIXTURE.analysis.error?.message ?? '')
	).toBeVisible();
	await expect(page.getByText('15 minutes')).toBeVisible();

	const retry = page.getByRole('button', { name: 'Retry this analysis' });
	await expect(retry).toBeVisible();
	// Keyboard-operable, and reachable without a pointer.
	await retry.focus();
	await expect(retry).toBeFocused();
});

test('a permanent failure offers no retry and explains why', async ({ page }) => {
	expect(FAILED_PERMANENT_FIXTURE.analysis.retry.allowed).toBe(false);

	await serveScenario(page, 'failed-permanent');
	await page.goto(PROGRESS_URL);

	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();
	await expect(page.getByRole('button', { name: /retry/i })).toHaveCount(0);

	// Displayed verbatim: it explains rather than merely denies.
	await expect(page.getByText(FAILED_PERMANENT_FIXTURE.analysis.retry.reason ?? '')).toBeVisible();
});

test('a completed analysis links to its report', async ({ page }) => {
	await serveScenario(page, 'completed-report');
	await page.goto(PROGRESS_URL);

	// `report_available`, not `state === COMPLETED`: the two are separate facts.
	const link = page.getByRole('link', { name: 'Open the report for this analysis' });
	await expect(link).toBeVisible();
	await expect(link).toHaveAttribute('href', `/reports/${ANALYSIS_ID}`);
});

test('reduced motion leaves a static, text-carried progress indicator', async ({ page }) => {
	await page.emulateMedia({ reducedMotion: 'reduce' });
	await serveScenario(page, 'resolving');
	await page.goto(PROGRESS_URL);

	// The active step's marker normally animates. Under reduced motion the animation is
	// neutralised, so the state has to be carried by the label — and it is, always.
	await expect(page.getByText('In progress')).toBeVisible();
	await expect(page.getByText('Step 2 of 5')).toBeVisible();

	const duration = await page
		.locator('.timeline__step--active .timeline__marker')
		.evaluate((node) => getComputedStyle(node).animationDuration);
	// global.css clamps every animation to 0.01ms under the media query.
	expect(Number.parseFloat(duration)).toBeLessThan(0.001);

	await page.emulateMedia({ reducedMotion: null });
});

test('the progress route has no accessibility violations', async ({ page }) => {
	await serveScenario(page, 'failed-retriable');
	await page.goto(PROGRESS_URL);
	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();

	const results = await new AxeBuilder({ page })
		.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
		.analyze();

	expect(results.violations).toEqual([]);
});

test('the progress route does not scroll the body horizontally', async ({ page }) => {
	await serveScenario(page, 'failed-retriable');
	await page.goto(PROGRESS_URL);
	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();

	const overflows = await page.evaluate(
		() => document.documentElement.scrollWidth > document.documentElement.clientWidth
	);
	expect(overflows).toBe(false);
});
