<script lang="ts">
	/*
	 * Why a submission did not start an analysis.
	 *
	 * Presentational, and driven by the two failing arms of `Fetched<T>` rather than by a
	 * flattened "error string" — because the two arms are different facts about the
	 * deployment and the whole reason that union has three members is that collapsing them
	 * produces the classic misdiagnosis: a CORS or CSP failure reported to the reader as
	 * "not found".
	 *
	 * ## What is rendered, and from where
	 *
	 * - The **server's own `message`**, verbatim. It is the only sentence that knows what
	 *   actually happened — that the URL was not a repository, that the account is not
	 *   signed in, that the deployment verifies no tokens. Paraphrasing it here would put
	 *   this component in the business of predicting the API's failures.
	 * - The **code**, with a label from `@repolens/api-client` (through
	 *   `$lib/contract/enums`, which forwards `describeErrorCode` and adds nothing to it).
	 *   A second label table in this file would compile, would pass, and would quietly
	 *   disagree with the contract — an unrecognised code still renders, named, rather than
	 *   being dropped.
	 * - The **status**, because "the API answered 503" and "the API never answered" are the
	 *   distinction this component exists to keep visible.
	 *
	 * `role="alert"` rather than moved focus: the reader is standing on the submit button
	 * they just pressed, which is where they want to stay. Announce the outcome, do not
	 * relocate them into it.
	 */
	import type { ApiError } from '$lib/api/analysis';
	import { errorCode } from '$lib/contract/enums';

	interface Props {
		/** The failing arms of `Fetched<Analysis>`, passed through unchanged. */
		outcome: { kind: 'rejected'; status: number; error: ApiError | null } | { kind: 'unreachable' };
	}

	let { outcome }: Props = $props();

	const code = $derived(outcome.kind === 'rejected' ? errorCode(outcome.error?.code) : null);
</script>

<div class="submit-error" role="alert">
	{#if outcome.kind === 'unreachable'}
		<p class="submit-error__headline">The API could not be reached.</p>
		<p>
			The request never reached a server, so this says nothing about the repository you entered — it
			is a transport or configuration failure, not a missing or invalid repository. Check your
			connection and try again.
		</p>
	{:else}
		<p class="submit-error__headline">This analysis could not be started.</p>
		{#if outcome.error}
			<p>{outcome.error.message}</p>
		{:else}
			<p>The API answered with status {outcome.status} and sent no explanation.</p>
		{/if}
		{#if code && code.raw}
			<p class="submit-error__code">
				{code.label} · <code>{code.raw}</code> · status {outcome.status}
			</p>
		{/if}
	{/if}
</div>

<style>
	.submit-error {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-block-start: var(--space-6);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-strong);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.submit-error p {
		margin: 0;
	}

	.submit-error__headline {
		font-weight: var(--font-weight-medium);
	}

	.submit-error__code {
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}
</style>
