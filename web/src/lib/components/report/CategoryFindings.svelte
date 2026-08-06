<script lang="ts">
	/*
	 * The findings that read under one report section, as full cards.
	 *
	 * **The order is the server's and is not re-sorted here.** "Ordering is part of the
	 * contract. A report that listed findings differently on each load would contradict the
	 * determinism it claims." Grouping by category preserves relative order within each
	 * group, so the two are compatible; sorting by severity would not be.
	 *
	 * ## Every finding is a card exactly once
	 *
	 * A finding's card carries its `id` anchor, which the overview and the evidence appendix
	 * link to. Rendering the same finding in two sections would duplicate that id — a
	 * genuine accessibility failure, and a link that lands on whichever copy the browser
	 * happens to find first. So the four category sections partition the findings, and the
	 * Findings section is an *index* over them rather than a second copy.
	 *
	 * ## The empty state does not borrow a `FindingState`
	 *
	 * A section with no findings is tempting to render as `MISSING` or `UNABLE_TO_VERIFY`,
	 * and both would be wrong. Those are the analyzer's conclusions about a *checked
	 * property*; "this ruleset produced no finding in this category" is a fact about the
	 * ruleset. Dressing it in the contract's vocabulary would state something the analysis
	 * never said, so it is plain neutral prose.
	 */
	import { findingCategory } from '$lib/contract/enums';
	import type { Finding } from '@repolens/api-client';
	import type { SvelteSet } from 'svelte/reactivity';

	import FindingCard from './FindingCard.svelte';

	interface Props {
		/** Already filtered to this section by `findingsForSection`. */
		findings: Finding[];
		/**
		 * Expanded evidence, keyed by finding id and shared across the whole report.
		 *
		 * Held by identity in a reactive set rather than as a flag on a copy of the findings:
		 * the reader's expanded state survives a re-render, and the findings array is never
		 * cloned — a copy is how a "server-decided order" quietly becomes a client-decided
		 * one.
		 */
		expanded: SvelteSet<string>;
		/** Names this section in the empty state, e.g. `architecture`. */
		emptyLabel: string;
	}

	let { findings, expanded, emptyLabel }: Props = $props();

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

	function setOne(id: string, open: boolean): void {
		if (open) expanded.add(id);
		else expanded.delete(id);
	}
</script>

{#if findings.length === 0}
	<p class="category__empty">
		No finding in this report is categorised under {emptyLabel}. That is a fact about what this
		ruleset produced, not a conclusion about the repository — nothing was checked and found absent
		here.
	</p>
{:else}
	{#each groups as group (group.category.raw)}
		<section class="category__group" aria-labelledby={`findings-${group.category.token}`}>
			<!--
				Always rendered, even when the section holds a single group. Dropping it would
				put an `h4` card directly under the section's `h2` and skip a heading level.
			-->
			<h3 class="category__heading" id={`findings-${group.category.token}`}>
				{group.category.label}
			</h3>
			<ul class="category__list">
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
	.category__empty {
		max-inline-size: var(--measure);
		margin: 0;
		color: var(--text-secondary);
	}

	.category__group {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.category__group + .category__group {
		margin-block-start: var(--space-8);
	}

	.category__heading {
		margin: 0;
	}

	.category__list {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
