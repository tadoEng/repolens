/**
 * Google sign-in, and the ID token that unlocks analysis creation.
 *
 * The whole of the frontend's authentication responsibility is: prove who the user is to
 * Firebase, and hand the resulting ID token to the RepoLens API. It decides nothing.
 * `POST /api/v1/analyses` verifies that token server-side and refuses without it, so a
 * signed-in state faked in the browser buys exactly nothing — which is the property that
 * lets this module stay small.
 *
 * ## Why the config can be absent
 *
 * A deployment without a Firebase project is a **read-only demo**: reports stay public and
 * the submit control says sign-in is unavailable. That is an honest configuration, not a
 * broken one, so the values are read as possibly-empty rather than required. `svelte.config.js`
 * defaults them to the empty string for the same reason.
 *
 * ## The token is never stored
 *
 * It is fetched immediately before a request and dropped afterwards. There is no
 * `localStorage`, no cookie, and no module-level copy: a token in storage is a token an
 * XSS can exfiltrate long after the tab that earned it is gone, and Firebase already holds
 * the refresh credential in its own IndexedDB store under its own origin rules.
 */

import {
	PUBLIC_FIREBASE_API_KEY,
	PUBLIC_FIREBASE_APP_ID,
	PUBLIC_FIREBASE_AUTH_DOMAIN,
	PUBLIC_FIREBASE_PROJECT_ID
} from '$env/static/public';

/** Whether this build was given enough configuration to sign anybody in. */
export const signInConfigured = Boolean(PUBLIC_FIREBASE_API_KEY && PUBLIC_FIREBASE_AUTH_DOMAIN);

/** What the UI needs to know about the current user. */
export interface SessionUser {
	/** Firebase uid. The API derives identity from the token, never from this. */
	readonly uid: string;
	/** Display name, when Google supplied one. */
	readonly name: string | null;
	/** Email, when Google supplied one. Shown so a user can tell which account is active. */
	readonly email: string | null;
}

/**
 * Reactive session state.
 *
 * `unknown` is a distinct state from `signed-out`, and the distinction is load-bearing:
 * Firebase restores a session asynchronously, so a UI that treated "not yet known" as
 * "signed out" would flash a sign-in button at somebody who is already signed in, and
 * would do it on every page load.
 */
export type SessionState =
	| { readonly status: 'unknown' }
	| { readonly status: 'unavailable' }
	| { readonly status: 'signed-out' }
	| { readonly status: 'signed-in'; readonly user: SessionUser };

class Session {
	/** Current state. Read this from components; it is a rune. */
	state = $state<SessionState>(
		signInConfigured ? { status: 'unknown' } : { status: 'unavailable' }
	);

	/** Set while a sign-in popup is open, so the button can disable itself. */
	busy = $state(false);

	/** The last sign-in failure, in words a user can act on. */
	error = $state<string | null>(null);

	/**
	 * Firebase's auth handle, created on first use.
	 *
	 * Imported dynamically so the SDK is not in the initial bundle. Every route except
	 * the submit form works without it, and two of the three are public report views that
	 * should not pay for an auth library to render.
	 */
	#auth: Promise<import('firebase/auth').Auth> | null = null;

	async #firebaseAuth(): Promise<import('firebase/auth').Auth> {
		this.#auth ??= (async () => {
			const { initializeApp, getApps } = await import('firebase/app');
			const { getAuth, onAuthStateChanged } = await import('firebase/auth');

			const app =
				getApps()[0] ??
				initializeApp({
					apiKey: PUBLIC_FIREBASE_API_KEY,
					authDomain: PUBLIC_FIREBASE_AUTH_DOMAIN,
					projectId: PUBLIC_FIREBASE_PROJECT_ID,
					appId: PUBLIC_FIREBASE_APP_ID
				});

			const auth = getAuth(app);
			onAuthStateChanged(auth, (user) => {
				this.state = user
					? {
							status: 'signed-in',
							user: { uid: user.uid, name: user.displayName, email: user.email }
						}
					: { status: 'signed-out' };
			});
			return auth;
		})();

		return this.#auth;
	}

	/** Restores an existing session, if there is one. Safe to call more than once. */
	async initialize(): Promise<void> {
		if (!signInConfigured) return;
		try {
			await this.#firebaseAuth();
		} catch {
			// A failure here means the SDK could not load or the config is wrong. Neither
			// is something the reader can fix, and neither should break the page — the
			// reports on it are public.
			this.state = { status: 'unavailable' };
		}
	}

	/** Opens the Google sign-in popup. */
	async signIn(): Promise<void> {
		if (!signInConfigured || this.busy) return;

		this.busy = true;
		this.error = null;
		try {
			const auth = await this.#firebaseAuth();
			const { GoogleAuthProvider, signInWithPopup } = await import('firebase/auth');
			await signInWithPopup(auth, new GoogleAuthProvider());
		} catch (cause) {
			this.error = describeSignInFailure(cause);
		} finally {
			this.busy = false;
		}
	}

	/** Ends the session. */
	async signOut(): Promise<void> {
		if (!signInConfigured) return;
		try {
			const auth = await this.#firebaseAuth();
			const { signOut } = await import('firebase/auth');
			await signOut(auth);
		} catch {
			this.error = 'Could not sign out. Reload the page and try again.';
		}
	}

	/**
	 * A fresh ID token, or `null` when nobody is signed in.
	 *
	 * Fetched per request rather than cached here. Firebase refreshes it when it is close
	 * to expiry, so asking each time is what keeps a long-lived tab from posting a token
	 * the API will reject as expired.
	 */
	async idToken(): Promise<string | null> {
		if (!signInConfigured) return null;
		try {
			const auth = await this.#firebaseAuth();
			return (await auth.currentUser?.getIdToken()) ?? null;
		} catch {
			return null;
		}
	}
}

/**
 * Turns a Firebase error into something worth showing.
 *
 * Cancelling a popup is not a failure and must not be reported as one — it is the single
 * most common outcome after clicking sign-in and changing your mind.
 */
function describeSignInFailure(cause: unknown): string | null {
	const code =
		typeof cause === 'object' && cause !== null && 'code' in cause
			? String((cause as { code: unknown }).code)
			: '';

	switch (code) {
		case 'auth/popup-closed-by-user':
		case 'auth/cancelled-popup-request':
		case 'auth/user-cancelled':
			return null;
		case 'auth/popup-blocked':
			return 'The sign-in window was blocked by the browser. Allow popups for this site and try again.';
		case 'auth/network-request-failed':
			return 'Sign-in could not reach Google. Check your connection and try again.';
		case 'auth/unauthorized-domain':
			// The deployment mistake this message exists for: forgetting to add the
			// Cloudflare domain to Firebase's authorised list. It fails only in
			// production, so it says exactly what to do.
			return 'This site is not an authorised sign-in domain for the configured Firebase project.';
		default:
			return 'Sign-in did not complete. Try again.';
	}
}

/** The single session for this tab. */
export const session = new Session();
