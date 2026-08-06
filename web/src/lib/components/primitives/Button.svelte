<script lang="ts">
	/*
	 * A native `<button>` with the project's two treatments.
	 *
	 * Not a Bits UI component. Bits does not ship a Button for the same reason we would not
	 * reach for one if it did: `<button>` is already keyboard-operable, already announces
	 * its disabled state, and already fires on Space and Enter.
	 *
	 * `type="button"` is hardcoded. The HTML default is `submit`, which inside a form makes
	 * every unlabelled button a submit button — the one default worth taking away.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		/** `primary` is the single main action on a screen. Everything else is `secondary`. */
		variant?: 'primary' | 'secondary';
		disabled?: boolean;
		/** Set while an action is in flight, so the label can say so and the control locks. */
		busy?: boolean;
		onclick?: (event: MouseEvent) => void;
		children: Snippet;
	}

	let {
		variant = 'secondary',
		disabled = false,
		busy = false,
		onclick,
		children
	}: Props = $props();
</script>

<button
	type="button"
	class="button button--{variant}"
	disabled={disabled || busy}
	aria-busy={busy ? 'true' : undefined}
	{onclick}
>
	{@render children()}
</button>

<style>
	.button {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-4);
		border: var(--border-width) solid var(--border-strong);
		border-radius: var(--radius-sm);
		font: inherit;
		font-weight: var(--font-weight-medium);
		line-height: var(--line-height-body);
		cursor: pointer;
		transition:
			background-color var(--duration-fast) var(--easing-standard),
			color var(--duration-fast) var(--easing-standard);
	}

	.button--primary {
		background-color: var(--accent);
		border-color: var(--accent);
		color: var(--accent-contrast);
	}

	.button--primary:hover:not(:disabled) {
		background-color: var(--accent-hover);
		border-color: var(--accent-hover);
	}

	.button--secondary {
		background-color: var(--surface-0);
		color: var(--text-primary);
	}

	.button--secondary:hover:not(:disabled) {
		background-color: var(--surface-1);
	}

	.button:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
</style>
