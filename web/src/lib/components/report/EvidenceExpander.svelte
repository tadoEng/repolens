<script lang="ts">
	/*
	 * "Expand all evidence", for the whole document.
	 *
	 * Content inside a closed `<details>` is invisible to browser find-in-page in several
	 * engines, and a file path is exactly what someone searches an evidence report for. It
	 * also serves printing and sharing, which is why it is a visible control rather than a
	 * keyboard shortcut.
	 *
	 * It lives near the top of the report, above every section it affects, because the cards
	 * are now spread across four category sections. A control that sat inside one of them
	 * would appear to be about that section alone.
	 */
	import Button from '$lib/components/primitives/Button.svelte';
	import type { Finding } from '@repolens/api-client';
	import type { SvelteSet } from 'svelte/reactivity';

	interface Props {
		/** Every finding in the report, so "all" means all. */
		findings: Finding[];
		/** The shared expanded set, mutated in place. */
		expanded: SvelteSet<string>;
	}

	let { findings, expanded }: Props = $props();

	const allExpanded = $derived(findings.length > 0 && expanded.size === findings.length);

	function setAll(open: boolean): void {
		expanded.clear();
		if (open) for (const finding of findings) expanded.add(finding.id);
	}
</script>

<div class="expander">
	<Button onclick={() => setAll(!allExpanded)}>
		{allExpanded ? 'Collapse all evidence' : 'Expand all evidence'}
	</Button>
	<p class="expander__hint">
		Evidence is collapsed by default to keep the report readable. Expanding it also makes every
		excerpt findable with your browser's search.
	</p>
</div>

<style>
	.expander {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3) var(--space-4);
	}

	.expander__hint {
		flex: 1 1 20rem;
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}
</style>
