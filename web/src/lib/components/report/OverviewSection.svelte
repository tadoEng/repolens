<script lang="ts">
	/*
	 * The executive overview.
	 *
	 * There is no universal score in RepoLens, so this section carries the entire
	 * summarization load. That makes it the one place where a statement could float free of
	 * its evidence — which is why every statement renders its own confidence and links to
	 * the findings that support it, by `rule_id`, rather than asserting on its own
	 * authority.
	 *
	 * An unresolvable `rule_id` is shown as plain text rather than as a link to nowhere.
	 * The reader still sees which rule was claimed to support the statement, which is the
	 * information that matters; a dead anchor would merely hide the mismatch.
	 */
	import ConfidenceBadge from '$lib/components/primitives/ConfidenceBadge.svelte';
	import type { Finding, Limitation, OverviewStatement } from '@repolens/api-client';

	import { focusAnchor } from './anchor';
	import LimitationsList from './LimitationsList.svelte';

	interface Props {
		overview: OverviewStatement[];
		/** Used only to resolve `rule_id` to the anchor of the finding that carries it. */
		findings: Finding[];
		/** Report-level limitations, shown here rather than buried inside a finding. */
		limitations: Limitation[];
	}

	let { overview, findings, limitations }: Props = $props();

	const anchorByRule = $derived(
		new Map(findings.map((finding) => [finding.rule_id, `finding-${finding.id}`]))
	);
</script>

{#if overview.length === 0}
	<p class="overview__empty">
		This report has no overview statements. That is not the same as having nothing to say — the
		findings below stand on their own evidence.
	</p>
{:else}
	<ol class="overview">
		{#each overview as statement, index (index)}
			<li class="overview__item">
				<p class="overview__statement">{statement.statement}</p>
				<div class="overview__support">
					<ConfidenceBadge value={statement.confidence} />
					{#if statement.supporting_rule_ids.length > 0}
						<span class="overview__rules">
							Supported by
							{#each statement.supporting_rule_ids as ruleId, ruleIndex (ruleId)}
								{#if ruleIndex > 0}<span aria-hidden="true">, </span>{/if}
								{#if anchorByRule.has(ruleId)}
									{@const anchor = anchorByRule.get(ruleId) ?? ''}
									<a href={`#${anchor}`} onclick={() => focusAnchor(anchor)}>
										<code>{ruleId}</code>
									</a>
								{:else}
									<code>{ruleId}</code>
								{/if}
							{/each}
						</span>
					{:else}
						<span class="overview__rules"> No supporting rule was named for this statement. </span>
					{/if}
				</div>
			</li>
		{/each}
	</ol>
{/if}

<LimitationsList {limitations} label="What this report as a whole does not establish" />

<style>
	.overview {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		margin: 0;
		padding: 0;
		list-style: none;
		counter-reset: overview;
	}

	.overview__item {
		counter-increment: overview;
		padding-inline-start: var(--space-8);
		position: relative;
	}

	/* The number is decoration over an ordered list that already conveys order, so it is
	 * hidden from assistive technology rather than read out twice. */
	.overview__item::before {
		content: counter(overview);
		position: absolute;
		inset-inline-start: 0;
		inset-block-start: 0;
		font-variant-numeric: tabular-nums;
		font-size: var(--font-size-lg);
		color: var(--text-muted);
	}

	.overview__statement {
		margin: 0;
		font-size: var(--font-size-lg);
	}

	.overview__support {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2) var(--space-3);
		margin-block-start: var(--space-2);
	}

	.overview__rules {
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.overview__empty {
		margin: 0;
		color: var(--text-secondary);
	}
</style>
