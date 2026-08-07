import { QUEUED_FIXTURE, type ApiError } from '@repolens/api-client';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { session, type SessionState } from '$lib/auth/session.svelte';
import HomeRoute from '../routes/+page.svelte';
import '$lib/styles/global.css';

/**
 * The submit form, in a real browser.
 *
 * Four session states reach this route and only one of them can start an analysis. Three
 * are exactly the states a hand-clicked check never sees — a session that has not resolved
 * yet, a deployment with no Firebase project at all, and a refusal from the API — so they
 * are driven here from the session singleton and from the wire rather than from a
 * configuration somebody has to reproduce.
 *
 * Both boundaries are asserted on the wire, not on the markup. "There is no enabled button"
 * and "no request was sent" are different claims, and creation is the one request in this
 * app that starts paid work: the failure mode worth testing is the one where it *succeeds*
 * when it should not have been possible.
 */

const navigation = vi.hoisted(() => ({
	/*
	 * `goto` is the only thing this route needs from `$app/navigation`, and the real one
	 * throws outside an initialised router. Mocking it also makes the assertion sharper than
	 * observing a URL change: the test can say *which* address was navigated to, and prove
	 * the failure paths navigate nowhere at all.
	 */
	goto: vi.fn(() => Promise.resolve())
}));

vi.mock('$app/navigation', () => navigation);

const net = vi.hoisted(() => {
	/*
	 * `openapi-fetch` captures `globalThis.fetch` when the client is constructed, and
	 * `$lib/api/client` constructs it at import time — so a stub installed in a test would
	 * arrive after the capture and never be called. Hoisting installs one stable indirection
	 * instead: its identity never changes, so the capture stays valid while each test
	 * redirects where it dispatches.
	 */
	const real = globalThis.fetch.bind(globalThis);
	const state: { dispatch: typeof globalThis.fetch } = { dispatch: real };
	globalThis.fetch = (input, init) => state.dispatch(input, init);
	return { state, real };
});

/** The session is a tab-wide singleton, so every test has to put it back. */
const INITIAL_STATE: SessionState = session.state;

/*
 * Severs the route's mount effect from the real Firebase SDK.
 *
 * `+page.svelte` calls `session.initialize()` on mount. That is a no-op only when
 * `signInConfigured` is false — and it is *true* on any machine with a populated root
 * `.env.local`, where it loads the SDK and registers `onAuthStateChanged`. That listener
 * then resolves with no user and writes `{ status: 'signed-out' }` over whatever state a
 * test had just set, disabling the submit button mid-assertion.
 *
 * It lands in whichever test is running when the dynamic import and IndexedDB read finish,
 * so it presents as an intermittent failure in a different test each time. CI never sees it,
 * because the Firebase variables are empty there and `initialize()` returns immediately —
 * which is the worst shape for a flake: reproducible only on a correctly configured
 * developer machine.
 *
 * `idToken` is stubbed for the same reason and it is not optional: it reaches the same
 * lazily-built auth handle, and that handle is cached on the singleton — so a single
 * unstubbed call anywhere in this file registers the listener for every test after it.
 * Stubbing only `initialize` fixed the symptom in one test and left it in another.
 *
 * Stubbed here rather than changed in `session.svelte.ts`: overwriting state when the auth
 * callback fires is exactly right in production. The defect is that a unit test depended on
 * ambient configuration.
 */
beforeEach(() => {
	vi.spyOn(session, 'initialize').mockResolvedValue();
	// Overridden by the tests that assert on a specific token.
	vi.spyOn(session, 'idToken').mockResolvedValue(null);
});

afterEach(() => {
	net.state.dispatch = net.real;
	session.state = INITIAL_STATE;
	session.busy = false;
	session.error = null;
	navigation.goto.mockClear();
	vi.restoreAllMocks();
});

const REPOSITORY_URL = 'https://github.com/rust-lang/crates.io';

const SIGNED_IN: SessionState = {
	status: 'signed-in',
	user: { uid: 'uid-1', name: 'Ada Lovelace', email: 'ada@example.com' }
};

/**
 * A `401` body, built from the contract's own types rather than copied from a fixture.
 *
 * There is no fixture to use: `contracts/fixtures/analysis-v1/*` describes *analyses*, and
 * the authentication refusals are produced by the HTTP layer and never stored on one. What
 * keeps this from being an invented shape is the annotation — `ApiError` and its `code` are
 * generated from `contracts/openapi.json`, so a renamed field or a dropped variant is a
 * compile error here. The sentence is the one the server actually sends
 * (`crates/repolens-server/src/api/authenticated.rs`), quoted so the test reads like the
 * thing it stands in for; the assertion below compares against this constant, so a reworded
 * server message cannot make it falsely green.
 */
const UNAUTHENTICATED: ApiError = {
	code: 'UNAUTHENTICATED',
	message: 'Starting an analysis requires signing in. Sign in and try again.',
	retry_after_seconds: null
};

/** Every request the page issues, answered with one fixed response. */
function answerWith(status: number, body: unknown): Request[] {
	const issued: Request[] = [];

	net.state.dispatch = (input, init) => {
		issued.push(
			input instanceof Request && init === undefined ? input.clone() : new Request(input, init)
		);
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
	};

	return issued;
}

/** Record requests without answering any of them, for the paths that must send none. */
function refuseEverything(): Request[] {
	const issued: Request[] = [];

	net.state.dispatch = (input, init) => {
		issued.push(input instanceof Request && init === undefined ? input : new Request(input, init));
		return Promise.reject(new Error('no request was expected from this state'));
	};

	return issued;
}

/** The buttons on screen, by their visible label. */
function buttonLabels(container: Element): string[] {
	return [...container.querySelectorAll('button')].map((button) =>
		(button.textContent ?? '').replace(/\s+/g, ' ').trim()
	);
}

/** The page's prose, with markup indentation collapsed to what a reader actually sees. */
function prose(container: Element): string {
	return (container.textContent ?? '').replace(/\s+/g, ' ').trim();
}

test('an unresolved session offers no sign-in button, and does not claim to be signed out', async () => {
	/*
	 * The flash this state exists to prevent. Firebase restores a session asynchronously, so
	 * treating "not yet known" as "signed out" puts a sign-in button in front of somebody who
	 * is already signed in — on every page load, for as long as the SDK takes to answer.
	 */
	session.state = { status: 'unknown' };

	const screen = await render(HomeRoute);

	expect(buttonLabels(screen.container)).toEqual(['Start analysis']);
	const text = prose(screen.container);
	expect(text).toContain('Checking whether you are signed in');
	// Neither of the two settled states may be asserted while the answer is still outstanding.
	expect(text).not.toContain('Sign in with Google');
	expect(text).not.toContain('not available in this deployment');
});

test('a deployment without sign-in says so as a read-only state, and offers no sign-in', async () => {
	// Not an error: reports stay public and only creation is closed. An error treatment here
	// would report a working configuration as broken.
	session.state = { status: 'unavailable' };

	const screen = await render(HomeRoute);

	const text = prose(screen.container);
	expect(text).toContain('Sign-in is not available in this deployment');
	expect(text).toContain('reports remain publicly viewable');
	// The affordance is withheld because it cannot work, not hidden behind a button that fails.
	expect(buttonLabels(screen.container)).toEqual(['Start analysis']);
});

test('signed out, the sign-in affordance is offered and no analysis can be started', async () => {
	session.state = { status: 'signed-out' };
	const issued = refuseEverything();

	const screen = await render(HomeRoute);

	expect(buttonLabels(screen.container)).toEqual(['Sign in with Google', 'Start analysis']);

	const submit = screen.container.querySelector('button[type="submit"]') as HTMLButtonElement;
	expect(submit.disabled).toBe(true);

	// On the wire, not on the markup: "there is no enabled button" and "no request was sent"
	// are different claims, and only the second one is the boundary that matters.
	await screen.getByLabelText('Public GitHub repository URL').fill(REPOSITORY_URL);
	submit.click();
	await new Promise((resolve) => setTimeout(resolve, 0));

	expect(issued).toEqual([]);
	expect(navigation.goto).not.toHaveBeenCalled();
});

test('signed in, a submission posts to the contract path and navigates to the new analysis', async () => {
	session.state = SIGNED_IN;
	const issued = answerWith(202, QUEUED_FIXTURE.analysis);

	const screen = await render(HomeRoute);

	// Who is signed in, so two Google accounts in one browser can be told apart.
	expect(prose(screen.container)).toContain('Ada Lovelace');

	await screen.getByLabelText('Public GitHub repository URL').fill(REPOSITORY_URL);
	await screen.getByRole('button', { name: 'Start analysis' }).click();

	await expect.poll(() => navigation.goto.mock.calls.length).toBe(1);

	const request = issued[0];
	expect(request).toBeDefined();
	expect(request?.method).toBe('POST');
	expect(new URL(request?.url ?? '').pathname).toBe('/api/v1/analyses');
	expect(await request?.json()).toEqual({ repository_url: REPOSITORY_URL });

	// The progress page, which is readable anonymously and is the URL worth sharing.
	expect(navigation.goto).toHaveBeenCalledWith(`/analyses/${QUEUED_FIXTURE.analysis.id}`);
});

test('the ID token reaches the wire as a bearer credential', async () => {
	// The whole point of the gate. A create that dropped the token would still be a valid
	// request and would still be refused — by the API, with a message the user cannot act on.
	session.state = SIGNED_IN;
	vi.spyOn(session, 'idToken').mockResolvedValue('id-token-value');
	const issued = answerWith(202, QUEUED_FIXTURE.analysis);

	const screen = await render(HomeRoute);
	await screen.getByLabelText('Public GitHub repository URL').fill(REPOSITORY_URL);
	await screen.getByRole('button', { name: 'Start analysis' }).click();

	await expect.poll(() => issued.length).toBe(1);
	expect(issued[0]?.headers.get('authorization')).toBe('Bearer id-token-value');
});

test('a refused creation renders the reason and navigates nowhere', async () => {
	/*
	 * `401 UNAUTHENTICATED` is the refusal the gate is built around, and the browser is not
	 * the authority on it: this state is reachable with a session the browser believes in —
	 * an expired token, a revoked account, a deployment that verifies against a different
	 * project. So the API's answer is what the reader is shown, and the navigation must not
	 * happen.
	 */
	session.state = SIGNED_IN;
	answerWith(401, UNAUTHENTICATED);

	const screen = await render(HomeRoute);
	await screen.getByLabelText('Public GitHub repository URL').fill(REPOSITORY_URL);
	await screen.getByRole('button', { name: 'Start analysis' }).click();

	await expect.element(screen.getByRole('alert')).toBeInTheDocument();

	const text = prose(screen.container);
	// The server's own sentence, verbatim — it is the only one that knows what happened.
	expect(text).toContain(UNAUTHENTICATED.message);
	// And the code, with the label the contract package owns.
	expect(text).toContain('Not signed in');
	expect(text).toContain('UNAUTHENTICATED');
	expect(navigation.goto).not.toHaveBeenCalled();
});

test('a refusal is announced, not merely printed below the fold', async () => {
	session.state = SIGNED_IN;
	answerWith(401, UNAUTHENTICATED);

	const screen = await render(HomeRoute);
	await screen.getByLabelText('Public GitHub repository URL').fill(REPOSITORY_URL);
	await screen.getByRole('button', { name: 'Start analysis' }).click();

	await expect.poll(() => screen.container.querySelector('[role="alert"]')).not.toBeNull();

	// `alert`, not `status`: the submission the reader just made did not happen, and nothing
	// else on screen changed to tell them so.
	const alert = screen.container.querySelector('[role="alert"]');
	expect(alert?.getAttribute('role')).toBe('alert');
	expect(prose(alert as Element)).toContain('could not be started');
});

test('a request that never reaches a server is not reported as a missing repository', async () => {
	// The misdiagnosis the three-outcome union exists to prevent: a CORS or CSP failure
	// rendered to the reader as "not found".
	session.state = SIGNED_IN;
	net.state.dispatch = () => Promise.reject(new TypeError('Failed to fetch'));

	const screen = await render(HomeRoute);
	await screen.getByLabelText('Public GitHub repository URL').fill(REPOSITORY_URL);
	await screen.getByRole('button', { name: 'Start analysis' }).click();

	await expect.poll(() => screen.container.querySelector('[role="alert"]')).not.toBeNull();

	const text = prose(screen.container);
	expect(text).toContain('The API could not be reached');
	expect(text).toContain('transport or configuration failure');
	expect(text).not.toContain('not found');
	expect(navigation.goto).not.toHaveBeenCalled();
});

test('the URL field has a real label, and the submit control is a real submit button', async () => {
	session.state = SIGNED_IN;

	const screen = await render(HomeRoute);

	const input = screen.container.querySelector('#repository-url') as HTMLInputElement;
	const label = screen.container.querySelector('label[for="repository-url"]');
	// A placeholder is not a label, and neither is an aria-label nobody can see.
	expect(label?.textContent).toContain('Public GitHub repository URL');
	expect(input.getAttribute('aria-describedby')).toBe('repository-url-hint');
	expect(screen.container.querySelector('#repository-url-hint')).not.toBeNull();

	// `type="submit"`, so Enter in the field submits the form the same way the button does.
	expect(screen.container.querySelector('button[type="submit"]')?.textContent?.trim()).toBe(
		'Start analysis'
	);
});
