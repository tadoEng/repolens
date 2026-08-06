<script lang="ts">
	/*
	 * Native `<details>` / `<summary>`.
	 *
	 * Not a Bits UI Collapsible. The rule is *Bits for non-trivial interaction, native for
	 * everything else*, and for a plain disclosure native is genuinely less code and
	 * correct by default: keyboard operation, `aria-expanded`, and the open/closed state
	 * all come from the platform rather than from our implementation of it. Bits earns its
	 * place when a disclosure needs an animated height, which evidence does not.
	 *
	 * **The known defect this element carries: `<details>` breaks Ctrl+F.** Content inside
	 * a closed disclosure is not found by find-in-page in several engines, and people
	 * *will* search an evidence report for a file path. Two mitigations, both applied by
	 * callers rather than here:
	 *
	 *   - `open` is bindable, so a section can ship an "Expand all evidence" control that
	 *     makes every excerpt findable — which also serves printing and sharing.
	 *   - engines that support find-in-page expansion of `<details>` handle the rest, and
	 *     they can only do that if the disclosure is a real `<details>`. Re-implementing it
	 *     with a div and `aria-expanded` would remove that capability permanently.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		/** The always-visible label. Plain text: a summary is a control, not a layout. */
		summary: string;
		/** Secondary text alongside the summary, e.g. a count. Optional. */
		hint?: string;
		/**
		 * Current state. Supplying `onOpenChange` alongside makes the disclosure fully
		 * controlled, which is what lets a section expand every one of them at once. Omit
		 * both and the element behaves like plain HTML.
		 */
		open?: boolean;
		onOpenChange?: (open: boolean) => void;
		children: Snippet;
	}

	let { summary, hint, open = false, onOpenChange, children }: Props = $props();
</script>

<details
	class="disclosure"
	{open}
	ontoggle={(event) => onOpenChange?.((event.currentTarget as HTMLDetailsElement).open)}
>
	<summary class="disclosure__summary">
		<span class="disclosure__label">{summary}</span>
		{#if hint}<span class="disclosure__hint">{hint}</span>{/if}
	</summary>
	<div class="disclosure__body">
		{@render children()}
	</div>
</details>

<style>
	.disclosure {
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background-color: var(--surface-0);
	}

	.disclosure__summary {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		font-size: var(--font-size-sm);
		font-weight: var(--font-weight-medium);
		color: var(--text-secondary);
	}

	.disclosure__summary:hover {
		color: var(--text-primary);
	}

	.disclosure__hint {
		font-weight: var(--font-weight-regular);
		color: var(--text-muted);
	}

	.disclosure__body {
		padding: var(--space-3);
		border-block-start: var(--border-width) solid var(--border-subtle);
	}
</style>
