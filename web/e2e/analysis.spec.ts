import AxeBuilder from '@axe-core/playwright';
import { FAILED_PERMANENT_FIXTURE, FAILED_RETRIABLE_FIXTURE } from '@repolens/api-client';
import { expect, test } from '@playwright/test';

import { ANALYSIS_ID, recordApiRequests, serveScenario } from './support/api-mock';

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

test('a retriable failure explains why no retry is offered, and never starts one', async ({
	page
}) => {
	// The server says a retry would be accepted. This build still offers no control, and
	// the reason is on screen rather than implied by an empty space.
	expect(FAILED_RETRIABLE_FIXTURE.analysis.retry.allowed).toBe(true);

	const requests = recordApiRequests(page);
	await serveScenario(page, 'failed-retriable');
	await page.goto(PROGRESS_URL);

	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();
	// The server's message, verbatim.
	await expect(
		page.getByText(FAILED_RETRIABLE_FIXTURE.analysis.error?.message ?? '')
	).toBeVisible();
	await expect(page.getByText('15 minutes')).toBeVisible();

	await expect(page.getByRole('button', { name: /retry/i })).toHaveCount(0);
	await expect(page.getByText('Retry is not available in this build')).toBeVisible();
	await expect(page.getByText('the published API contract does not define yet')).toBeVisible();

	/*
	 * The blocker, asserted at the only layer that can prove it: the wire. A retry request
	 * is an authenticated mutation that starts paid work, and nothing in this build sends
	 * one — not on load, and not from any control, because there is no control.
	 *
	 * The layout's system probe also hits this origin, so the assertion is about *methods*
	 * rather than a request count.
	 */
	const sent = requests();
	expect(sent.length).toBeGreaterThan(0);
	expect(sent.filter((request) => request.method !== 'GET')).toEqual([]);
	expect(sent.filter((request) => request.url.includes('/retry'))).toEqual([]);
});

test('the failure notice takes focus deterministically once it resolves', async ({ page }) => {
	await serveScenario(page, 'failed-retriable');
	await page.goto(PROGRESS_URL);

	const heading = page.getByRole('heading', { name: 'This analysis failed' });
	await expect(heading).toBeVisible();

	/*
	 * The route opens in a loading state and resolves asynchronously, so without this a
	 * keyboard or screen-reader user is left on `<body>` while the most important thing on
	 * the page appears below the fold. This is the "deterministic focus" criterion the
	 * design asks for after a retry; the retry is gone, the criterion is not.
	 */
	await expect(heading).toBeFocused();
	const focused = await page.evaluate(() => ({
		id: document.activeElement?.id ?? null,
		tag: document.activeElement?.tagName ?? null,
		tabindex: document.activeElement?.getAttribute('tabindex') ?? null
	}));
	expect(focused).toEqual({ id: 'failure', tag: 'H2', tabindex: '-1' });

	// And it does not fight the reader: focus moves once, not on every poll.
	await page.keyboard.press('Tab');
	await expect(heading).not.toBeFocused();
});

test('a permanent failure offers no retry and explains why', async ({ page }) => {
	expect(FAILED_PERMANENT_FIXTURE.analysis.retry.allowed).toBe(false);

	await serveScenario(page, 'failed-permanent');
	await page.goto(PROGRESS_URL);

	await expect(page.getByRole('heading', { name: 'This analysis failed' })).toBeVisible();
	await expect(page.getByRole('button', { name: /retry/i })).toHaveCount(0);

	// Displayed verbatim: it explains rather than merely denies.
	await expect(page.getByText(FAILED_PERMANENT_FIXTURE.analysis.retry.reason ?? '')).toBeVisible();
	// "The server refused" and "this build cannot ask yet" are different facts. A permanent
	// failure must not be explained away as a gap in the frontend.
	await expect(page.getByText('in this build')).toHaveCount(0);
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
