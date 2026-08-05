<script lang="ts">
	/*
	 * One checkable fact.
	 *
	 * The excerpt is already truncated server-side — the frontend is never the thing that
	 * stops a five-megabyte payload, because by the time this component could decide, the
	 * bytes have crossed the network and been parsed. What this component owes the reader
	 * is that `truncated` is *said*, not implied: an excerpt that stops mid-file without
	 * saying so reads as a complete file that ends there.
	 *
	 * The `digest` is of the **full** source, not of the excerpt. That is what makes the
	 * evidence checkable against the commit, so it is copyable in full.
	 */
	import CopyableSha from '$lib/components/primitives/CopyableSha.svelte';
	import { evidenceKind } from '$lib/contract/enums';
	import type { Evidence } from '@repolens/api-client';

	interface Props {
		evidence: Evidence;
	}

	let { evidence }: Props = $props();

	const kind = $derived(evidenceKind(evidence.kind));
</script>

<div class="evidence">
	<p class="evidence__meta">
		<span class="evidence__kind" data-kind={kind.raw}>
			{kind.label}
		</span>
		{#if evidence.path}
			<code class="evidence__path">{evidence.path}</code>
		{/if}
		{#if evidence.line_range}
			<span class="evidence__lines">
				lines {evidence.line_range.start}–{evidence.line_range.end}
			</span>
		{/if}
	</p>

	{#if evidence.excerpt}
		<!--
			`tabindex="0"` because the block scrolls: a region reachable only by mouse wheel is
			unreachable content for a keyboard user, and Safari gives it no affordance at all.
			axe's `scrollable-region-focusable`, which is a serious failure rather than advice.
			Svelte's `a11y_no_noninteractive_tabindex` disagrees; scrolling is the interaction,
			and it is suppressed narrowly here rather than by turning the rule off.
		-->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<!-- prettier-ignore -->
		<pre class="evidence__excerpt" tabindex="0"><code>{evidence.excerpt}</code></pre>
	{/if}

	{#if evidence.truncated}
		<p class="evidence__note">
			Truncated by the server. The file continues past what is shown{evidence.excerpt
				? ''
				: ', and no excerpt was included'}.
		</p>
	{/if}

	{#if evidence.digest}
		<p class="evidence__note">
			Digest of the full file, not of the excerpt:
			<CopyableSha value={evidence.digest} label="content digest" />
		</p>
	{/if}
</div>

<style>
	.evidence {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.evidence__meta {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		max-inline-size: none;
		margin: 0;
		font-size: var(--font-size-sm);
	}

	.evidence__kind {
		color: var(--text-secondary);
		font-weight: var(--font-weight-medium);
	}

	.evidence__path {
		overflow-wrap: anywhere;
	}

	.evidence__lines {
		color: var(--text-muted);
	}

	/* Evidence scrolls inside its own container — the page body never does. */
	.evidence__excerpt {
		margin: 0;
		max-block-size: 24rem;
		overflow: auto;
	}

	.evidence__note {
		max-inline-size: var(--measure);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}
</style>
