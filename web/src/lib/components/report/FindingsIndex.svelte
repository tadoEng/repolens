<script lang="ts">
	/*
	 * Every finding in one place — as an index, not as a second set of cards.
	 *
	 * The four category sections above carry the findings in full. This section answers a
	 * different question: *what did the ruleset conclude, altogether, and where does each
	 * conclusion live?* One screen, server order, three axes visible side by side, and a
	 * link to the card. That is genuinely useful next to a hierarchy — a duplicate of the
	 * cards would not be, and would duplicate every `finding-…` anchor id along with it.
	 *
	 * **State, severity and confidence stay three columns.** They are three different
	 * questions — what the analyzer concluded, the impact if it is right, and how strong the
	 * evidence is — and a table that merged any two would be the same defect as a merged
	 * badge, just wider.
	 */
	import ConfidenceBadge from '$lib/components/primitives/ConfidenceBadge.svelte';
	import ScrollRegion from '$lib/components/primitives/ScrollRegion.svelte';
	import SeverityBadge from '$lib/components/primitives/SeverityBadge.svelte';
	import StatusChip from '$lib/components/primitives/StatusChip.svelte';
	import { findingCategory } from '$lib/contract/enums';
	import type { Finding } from '@repolens/api-client';

	import { focusAnchor } from './anchor';

	interface Props {
		findings: Finding[];
	}

	let { findings }: Props = $props();
</script>

{#if findings.length === 0}
	<p>This report contains no findings.</p>
{:else}
	<ScrollRegion label="Every finding, with its category and three axes">
		<table class="index">
			<!--
				Labels the data rather than restating the section's lead. A caption that repeats
				the paragraph above it is read twice by a screen reader and skimmed past by
				everyone else.
			-->
			<caption>Every finding, with the section it reads under.</caption>
			<thead>
				<tr>
					<th scope="col">Finding</th>
					<th scope="col">Category</th>
					<th scope="col">State</th>
					<th scope="col">Severity</th>
					<th scope="col">Confidence</th>
				</tr>
			</thead>
			<tbody>
				{#each findings as finding (finding.id)}
					{@const category = findingCategory(finding.category)}
					<tr>
						<th scope="row">
							<a
								href={`#finding-${finding.id}`}
								onclick={() => focusAnchor(`finding-${finding.id}`)}
							>
								{finding.title}
							</a>
							<span class="index__rule"><code>{finding.rule_id}</code></span>
						</th>
						<td>{category.label}</td>
						<td><StatusChip state={finding.state} /></td>
						<td><SeverityBadge value={finding.severity} /></td>
						<td><ConfidenceBadge value={finding.confidence} /></td>
					</tr>
				{/each}
			</tbody>
		</table>
	</ScrollRegion>
{/if}

<style>
	.index {
		min-inline-size: 44rem;
		font-size: var(--font-size-sm);
	}

	.index th[scope='row'] {
		vertical-align: top;
		font-size: var(--font-size-sm);
	}

	.index__rule {
		display: block;
		margin-block-start: var(--space-1);
		font-weight: var(--font-weight-regular);
		color: var(--text-muted);
	}
</style>
