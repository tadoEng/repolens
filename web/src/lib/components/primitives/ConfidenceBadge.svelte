<script lang="ts">
	/*
	 * Confidence: **strength of the evidence.**
	 *
	 * The other half of the pair, and deliberately a second component rather than a
	 * `kind="confidence"` prop on one. A shared component is one refactor away from a
	 * shared badge, and a shared badge is how a `LOW`-confidence guess ends up rendered
	 * identically to a `HIGH`-confidence measurement.
	 *
	 * Shape differs from `SeverityBadge` — pill, not square — so the two axes are
	 * separable without reading the label, in greyscale, and in forced-colors mode.
	 */
	import { confidence } from '$lib/contract/enums';

	interface Props {
		/** Raw wire value. `string`, because that is what a response contains. */
		value: string | null | undefined;
	}

	let { value }: Props = $props();

	const display = $derived(confidence(value));
</script>

<span class="badge badge--{display.token}" data-confidence={display.raw}>
	<span class="badge__axis">Confidence</span>
	<span class="badge__value">{display.label}</span>
</span>

<style>
	.badge {
		display: inline-flex;
		align-items: baseline;
		gap: var(--space-1);
		/* Pill. Squares are severity. */
		border-radius: 999px;
		border: var(--border-width) solid var(--border-subtle);
		background-color: var(--surface-1);
		padding: 0.1em var(--space-3);
		font-size: var(--font-size-sm);
		line-height: var(--line-height-body);
	}

	.badge__axis {
		color: var(--text-muted);
		text-transform: lowercase;
	}

	.badge__value {
		color: var(--text-secondary);
		font-weight: var(--font-weight-medium);
	}

	/*
	 * The scale runs the *opposite* way from severity: high confidence is the reassuring
	 * end, low confidence is the one a reader must not miss. So LOW gets the emphasis
	 * here, where HIGH gets it on severity. Both are still carried by the word first.
	 */
	.badge--low {
		border-color: var(--border-strong);
	}

	.badge--low .badge__value {
		color: var(--text-primary);
		font-weight: var(--font-weight-bold);
	}

	.badge--unknown .badge__value {
		font-family: var(--font-mono);
	}
</style>
