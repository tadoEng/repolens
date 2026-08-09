import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';

import AdminRefusal, { type Remedy } from '$lib/components/admin/AdminRefusal.svelte';
import '$lib/styles/global.css';

/**
 * What each refusal state offers the reader.
 *
 * ## The property under test is a count, not a name
 *
 * The requirement is *does this state offer an action, and how many* — not *is there a
 * button called "Try again"*. Those are different questions, and the end-to-end suite
 * originally asked the second: it asserted no button named "Try again" existed on a
 * `FORBIDDEN` page while the page shipped a "Refresh" button directly above the refusal.
 * The assertion was true, specific, and about the wrong thing.
 *
 * Reviewing the fix then found two more of exactly the same shape — a `401` test named
 * "offers a way to sign in" that never asserted a sign-in control, and an unknown-code test
 * claiming "invents no remedy" that only excluded two known button labels. Counting is what
 * closes all three, because a count cannot be satisfied by a control under another name.
 *
 * ## Why this is here and not in `e2e/admin.spec.ts`
 *
 * There is no Firebase configuration in CI, so `canSignIn` is false in every end-to-end
 * capture and the sign-in control is *correctly* absent from all of them. A suite that
 * could only ever observe that branch would have to assert something adjacent to the
 * requirement — the failure this file exists to stop repeating. Rendering the component
 * directly is what makes both branches reachable.
 */

/** Rendered controls, which is the property every case below is about. */
function controls(screen: Awaited<ReturnType<typeof render>>): HTMLButtonElement[] {
	return [...screen.container.querySelectorAll('button')];
}

async function refusal(remedy: Remedy, canSignIn = false) {
	return render(AdminRefusal, {
		props: {
			remedy,
			title: 'A title this component was given',
			// Deliberately not a real server sentence. Nothing here may depend on the prose;
			// the backend owns it and rewording it must not move a test.
			message: 'an explanation that came from the server',
			canSignIn
		}
	});
}

test('the server’s explanation is always what is shown', async () => {
	// Every remedy renders it. The division of authority is that the backend says *why* and
	// this component says *what follows* — so the sentence must survive all four branches.
	for (const remedy of ['sign-in', 'not-permitted', 'try-again', 'refused'] as const) {
		const screen = await refusal(remedy);
		expect(screen.container.textContent, remedy).toContain(
			'an explanation that came from the server'
		);
	}
});

test('UNAUTHENTICATED offers exactly one control, and it signs in', async () => {
	// The affordance asserted directly, in the configuration where it can exist at all.
	const screen = await refusal('sign-in', true);
	const buttons = controls(screen);

	expect(buttons).toHaveLength(1);
	expect(buttons[0]?.textContent?.trim()).toBe('Sign in');
});

test('UNAUTHENTICATED without sign-in configured offers none, and says why', async () => {
	// A read-only deployment is a supported configuration, not a broken one. The honest
	// state is no control plus the reason, never a button that opens a popup against a
	// project this build was never given.
	const screen = await refusal('sign-in', false);

	expect(controls(screen)).toHaveLength(0);
	expect(screen.container.textContent).toContain('Sign-in is not configured in this build');
});

test('FORBIDDEN offers no control at all', async () => {
	// Signing in again cannot change the answer and repeating the request fails identically,
	// so any control here would lead somewhere it cannot go. Asserted as a count, in both
	// sign-in configurations, because "signed in and not permitted" is precisely the state
	// where a sign-in control is most tempting to offer.
	for (const canSignIn of [false, true]) {
		const screen = await refusal('not-permitted', canSignIn);
		expect(controls(screen), `canSignIn=${canSignIn}`).toHaveLength(0);
	}
});

test('AUTHENTICATION_UNAVAILABLE offers exactly one control, and it retries', async () => {
	// Ours rather than the caller's, so the remedy is to ask again. Never a sign-out, which
	// would take a valid session away over a dependency that was briefly unreachable.
	const screen = await refusal('try-again', true);
	const buttons = controls(screen);

	expect(buttons).toHaveLength(1);
	expect(buttons[0]?.textContent?.trim()).toBe('Try again');
	expect(screen.container.textContent).not.toContain('Sign out');
});

test('a code this build has never seen invents no control', async () => {
	// Failing closed, on the presentation side. A future backend can add a code months after
	// this bundle was cached; showing what the server said and offering nothing is the only
	// honest response, and the count is what proves nothing was invented — the previous
	// version excluded two known labels and would have passed with a third.
	for (const canSignIn of [false, true]) {
		const screen = await refusal('refused', canSignIn);
		expect(controls(screen), `canSignIn=${canSignIn}`).toHaveLength(0);
	}
});

test('a busy sign-in disables its own control rather than removing it', async () => {
	// Removing it would reflow the page under a reader mid-click, and would make a second
	// click land on whatever moved into that position.
	const screen = await render(AdminRefusal, {
		props: {
			remedy: 'sign-in',
			title: 'Sign in required',
			message: 'an explanation that came from the server',
			canSignIn: true,
			busy: true
		}
	});

	const buttons = controls(screen);
	expect(buttons).toHaveLength(1);
	expect(buttons[0]?.disabled).toBe(true);
});
