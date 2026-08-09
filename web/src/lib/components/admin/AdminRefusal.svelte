<script lang="ts">
	/*
	 * What a refused operational snapshot offers the reader.
	 *
	 * ## Why this is a component
	 *
	 * The contract is `ErrorCode → remedy → affordance`, and the property that owns it is
	 * *how many controls the state offers*, not what any of them is called. That property is
	 * only assertable if both branches of `canSignIn` can be driven — and in the end-to-end
	 * build they cannot: there is no Firebase configuration in CI, so the sign-in control is
	 * correctly absent from every capture. A suite that could only ever see one branch would
	 * have to either skip the assertion or assert something adjacent to it, which is exactly
	 * the failure this file exists to stop repeating.
	 *
	 * Lifted out of the route with its markup unchanged. The rendered output is identical,
	 * which the committed visual baselines are the check on.
	 *
	 * ## The four remedies
	 *
	 *   sign-in        a credential was not presented, so presenting one is the remedy
	 *   not-permitted  a credential *was* presented and is not allow-listed; nothing the
	 *                  reader can do here changes that, so nothing is offered
	 *   try-again      sign-in could not be checked, which is ours rather than theirs —
	 *                  a retry, and never a sign-out, which would take away a valid session
	 *                  over a dependency that was briefly unreachable
	 *   refused        a code this build has never seen: show what the server said and
	 *                  invent no action, which is what failing closed looks like in a UI
	 *
	 * The explanatory sentence is always the server's. The backend owns *why* a request was
	 * refused; this component owns *what follows from it*.
	 */
	import Button from '$lib/components/primitives/Button.svelte';

	export type Remedy = 'sign-in' | 'not-permitted' | 'try-again' | 'refused';

	interface Props {
		/** Which interaction this refusal calls for, derived from the error code. */
		remedy: Remedy;
		/** The heading. Written here, never by the server. */
		title: string;
		/** The server's explanation, or this build's fallback when it sent none. */
		message: string;
		/** Whether a sign-in control could do anything in this build. */
		canSignIn?: boolean;
		/** Whether a sign-in attempt is already in flight. */
		busy?: boolean;
		onSignIn?: () => void;
		onRetry?: () => void;
	}

	let {
		remedy,
		title,
		message,
		canSignIn = false,
		busy = false,
		onSignIn,
		onRetry
	}: Props = $props();
</script>

<section class="refusal" aria-labelledby="refusal-title">
	<h2 id="refusal-title">{title}</h2>
	<p>{message}</p>

	{#if remedy === 'sign-in'}
		{#if canSignIn}
			<Button onclick={() => onSignIn?.()} disabled={busy}>
				{busy ? 'Signing in…' : 'Sign in'}
			</Button>
		{:else}
			<!--
				No control, because there is nothing for it to do. Saying so is the remedy this
				state can offer: a button that opened a popup against a project this build was
				never given would fail in a way the reader could not act on.
			-->
			<p>
				Sign-in is not configured in this build, so there is no way to present a credential from
				here. Reports remain public; operational data does not.
			</p>
		{/if}
	{:else if remedy === 'try-again'}
		<Button onclick={() => onRetry?.()} disabled={busy}>
			{busy ? 'Reading…' : 'Try again'}
		</Button>
	{/if}
</section>

<style>
	/*
	 * A refusal is the whole page when it happens, so it gets a section's breathing room
	 * rather than sitting tight under the heading as though it were a subtitle.
	 */
	.refusal {
		display: flex;
		flex-direction: column;
		align-items: start;
		gap: var(--space-4);
		padding-block-start: var(--space-8);
		max-width: var(--measure);
	}

	.refusal h2 {
		margin: 0;
	}

	.refusal p {
		margin: 0;
		color: var(--text-secondary);
	}
</style>
