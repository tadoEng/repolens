<script lang="ts">
	/*
	 * One finding, with everything a reader needs to disagree with it.
	 *
	 * The three axes are rendered **separately and always**: state (what the analyzer
	 * concluded), severity (impact if valid), confidence (strength of the evidence). They
	 * are three different questions, and a report that answers them with one badge is a
	 * report where a low-confidence guess about something important is indistinguishable
	 * from a measurement.
	 *
	 * Evidence sits behind a disclosure so the main report stays readable, and the
	 * disclosure's `open` is bindable so the section can expand every finding at once —
	 * without that, a closed `<details>` hides its excerpt from browser find-in-page, and
	 * a file path is the thing people search an evidence report for.
	 */
	import ConfidenceBadge from '$lib/components/primitives/ConfidenceBadge.svelte';
	import Disclosure from '$lib/components/primitives/Disclosure.svelte';
	import SeverityBadge from '$lib/components/primitives/SeverityBadge.svelte';
	import StatusChip from '$lib/components/primitives/StatusChip.svelte';
	import type { Finding } from '@repolens/api-client';

	import EvidenceItem from './EvidenceItem.svelte';
	import LimitationsList from './LimitationsList.svelte';

	interface Props {
		finding: Finding;
		/** Controlled, so a whole section can be expanded or collapsed in one action. */
		evidenceOpen?: boolean;
		onEvidenceOpenChange?: (open: boolean) => void;
	}

	let { finding, evidenceOpen = false, onEvidenceOpenChange }: Props = $props();

	const headingId = $derived(`finding-${finding.id}`);
	const evidenceCount = $derived(finding.evidence.length);
</script>

<article class="finding" aria-labelledby={headingId}>
	<!--
		`tabindex="-1"` for the same reason the section headings carry it: the overview links
		to findings by rule id, and a jump that scrolls without moving focus leaves a
		keyboard user Tabbing from the top of the document.
	-->
	<h4 class="finding__title" id={headingId} tabindex="-1">{finding.title}</h4>

	<p class="finding__rule">
		<code>{finding.rule_id}</code>
		<span>ruleset {finding.ruleset_version}</span>
	</p>

	<!--
		A list, not a row of spans: three sibling values of equal standing, and a screen
		reader announces "list of 3 items" rather than running them into one another.
	-->
	<ul class="finding__axes">
		<li><StatusChip state={finding.state} /></li>
		<li><SeverityBadge value={finding.severity} /></li>
		<li><ConfidenceBadge value={finding.confidence} /></li>
	</ul>

	<p class="finding__explanation">{finding.explanation}</p>

	{#if finding.recommended_action}
		<p class="finding__action">
			<strong>Suggested next step.</strong>
			{finding.recommended_action}
		</p>
	{/if}

	<LimitationsList limitations={finding.limitations} />

	{#if evidenceCount > 0}
		<Disclosure
			summary="Evidence"
			hint={`${evidenceCount} item${evidenceCount === 1 ? '' : 's'}`}
			open={evidenceOpen}
			onOpenChange={onEvidenceOpenChange}
		>
			<ul class="finding__evidence">
				{#each finding.evidence as item, index (`${item.kind}-${item.path ?? index}`)}
					<li><EvidenceItem evidence={item} /></li>
				{/each}
			</ul>
		</Disclosure>
	{:else}
		<!--
			No empty disclosure. `UNABLE_TO_VERIFY` is precisely the case where there is
			nothing to show, and an expander that opens onto nothing implies the evidence is
			hidden rather than absent. The limitations above say why.
		-->
		<p class="finding__no-evidence">
			No evidence is attached to this finding. Where the analyzer could not check something, there
			is nothing to show — the limitation above says what stopped it.
		</p>
	{/if}
</article>

<style>
	.finding {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background-color: var(--surface-0);
	}

	.finding__title {
		margin: 0;
		font-size: var(--font-size-lg);
	}

	.finding__rule {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		max-inline-size: none;
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.finding__axes {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.finding__explanation,
	.finding__action,
	.finding__no-evidence {
		margin: 0;
	}

	.finding__no-evidence {
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.finding__evidence {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.finding__evidence > li + li {
		padding-block-start: var(--space-4);
		border-block-start: var(--border-width) solid var(--border-subtle);
	}
</style>
