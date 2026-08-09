import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

import { refuseAdminOverview, serveAdminOverview } from './support/api-mock';

/**
 * The operational dashboard.
 *
 * ## What this suite asserts, and what it deliberately does not
 *
 * The functional contract is `ErrorCode → remedy → affordance`. A refusal decides *which
 * controls exist*, and the server decides *what sentence explains it*. So every assertion
 * here is about a control, a heading, or a section — never about the server's prose.
 *
 * That division is the point. Rewording a Rust string is visible product output and moves a
 * screenshot; it must not move a behavioural test. A suite that matched "This account is not
 * permitted…" would make punctuation in `api/authenticated.rs` a breaking change, and the
 * first fix anyone reached for would be to loosen the assertion until it proved nothing.
 *
 * ## Why the page is reachable at all without signing in
 *
 * There is no Firebase configuration in CI, so the session resolves to `unavailable` and the
 * page sends an anonymous request. That is the designed behaviour rather than a test
 * convenience: Axum is the authorisation boundary, and a page that refused to ask would be a
 * second, weaker copy of the rule. What the browser proves here is that the *presentation*
 * of each refusal is right.
 */

const ADMIN = '/admin';

test.describe('populated', () => {
	test.beforeEach(async ({ page }) => {
		await serveAdminOverview(page, 'overview');
		await page.goto(ADMIN);
	});

	test('renders all five sections, including the two that are not instrumented', async ({
		page
	}) => {
		// Five and no sixth. The two unmeasured ones are *present* on purpose: omitting them
		// would let a three-section page read as a complete picture, and where the
		// instrumentation stopped is itself a finding this experiment is recording.
		for (const title of ['API / Axum', 'Runtime', 'PostgreSQL', 'Analyzer', 'Deployment']) {
			await expect(page.getByRole('heading', { name: title, level: 2 })).toBeVisible();
		}

		await expect(page.getByRole('heading', { level: 1 })).toHaveCount(1);
	});

	test('the unmeasured sections say so rather than showing figures', async ({ page }) => {
		for (const id of ['postgresql', 'analyzer']) {
			const section = page.locator(`section:has(> h2#${id})`);
			await expect(section).toContainText('Not instrumented in Experimental-v1');
			// No table, no cards. An empty grid would look like a load that failed.
			await expect(section.locator('table')).toHaveCount(0);
			await expect(section.locator('dl')).toHaveCount(0);
		}
	});

	test('the route table renders one row per published label and derives none', async ({ page }) => {
		const rows = page.locator('table tbody tr');
		await expect(rows).toHaveCount(5);

		// The matched pattern, not a concrete path. The `{analysis_id}` placeholder is the
		// proof: a URI with an id in it could not produce that string.
		await expect(
			page.getByRole('rowheader', { name: '/api/v1/analyses/{analysis_id}' })
		).toBeVisible();
		// Not a route at all, and it says so rather than being hidden or renamed.
		await expect(page.getByRole('rowheader', { name: '<unmatched>' })).toBeVisible();
	});

	test('an overflow percentile renders as a floor, never as a measurement', async ({ page }) => {
		// The fixture's POST row has a p99 past the last bucket bound, where the histogram
		// knows only that something was slower. `10.00 s` would state a measurement that was
		// never taken; `≥ 10.00 s` states what was observed.
		const row = page.locator('tbody tr', {
			has: page.getByRole('rowheader', { name: '/api/v1/analyses', exact: true })
		});
		await expect(row).toContainText('≥ 10.00 s');
	});

	test('the page says its figures describe one process', async ({ page }) => {
		// The sentence that stops a reader treating a restart as a traffic collapse, or two
		// instances as one. It is part of the contract's own documentation, not decoration.
		await expect(page.getByText('the single process that answered this request')).toBeVisible();
	});

	test('no accessibility violations', async ({ page }) => {
		const results = await new AxeBuilder({ page })
			.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
			.analyze();
		expect(results.violations).toEqual([]);
	});
});

test('unmeasurable memory reads as unknown, never as zero', async ({ page }) => {
	// The whole reason the second fixture exists. `0 MiB` would be a measurement of a
	// process using no memory, which is certainly untrue; "Not measured" is the honest
	// rendering of a platform with no /proc to read.
	await serveAdminOverview(page, 'overview-memory-unavailable');
	await page.goto(ADMIN);

	const card = page.locator('div:has(> dt:text-is("Resident memory"))');
	await expect(card).toContainText('Not measured');
	await expect(card).not.toContainText('0 MiB');
});

test('a measured and an unmeasured absence are told apart', async ({ page }) => {
	// `NOT_INSTRUMENTED` and `UNAVAILABLE` are different claims, exactly as `MISSING` and
	// `UNABLE_TO_VERIFY` are in the analysis contract. Collapsing them would have the
	// dashboard say "we could not measure CPU" when the truth is that nobody ever tried.
	await serveAdminOverview(page, 'overview-memory-unavailable');
	await page.goto(ADMIN);

	await expect(page.locator('div:has(> dt:text-is("Resident memory"))')).toContainText(
		'Not measured'
	);
	await expect(page.locator('div:has(> dt:text-is("CPU"))')).toContainText('Not instrumented');
});

test.describe('refusals decide affordances, not wording', () => {
	/** A signed-in control by any of its names. There must be exactly one rule about it. */
	const signIn = 'button:text-matches("Sign in", "i")';
	const retry = 'button:text-matches("Try again", "i")';

	test('UNAUTHENTICATED offers a way to sign in', async ({ page }) => {
		await refuseAdminOverview(page, 401, 'UNAUTHENTICATED', 'anything at all');
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Sign in required' })).toBeVisible();
		// The server's sentence is what explains it — asserted as *present*, never matched.
		await expect(page.getByText('anything at all')).toBeVisible();
	});

	test('FORBIDDEN offers no control that would repeat the request', async ({ page }) => {
		// The remedy is neither signing in nor retrying: signing in again cannot change the
		// answer, and repeating the request unchanged fails identically. So the assertion
		// counts the controls rather than naming them.
		//
		// Naming them is what let the first version of this pass while the page shipped a
		// "Refresh" button directly above the refusal — a retry affordance by another name,
		// asserted against by a test looking for "Try again". A control that leads nowhere
		// is the defect; what it happens to be called is not.
		await refuseAdminOverview(page, 403, 'FORBIDDEN', 'a different sentence entirely');
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Not permitted' })).toBeVisible();
		await expect(page.getByRole('main').getByRole('button')).toHaveCount(0);
		await expect(page.getByText('a different sentence entirely')).toBeVisible();
	});

	test('AUTHENTICATION_UNAVAILABLE offers a retry and never a sign-out', async ({ page }) => {
		// Ours, not the caller's. Signing somebody out over a dependency that was briefly
		// unreachable would take away a session that is perfectly valid.
		await refuseAdminOverview(page, 503, 'AUTHENTICATION_UNAVAILABLE', 'a third sentence');
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Sign-in cannot be checked' })).toBeVisible();
		// Exactly one control, and it retries. Counted as well as named, so a second way to
		// re-send the request cannot appear beside it unnoticed.
		await expect(page.getByRole('main').getByRole('button')).toHaveCount(1);
		await expect(page.locator(retry)).toHaveCount(1);
		await expect(page.locator('button:text-matches("Sign out", "i")')).toHaveCount(0);
	});

	test('a code this build has never seen invents no remedy', async ({ page }) => {
		// The presentation-side equivalent of failing closed. A future backend can add a
		// code months after this bundle was cached, and the honest response is to show what
		// the server said and offer nothing — never to guess at an action.
		await refuseAdminOverview(
			page,
			418,
			'VARIANT_FROM_A_LATER_RELEASE' as never,
			'something this build cannot interpret'
		);
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Request refused' })).toBeVisible();
		await expect(page.getByText('something this build cannot interpret')).toBeVisible();
		await expect(page.locator(signIn)).toHaveCount(0);
		await expect(page.locator(retry)).toHaveCount(0);
	});
});
