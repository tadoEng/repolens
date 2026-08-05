<script lang="ts">
	/*
	 * Every finding, grouped by category, in the server's order.
	 *
	 * **The order is the server's and is not re-sorted here.** "Ordering is part of the
	 * contract. A report that listed findings differently on each load would contradict the
	 * determinism it claims." Grouping by category preserves relative order within each
	 * group, so the two are compatible; sorting by severity would not be.
	 *
	 * "Expand all evidence" exists because a closed `<details>` is invisible to browser
	 * find-in-page in several engines, and a file path is exactly what someone searches an
	 * evidence report for. It also serves printing and sharing, which is why it is a
	 * visible control rather than a keyboard shortcut.
	 */
	import { SvelteSet } from 'svelte/reactivity';

	import Button from '$lib/components/primitives/Button.svelte';
	import { findingCategory } from '$lib/contract/enums';
	import type { Finding } from '@repolens/api-client';

	import FindingCard from './FindingCard.svelte';

	interface Props {
		findings: Finding[];
	}

	let { findings }: Props = $props();

	/*
	 * Expanded evidence, held by finding id in a reactive set rather than as a flag on a
	 * copy of the findings. Keying by identity means the reader's expanded state survives
	 * a re-render of the list, and the findings array itself is never cloned — a copy is
	 * how a "server-decided order" quietly becomes a client-decided one.
	 */
	const expanded = new SvelteSet<string>();

	const allExpanded = $derived(findings.length > 0 && expanded.size === findings.length);

	/**
	 * Category groups in first-appearance order, preserving the server's sequence.
	 *
	 * A plain array rather than a `Map`: the groups are a handful, the linear scan is
	 * irrelevant at this size, and a mutable `Map` inside a `$derived` is the shape
	 * `svelte/prefer-svelte-reactivity` flags — correctly, since a reader cannot tell at a
	 * glance whether it was meant to be reactive state.
	 */
	const groups = $derived.by(() => {
		const ordered: { category: ReturnType<typeof findingCategory>; findings: Finding[] }[] = [];

		for (const finding of findings) {
			const existing = ordered.find((group) => group.category.raw === finding.category);
			if (existing) existing.findings.push(finding);
			else ordered.push({ category: findingCategory(finding.category), findings: [finding] });
		}

		return ordered;
	});

	function setAll(open: boolean): void {
		expanded.clear();
		if (open) for (const finding of findings) expanded.add(finding.id);
	}

	function setOne(id: string, open: boolean): void {
		if (open) expanded.add(id);
		else expanded.delete(id);
	}
</script>

<div class="findings__controls">
	<Button onclick={() => setAll(!allExpanded)}>
		{allExpanded ? 'Collapse all evidence' : 'Expand all evidence'}
	</Button>
	<p class="findings__hint">
		Evidence is collapsed by default to keep the report readable. Expanding it also makes every
		excerpt findable with your browser's search.
	</p>
</div>

{#if findings.length === 0}
	<p>This report contains no findings.</p>
{:else}
	{#each groups as group (group.category.raw)}
		<section class="findings__group" aria-labelledby={`findings-${group.category.token}`}>
			<h3 class="findings__category" id={`findings-${group.category.token}`}>
				{group.category.label}
			</h3>
			<ul class="findings__list">
				{#each group.findings as finding (finding.id)}
					<li>
						<FindingCard
							{finding}
							evidenceOpen={expanded.has(finding.id)}
							onEvidenceOpenChange={(open) => setOne(finding.id, open)}
						/>
					</li>
				{/each}
			</ul>
		</section>
	{/each}
{/if}

<style>
	.findings__controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3) var(--space-4);
	}

	.findings__hint {
		flex: 1 1 20rem;
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.findings__group {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin-block-start: var(--space-8);
	}

	.findings__category {
		margin: 0;
	}

	.findings__list {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
