<script lang="ts">
	/*
	 * Who is signed in, and what can be done about it.
	 *
	 * Presentational only. It performs no transport, imports no Firebase, and never reads
	 * the session singleton — the route passes the state in and gets intent back through two
	 * callbacks, so all four states are renderable from a literal in a test. That is the same
	 * rule the rest of `components/` follows, and here it is worth more than usual: the
	 * states that matter most are the ones a hand-clicked check never reaches.
	 *
	 * ## Four states, and why none of them collapse
	 *
	 * - **`unknown`** is not `signed-out`. Firebase restores a session asynchronously, so a
	 *   gate that treated "not yet known" as "signed out" would flash a sign-in button at
	 *   somebody who is already signed in, on every page load. It renders a quiet placeholder
	 *   and *no control at all* — the `min-block-size` below keeps the resolved state from
	 *   shoving the form down the page when it arrives.
	 *
	 * - **`unavailable`** is not an error. A deployment with no Firebase project is a
	 *   read-only demo: reports stay public, and only *creation* is closed. Saying so plainly
	 *   is the honest reading; an error treatment here would report a working configuration
	 *   as broken.
	 *
	 * - **`signed-out`** gets the reason as well as the button. "Sign in" with no account of
	 *   why is the version of this control that reads as a paywall.
	 *
	 * - **`signed-in`** names the account. Two Google accounts in one browser is the ordinary
	 *   case, and "which one is this?" is unanswerable without it.
	 */
	import type { SessionState } from '$lib/auth/session.svelte';
	import Button from '$lib/components/primitives/Button.svelte';

	interface Props {
		/** The session's current state, straight from `session.state`. */
		state: SessionState;
		/** Set while a sign-in popup is open, so the control locks rather than re-opening it. */
		busy?: boolean;
		/** The last sign-in failure, already in words a reader can act on. */
		error?: string | null;
		onSignIn: () => void;
		onSignOut: () => void;
	}

	let { state, busy = false, error = null, onSignIn, onSignOut }: Props = $props();

	/**
	 * Name, then email, then a neutral phrase — never the uid.
	 *
	 * Google supplies both fields as nullable, and a uid on screen identifies the account to
	 * nobody. The fallback says *that* somebody is signed in without pretending to say who.
	 */
	const who = $derived(
		state.status === 'signed-in' ? (state.user.name ?? state.user.email ?? 'your account') : ''
	);
</script>

<div class="auth-gate">
	{#if state.status === 'unknown'}
		<p class="auth-gate__pending">Checking whether you are signed in…</p>
	{:else if state.status === 'unavailable'}
		<p class="auth-gate__note">
			Sign-in is not available in this deployment, so no new analysis can be started here. Existing
			reports remain publicly viewable at their own addresses.
		</p>
	{:else if state.status === 'signed-out'}
		<p class="auth-gate__note">
			Starting an analysis runs work on our side, so it is limited to a signed-in account. Reading a
			finished report is not.
		</p>
		<Button variant="primary" {busy} onclick={onSignIn}>Sign in with Google</Button>
	{:else}
		<p class="auth-gate__note">Signed in as <strong>{who}</strong>.</p>
		<Button {busy} onclick={onSignOut}>Sign out</Button>
	{/if}

	{#if error}
		<!--
			A sign-in failure is announced rather than merely printed: the popup that produced
			it has closed, so nothing else on screen has changed and a silent line of text
			below the fold is indistinguishable from nothing happening.
		-->
		<p class="auth-gate__error" role="alert">{error}</p>
	{/if}
</div>

<style>
	.auth-gate {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-3);
		/*
		 * A floor, not a promise. It reserves roughly what the resolved states occupy so the
		 * form below does not jump when Firebase answers. Long copy still grows the box —
		 * the alternative is a fixed height that clips at 360px, which is worse.
		 */
		min-block-size: var(--space-16);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.auth-gate__pending,
	.auth-gate__note {
		margin: 0;
		color: var(--text-secondary);
	}

	.auth-gate__error {
		margin: 0;
		color: var(--text-primary);
		font-size: var(--font-size-sm);
	}
</style>
