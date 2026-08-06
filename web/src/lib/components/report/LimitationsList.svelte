<script lang="ts">
	/*
	 * What a finding — or the report as a whole — does not establish.
	 *
	 * **Always visible, never behind hover.** Tooltips do not exist on touch, they are
	 * transient, and WCAG 1.4.13 requires hover content to be dismissible, hoverable and
	 * persistent. A limitation is first-class information: it is the sentence that stops
	 * "no architecture document was found" from being read as "this project has no
	 * architecture". So it renders as inline text.
	 *
	 * The `code` is shown next to the prose because it is stable across ruleset versions
	 * and the prose is not — it is what a reader quotes when asking why.
	 */
	import type { Limitation } from '@repolens/api-client';

	interface Props {
		limitations: Limitation[];
		/** Introduces the list. Differs between a finding and the whole report. */
		label?: string;
	}

	let { limitations, label = 'What this does not establish' }: Props = $props();
</script>

{#if limitations.length > 0}
	<div class="limitations">
		<p class="limitations__label">{label}</p>
		<ul class="limitations__list">
			{#each limitations as limitation (limitation.code)}
				<li class="limitations__item">
					<code>{limitation.code}</code>
					<span>{limitation.explanation}</span>
				</li>
			{/each}
		</ul>
	</div>
{/if}

<style>
	.limitations {
		padding: var(--space-3);
		border: var(--border-width) dotted var(--status-unable-to-verify-border);
		border-radius: var(--radius-md);
		background-color: var(--status-unable-to-verify-bg);
	}

	.limitations__label {
		margin: 0 0 var(--space-2);
		font-size: var(--font-size-sm);
		font-weight: var(--font-weight-medium);
		color: var(--status-unable-to-verify-fg);
	}

	.limitations__list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.limitations__item {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		max-inline-size: var(--measure);
		font-size: var(--font-size-sm);
		color: var(--text-primary);
	}
</style>
