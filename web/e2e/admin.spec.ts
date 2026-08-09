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
 * The page sends its request as soon as the session settles, signed in or not. That is the
 * designed behaviour rather than a test convenience: Axum is the authorisation boundary, and
 * a page that refused to ask would be a second, weaker copy of the rule. What the browser
 * proves here is that the *presentation* of each refusal is right.
 *
 * ## One assertion deliberately lives elsewhere
 *
 * `PUBLIC_FIREBASE_*` is read from a git-ignored `.env.local` at the repository root, so a
 * developer machine and CI build **different** applications: one where sign-in is offered
 * and one where it is not. Both are supported configurations. Anything that depends on
 * which is asserted in `src/tests/admin-refusal.svelte.test.ts`, where the component is
 * rendered with that flag set explicitly, rather than here where it is ambient.
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

	test('the provenance claim covers only the sections it holds for', async ({ page }) => {
		// The sentence that stops a reader treating a restart as a traffic collapse, or two
		// instances as one — and it is scoped to the two sections it is true of.
		//
		// It used to say "every figure on this page", which the composition made false: the
		// database and schema facts come from a separate system-probe request, and nothing
		// routes two requests to the same instance. Each endpoint was truthful; the sentence
		// over both was not.
		await expect(
			page.getByText('the single process that answered the operational snapshot')
		).toBeVisible();

		// And the section that mixes in the second request says so, where a reader meets it.
		const deployment = page.locator('section:has(> h2#deployment)');
		await expect(deployment).toContainText('separate system-probe request');
	});

	test('no accessibility violations', async ({ page }) => {
		// Wait for the loaded state before analysing. Without this the run is a race: under
		// parallel load axe can reach a page whose fetch has not resolved, and it then audits
		// the "Reading…" placeholder — passing in isolation and failing in a full run, which
		// is the least useful shape a test can have.
		//
		// The document title is the tell. SvelteKit applies `<svelte:head>` after hydration,
		// so an early audit reports a missing `<title>` — a violation of the harness's timing
		// rather than of the page.
		await expect(page.getByRole('heading', { name: 'Deployment', level: 2 })).toBeVisible();
		await expect(page).toHaveTitle(/Operations/);

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
	const retry = 'button:text-matches("Try again", "i")';

	/**
	 * Every control the page offers, counted rather than named.
	 *
	 * Naming is what let three assertions in this file be true and about the wrong thing: a
	 * `FORBIDDEN` test excluding "Try again" while a "Refresh" button sat above the refusal,
	 * a `401` test called "offers a way to sign in" that asserted no control at all, and an
	 * unknown-code test claiming "invents no remedy" that excluded exactly two known labels.
	 * A count cannot be satisfied by a control under another name.
	 */
	const buttons = (page: import('@playwright/test').Page) =>
		page.getByRole('main').getByRole('button');

	test('UNAUTHENTICATED reaches the sign-in state and never offers a retry', async ({ page }) => {
		// **What this state offers depends on the build**, and that is why the control count
		// is asserted elsewhere. `PUBLIC_FIREBASE_*` comes from a git-ignored `.env.local` at
		// the repository root: a developer machine that has one produces a sign-in button
		// here, and CI, which has none, correctly produces the "not configured" explanation
		// instead. Both are right; a count asserted here would pass in one place and fail in
		// the other.
		//
		// So the count is owned by `src/tests/admin-refusal.svelte.test.ts`, which renders
		// the component with `canSignIn` set both ways and asserts exactly one control in
		// the configured branch and none in the unconfigured one. What is asserted *here* is
		// the part that holds in every build: the page reaches the sign-in state, and it
		// never offers a way to repeat a request that will be refused identically.
		//
		// Discovering this was itself the third instance of the property the rest of this
		// file is about — the environment, not the page, was the variable nobody had named.
		await refuseAdminOverview(page, 401, 'UNAUTHENTICATED', 'anything at all');
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Sign in required' })).toBeVisible();
		await expect(page.locator(retry)).toHaveCount(0);
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
		await expect(buttons(page)).toHaveCount(0);
		await expect(page.getByText('a different sentence entirely')).toBeVisible();
	});

	test('AUTHENTICATION_UNAVAILABLE offers a retry and never a sign-out', async ({ page }) => {
		// Ours, not the caller's. Signing somebody out over a dependency that was briefly
		// unreachable would take away a session that is perfectly valid.
		await refuseAdminOverview(page, 503, 'AUTHENTICATION_UNAVAILABLE', 'a third sentence');
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Sign-in cannot be checked' })).toBeVisible();
		// Exactly one control, and it retries. Counted as well as named, so a second way to
		// re-send the request cannot appear beside it unnoticed — and this is also what
		// proves the zero-count assertions above are not vacuous.
		await expect(buttons(page)).toHaveCount(1);
		await expect(page.locator(retry)).toHaveCount(1);
		await expect(page.locator('button:text-matches("Sign out", "i")')).toHaveCount(0);
	});

	test('a code this build has never seen invents no remedy', async ({ page }) => {
		// The presentation-side equivalent of failing closed. A future backend can add a
		// code months after this bundle was cached, and the honest response is to show what
		// the server said and offer nothing — never to guess at an action.
		//
		// Counted, not excluded by name. The previous version asserted that no button was
		// called "Sign in" or "Try again", which would have passed with a third control
		// under any other label — the very defect this test claims to rule out.
		await refuseAdminOverview(
			page,
			418,
			'VARIANT_FROM_A_LATER_RELEASE' as never,
			'something this build cannot interpret'
		);
		await page.goto(ADMIN);

		await expect(page.getByRole('heading', { name: 'Request refused' })).toBeVisible();
		await expect(page.getByText('something this build cannot interpret')).toBeVisible();
		await expect(buttons(page)).toHaveCount(0);
	});
});
