<script lang="ts">
	/*
	 * One section of the operational dashboard.
	 *
	 * Mirrors `ReportSection` — same anchor-and-focus arrangement, and `tabindex="-1"` on the
	 * heading for the same reason — with one addition that is the whole justification for a
	 * second component rather than a reuse: a section can be **present and unmeasured**.
	 *
	 * `/admin` shows five sections because five are what an operator would look for. Three
	 * carry measurements. Two do not, and the honest rendering of those two is to name them
	 * and say so, not to omit them and let a three-section page read as a complete picture.
	 * Omission is how a measurement gap becomes invisible exactly where somebody went looking
	 * for it — the same rule the report contract keeps when it makes a limitation part of the
	 * payload rather than an absence in it.
	 *
	 * The alternative was filling them with whatever numbers happened to be reachable. Two
	 * pool gauges are not PostgreSQL observability, and a panel assembled from what was
	 * available rather than from what was measured is the over-claim this project exists to
	 * refuse.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		/** Anchor id. Also the heading's id, so focus and `:target` land on the same node. */
		id: string;
		title: string;
		/** Optional lead paragraph rendered before the section body. */
		lead?: string;
		/**
		 * Why this section carries no measurements, when it carries none.
		 *
		 * Present means the section is unmeasured and `children` is not rendered at all —
		 * rather than rendered empty, which would look like a load that failed. Absent means
		 * the section has data and this is an ordinary section.
		 */
		notInstrumented?: string;
		children?: Snippet;
	}

	let { id, title, lead, notInstrumented, children }: Props = $props();
</script>

<section class="section" aria-labelledby={id}>
	<h2 class="section__title" {id} tabindex="-1">{title}</h2>
	{#if lead}
		<p class="section__lead">{lead}</p>
	{/if}

	{#if notInstrumented}
		<!--
			A paragraph, not a badge or an empty state illustration. The reader needs the
			sentence — what is not measured, and that its absence is a decision rather than a
			failure — and a badge cannot carry it. The status word is in the text so it
			survives greyscale, forced colours, and a screen reader, none of which see the
			border.
		-->
		<p class="section__unmeasured">
			<strong>Not instrumented in Experimental-v1.</strong>
			{notInstrumented}
		</p>
	{:else}
		{@render children?.()}
	{/if}
</section>

<style>
	.section {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding-block-start: var(--space-12);
	}

	.section__title {
		margin: 0;
		/* Keeps the heading clear of a jumped-to position at the very top of the viewport. */
		scroll-margin-block-start: var(--space-8);
	}

	.section__lead {
		margin: 0;
		color: var(--text-secondary);
	}

	.section__unmeasured {
		margin: 0;
		padding: var(--space-4);
		border: 1px dashed var(--border-subtle);
		border-radius: var(--radius-md);
		color: var(--text-secondary);
	}
</style>
