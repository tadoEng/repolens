<script lang="ts">
	/*
	 * A top-level report section, and the anchor target its nav link points at.
	 *
	 * `tabindex="-1"` on the heading is the load-bearing attribute. Clicking an in-page
	 * link scrolls the viewport but leaves focus at the top of the document in every major
	 * browser, so a keyboard user who "jumps to Findings" then Tabs from the beginning of
	 * the page again. This is the single most commonly shipped accessibility bug of its
	 * kind, and the fix has to exist on the *target*, not only in the link handler — the
	 * heading is not focusable without it.
	 *
	 * The ring is suppressed for `[tabindex="-1"]:focus` in global.css: a focus ring on a
	 * heading nobody tabbed to reads as a rendering bug rather than as an affordance.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		/** Anchor id. Also the heading's id, so focus and `:target` land on the same node. */
		id: string;
		title: string;
		/** Optional lead paragraph rendered before the section body. */
		lead?: string;
		children: Snippet;
	}

	let { id, title, lead, children }: Props = $props();
</script>

<section class="section" aria-labelledby={id}>
	<h2 class="section__title" {id} tabindex="-1">{title}</h2>
	{#if lead}
		<p class="section__lead">{lead}</p>
	{/if}
	{@render children()}
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
</style>
