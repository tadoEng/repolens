<script lang="ts">
	/*
	 * Whether this analysis could be retried, and why nothing here starts one.
	 *
	 * ## There is no button, and its absence is stated rather than implied
	 *
	 * Retry is an authenticated mutation that starts paid work. The published contract does
	 * not define the operation, so there is no generated client method, no request or error
	 * schema, no Firebase bearer credential (#13) and no declared idempotency semantics. A
	 * hand-written `POST` would have supplied none of those, and its failure mode is not an
	 * error message — it is a second analysis billed against a repository the reader already
	 * queued.
	 *
	 * The alternative to a wrong button is **not** an empty space. A failure page that simply
	 * omits the affordance reads as a rendering bug, and the reader is left with no account
	 * of whether another attempt is possible. So both facts are shown: what the *server* says
	 * about a retry, and what this *build* can do about it. They are separate sentences
	 * because they are separate facts, and conflating them is how "we cannot ask yet" becomes
	 * indistinguishable from "the server refused".
	 *
	 * ## `retry.allowed` still drives what is said
	 *
	 * **The state name is never consulted.** `FAILED_RETRIABLE` describes the *kind* of
	 * failure; whether a retry would actually be accepted also depends on attempts already
	 * spent and whether the work is still claimable — facts only the server holds. When it
	 * refuses, its `reason` is displayed verbatim, because it is written to explain rather
	 * than merely deny and paraphrasing it here would drop the explanation.
	 */
	import type { RetryPolicy } from '@repolens/api-client';

	interface Props {
		retry: RetryPolicy;
	}

	let { retry }: Props = $props();
</script>

<div class="retry">
	{#if retry.allowed}
		<p class="retry__lead"><strong>Retry is not available in this build.</strong></p>
		<p class="retry__detail">
			The server reports that it would accept another attempt for this analysis. Starting one is an
			authenticated operation that the published API contract does not define yet, so this page
			deliberately offers no control rather than sending a request whose behaviour on a repeat is
			undefined.
		</p>
	{:else}
		<p class="retry__lead"><strong>Retry is not available.</strong></p>
		<p class="retry__detail">
			{#if retry.reason}
				{retry.reason}
			{:else}
				The server did not permit a retry and gave no reason.
			{/if}
		</p>
	{/if}

	<p class="retry__detail">
		Nothing about the analysis has changed, and its address stays valid. Reload this page to pick up
		a newer state.
	</p>
</div>

<style>
	.retry {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		align-items: flex-start;
	}

	.retry__lead {
		margin: 0;
	}

	.retry__detail {
		max-inline-size: var(--measure);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}
</style>
