import { afterEach, expect, test, vi } from 'vitest';

import { describeSignInFailure } from '$lib/auth/session.svelte';

/**
 * What a failed sign-in tells the person looking at it.
 *
 * This mapping is the only diagnosis anybody gets. The popup runs in another browsing
 * context, the SDK swallows the HTTP exchange, and the deployment has no server log to
 * read — so a code that falls through to the generic sentence is a code that costs an hour.
 * One already did: Firebase Authentication had never been enabled on the project, Identity
 * Toolkit answered `CONFIGURATION_NOT_FOUND`, and the browser said "Sign-in did not
 * complete. Try again."
 *
 * ## Why this calls the function rather than clicking the button
 *
 * `signIn` reaches this code only when `signInConfigured` is true, and that is decided at
 * build time from `PUBLIC_FIREBASE_*`. CI sets `PUBLIC_API_ORIGIN` and nothing else
 * (`.github/workflows/ci.yml`), and the real values live in a git-ignored root `.env.local`
 * — so a test driven through the button would assert against an early return in CI and
 * against a live popup on a configured machine. Neither is the thing worth checking. The
 * function is exported for that reason and is called directly here.
 */

afterEach(() => {
	vi.restoreAllMocks();
});

/** Capture `console.warn` without letting the diagnostic land in the test output. */
function captureWarnings() {
	return vi.spyOn(console, 'warn').mockImplementation(() => {});
}

const CONFIGURATION_NOT_FOUND = { code: 'auth/configuration-not-found' };
const OPERATION_NOT_ALLOWED = { code: 'auth/operation-not-allowed' };

test('an unconfigured Firebase project is reported as a deployment fault, never as "try again"', () => {
	const message = describeSignInFailure(CONFIGURATION_NOT_FOUND);

	// It has to say something, and it has to be about the project rather than the visitor.
	expect(message).toBeTruthy();
	expect(message).toContain('Firebase project');

	/*
	 * The load-bearing assertion. Authentication was never enabled on the project, so no
	 * retry, no other browser and no other Google account can succeed — telling the reader
	 * to try again is not merely unhelpful, it points every one of them away from the only
	 * person who can fix it.
	 */
	expect(message).not.toMatch(/try again/i);
});

test('a disabled Google provider gets its own message, distinct from an unconfigured project', () => {
	const message = describeSignInFailure(OPERATION_NOT_ALLOWED);

	expect(message).toBeTruthy();
	expect(message).not.toMatch(/try again/i);

	// Two different mistakes, fixed on two different console pages. Collapsing them into one
	// sentence would put the operator on the wrong page, which is what the generic message
	// already did.
	expect(message).not.toBe(describeSignInFailure(CONFIGURATION_NOT_FOUND));
});

test('cancelling the popup is not a failure and is reported as nothing at all', () => {
	// The single most common outcome after clicking sign-in and changing your mind. An error
	// banner here would report the user's own decision back to them as a fault.
	expect(describeSignInFailure({ code: 'auth/popup-closed-by-user' })).toBeNull();
	expect(describeSignInFailure({ code: 'auth/cancelled-popup-request' })).toBeNull();
	expect(describeSignInFailure({ code: 'auth/user-cancelled' })).toBeNull();
});

test('the three genuine-failure codes keep their own messages', () => {
	// Not new behaviour — pinned because the two additions above sit in the same switch, and
	// a mis-edited case label is silent until somebody's popup is blocked in production.
	const blocked = describeSignInFailure({ code: 'auth/popup-blocked' });
	const offline = describeSignInFailure({ code: 'auth/network-request-failed' });
	const domain = describeSignInFailure({ code: 'auth/unauthorized-domain' });

	expect(blocked).toContain('blocked by the browser');
	expect(offline).toContain('could not reach Google');
	expect(domain).toContain('not an authorised sign-in domain');

	expect(new Set([blocked, offline, domain]).size).toBe(3);
});

test('an unmapped code still reads generically, and puts the code in the console', () => {
	const warn = captureWarnings();

	const message = describeSignInFailure({ code: 'auth/something-new' });

	expect(message).toBe('Sign-in did not complete. Try again.');

	// The whole point of the change: the next code nobody anticipated is diagnosable from a
	// deployed browser, without a rebuild to find out what it was.
	expect(warn).toHaveBeenCalledTimes(1);
	const logged = String(warn.mock.calls[0]?.[0]);
	expect(logged).toContain('auth/something-new');
	expect(logged).toContain('[repolens]');
});

test('the console gets the code and nothing else off the error', () => {
	const warn = captureWarnings();

	/*
	 * A Firebase `AuthError` carries `customData`, and on some paths that includes the
	 * Identity Toolkit response — tokens included. `console.warn(error)` would print all of
	 * it into a place users paste into issues and screenshots, so the argument has to stay a
	 * string built from the code.
	 */
	describeSignInFailure({
		code: 'auth/internal-error',
		message: 'INTERNAL ASSERTION FAILED',
		customData: { _tokenResponse: { idToken: 'secret-token-value' } }
	});

	expect(warn).toHaveBeenCalledTimes(1);
	const args = warn.mock.calls[0] ?? [];
	expect(args).toHaveLength(1);
	expect(typeof args[0]).toBe('string');

	const logged = String(args[0]);
	expect(logged).toContain('auth/internal-error');
	expect(logged).not.toContain('secret-token-value');
	expect(logged).not.toContain('INTERNAL ASSERTION FAILED');
});

test('a thrown value that is not an object is described rather than crashing', () => {
	const warn = captureWarnings();

	// Nothing guarantees a rejection is an `Error`, let alone one with a `code`. A property
	// read on a string that happened to be thrown would replace a bad sign-in message with a
	// blank page.
	expect(describeSignInFailure('boom')).toBe('Sign-in did not complete. Try again.');
	expect(describeSignInFailure(null)).toBe('Sign-in did not complete. Try again.');
	expect(describeSignInFailure(undefined)).toBe('Sign-in did not complete. Try again.');

	// And the diagnostic still says something rather than logging an empty tail.
	expect(warn).toHaveBeenCalledTimes(3);
	expect(String(warn.mock.calls[0]?.[0])).toContain('unknown');
});

test('a code that is not shaped like a Firebase code is never echoed', () => {
	const warn = captureWarnings();

	/*
	 * `cause` is `unknown` — whatever the SDK, a dependency, or a hostile page threw. Its
	 * `code` is therefore untrusted data that merely happens to be named like an identifier,
	 * and this console line is pasted into issues and screenshots. Echoing whatever arrived
	 * on that field would make the diagnostic a disclosure channel for anything a caller can
	 * get into a rejection.
	 */
	const secretShaped = 'ghp_0123456789abcdefghijklmnopqrstuvwxyz0123';
	const message = describeSignInFailure({ code: secretShaped });

	expect(message).toBe('Sign-in did not complete. Try again.');
	const logged = String(warn.mock.calls[0]?.[0]);
	expect(logged).not.toContain(secretShaped);
	expect(logged).toContain('unknown');
});

test('an over-long code is rejected rather than logged in part', () => {
	const warn = captureWarnings();

	// A bounded shape, not a bounded length: 300 characters is not a Firebase code however it
	// starts, and logging a prefix would still be logging whatever arrived.
	describeSignInFailure({ code: `auth/${'a'.repeat(300)}` });

	const logged = String(warn.mock.calls[0]?.[0]);
	expect(logged).toContain('unknown');
	expect(logged).not.toContain('aaaaaaaaaa');
});

test('a code that is not a string neither reaches the log nor crashes the handler', () => {
	const warn = captureWarnings();

	/*
	 * `String(cause.code)` would coerce every one of these, and the last would throw — turning
	 * a failed sign-in into a blank page from inside the code whose only job is to explain the
	 * failure. Nothing about a `catch` guarantees `code` is a string.
	 */
	const hostile = {
		code: {
			toString() {
				throw new Error('boom');
			}
		}
	};
	const causes = [{ code: 42 }, { code: null }, { code: ['auth/popup-blocked'] }, hostile];

	for (const cause of causes) {
		expect(describeSignInFailure(cause)).toBe('Sign-in did not complete. Try again.');
	}

	expect(warn).toHaveBeenCalledTimes(causes.length);
	for (const call of warn.mock.calls) {
		expect(String(call[0])).toContain('unknown');
	}
});
