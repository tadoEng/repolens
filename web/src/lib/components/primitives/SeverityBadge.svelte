<script lang="ts">
	/*
	 * Severity: **impact if the finding is valid.**
	 *
	 * A separate component from `ConfidenceBadge` on purpose, and they must never be
	 * merged. Severity and confidence are orthogonal: `HIGH` severity with `LOW`
	 * confidence is a guess about something that would matter, and a single combined
	 * badge would let that guess read as a certainty. The contract keeps them as two
	 * types; the UI keeps them as two badges.
	 *
	 * The word "Severity" is rendered, not implied. Two bare badges reading "High" and
	 * "Low" next to each other tell a reader nothing about which axis is which.
	 *
	 * No new hues. tokens.css has five status hues and they belong to `FindingState`;
	 * borrowing one here would make `HIGH` severity look like a *state*. The scale is
	 * carried by weight and border strength instead — and, decisively, by the word.
	 */
	import { severity } from '$lib/contract/enums';

	interface Props {
		/** Raw wire value. `string`, because that is what a response contains. */
		value: string | null | undefined;
	}

	let { value }: Props = $props();

	const display = $derived(severity(value));
</script>

<span class="badge badge--{display.token}" data-severity={display.raw}>
	<span class="badge__axis">Severity</span>
	<span class="badge__value">{display.label}</span>
</span>

<style>
	.badge {
		display: inline-flex;
		align-items: baseline;
		gap: var(--space-1);
		/* Square corners. ConfidenceBadge is a pill: a second, non-colour difference
		 * between the two axes, readable at a glance and in greyscale. */
		border-radius: var(--radius-sm);
		border: var(--border-width) solid var(--border-subtle);
		background-color: var(--surface-1);
		padding: 0.1em var(--space-2);
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

	.badge--medium,
	.badge--high {
		border-color: var(--border-strong);
	}

	.badge--medium .badge__value,
	.badge--high .badge__value {
		color: var(--text-primary);
	}

	.badge--high .badge__value {
		font-weight: var(--font-weight-bold);
	}

	.badge--unknown .badge__value {
		font-family: var(--font-mono);
	}
</style>
