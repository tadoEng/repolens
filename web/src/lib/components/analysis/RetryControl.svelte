<script lang="ts">
	/*
	 * Retry, exactly as the server permits it.
	 *
	 * **`retry.allowed` is the only input. The state name is never consulted.**
	 * `FAILED_RETRIABLE` describes the *kind* of failure; whether a retry would actually be
	 * accepted also depends on how many attempts have been spent and whether the work is
	 * still claimable — facts only the server holds. A frontend that inferred this from the
	 * state would render a button that does nothing, which is worse than no button: it
	 * spends the reader's attention and then fails silently.
	 *
	 * When retry is refused, the server's `reason` is displayed verbatim. It is written to
	 * explain rather than merely deny, and paraphrasing it here would drop the explanation.
	 *
	 * **No confirmation dialog.** Confirmations are for destructive actions; retry is
	 * idempotent. And no toast — the outcome renders in place, where the reader is looking.
	 */
	import Button from '$lib/components/primitives/Button.svelte';
	import type { RetryPolicy } from '@repolens/api-client';

	interface Props {
		retry: RetryPolicy;
		/** Runs the retry request. Called only when `retry.allowed` is true. */
		onRetry: () => void | Promise<void>;
		/** True while a retry request is in flight. */
		busy?: boolean;
		/** Set when a retry request itself failed, so the reader is not left guessing. */
		problem?: string | null;
	}

	let { retry, onRetry, busy = false, problem = null }: Props = $props();
</script>

<div class="retry">
	{#if retry.allowed}
		<Button variant="primary" {busy} onclick={() => void onRetry()}>
			{busy ? 'Retrying…' : 'Retry this analysis'}
		</Button>
	{:else}
		<p class="retry__denied">
			<strong>Retry is not available.</strong>
			{#if retry.reason}
				{retry.reason}
			{:else}
				The server did not permit a retry and gave no reason.
			{/if}
		</p>
	{/if}

	<!--
		Rendered from the start rather than created on failure: a live region that appears
		together with its first message is frequently missed by assistive technology.
	-->
	<p class="retry__result" role="status">
		{#if problem}
			{problem}
		{/if}
	</p>
</div>

<style>
	.retry {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		align-items: flex-start;
	}

	.retry__denied {
		max-inline-size: var(--measure);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.retry__result {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--status-unable-to-verify-fg);
	}
</style>
