<script lang="ts">
	/*
	 * What went wrong, and what can be done about it.
	 *
	 * The server's `message` is displayed verbatim. It is written to be shown — deliberately
	 * free of internal identifiers, hostnames and credentials — and rewriting it in the
	 * client would guarantee the two drift. The `code` is shown next to it because that is
	 * the stable thing: a reader quotes the code, and support switches on it.
	 *
	 * `retry_after_seconds` is absent rather than zero when it is unknown, so a countdown
	 * appears only when the server actually supplied one. "Retry in 0s" rendered from a
	 * missing value is worse than no countdown at all.
	 */
	import { errorCode } from '$lib/contract/enums';
	import { duration } from '$lib/contract/format';
	import type { ApiError, RetryPolicy } from '@repolens/api-client';

	import RetryNotice from './RetryNotice.svelte';

	interface Props {
		/** Present on both failure states, but the contract makes it optional — so is this. */
		error: ApiError | null | undefined;
		retry: RetryPolicy;
	}

	let { error, retry }: Props = $props();

	const code = $derived(errorCode(error?.code));
</script>

<div class="failure">
	{#if error}
		<p class="failure__headline">
			{code.label}
		</p>
		<p class="failure__message">{error.message}</p>
		<p class="failure__code"><code>{error.code}</code></p>

		{#if error.retry_after_seconds !== null && error.retry_after_seconds !== undefined}
			<p class="failure__wait">
				The server asked for a wait of about {duration(error.retry_after_seconds)} before trying again.
			</p>
		{/if}
	{:else}
		<p class="failure__message">
			The analysis failed and the server did not describe why. The state below is all that is known.
		</p>
	{/if}

	<RetryNotice {retry} />
</div>

<style>
	.failure {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-strong);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.failure__headline {
		margin: 0;
		font-size: var(--font-size-lg);
		font-weight: var(--font-weight-bold);
	}

	.failure__message {
		margin: 0;
	}

	.failure__code,
	.failure__wait {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}
</style>
