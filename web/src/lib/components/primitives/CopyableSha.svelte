<script lang="ts">
	/*
	 * A commit SHA or content digest: shown short, copied in full.
	 *
	 * The exact analyzed commit is the single most load-bearing value in a RepoLens
	 * report — it is what makes the report checkable — so it is always visible, always
	 * mono, and always copyable verbatim. Seven characters is what a reader recognises;
	 * forty is what they need to paste. Both, rather than either.
	 *
	 * `title` carries the full value for a pointer user, but a `title` is not an
	 * accessible substitute: it does not exist on touch and is not reliably announced.
	 * So the full value is also in the button's accessible name, which is where a screen
	 * reader user gets it without having a forty-character hex string read into the
	 * middle of a sentence.
	 */
	import { shortDigest } from '$lib/contract/format';

	interface Props {
		/** The full value. Never truncated before it reaches this component. */
		value: string;
		/** What this identifies, e.g. `commit SHA`. Used in the copy button's name. */
		label?: string;
	}

	let { value, label = 'commit SHA' }: Props = $props();

	const short = $derived(shortDigest(value));

	/*
	 * Three outcomes, not two. "Copied" and "could not copy" are different facts, and a
	 * button that silently does nothing on an insecure origin or a denied permission is
	 * the worst of the three.
	 */
	let status = $state<'idle' | 'copied' | 'failed'>('idle');
	let resetAt = $state(0);

	$effect(() => {
		if (status === 'idle') return;
		// `resetAt` is read so that a second copy restarts the timer rather than
		// inheriting the first one's remaining time.
		void resetAt;
		const timer = setTimeout(() => {
			status = 'idle';
		}, 4000);
		return () => clearTimeout(timer);
	});

	async function copy(): Promise<void> {
		try {
			await navigator.clipboard.writeText(value);
			status = 'copied';
		} catch {
			// Insecure context, denied permission, or no Clipboard API. The value is
			// still selectable on screen, which the message says.
			status = 'failed';
		}
		resetAt = Date.now();
	}
</script>

<span class="sha">
	<code class="sha__value" title={value}>{short}</code>
	<button type="button" class="sha__copy" onclick={copy}>
		Copy
		<span class="visually-hidden">the full {label}, {value}</span>
	</button>
	<!--
		`role="status"` is an implicit aria-live="polite" region. It is rendered empty and
		filled on demand, which is the shape a live region has to have: a region created at
		the moment of the announcement is frequently missed by assistive technology.
	-->
	<span class="sha__status" role="status">
		{#if status === 'copied'}
			Copied
		{:else if status === 'failed'}
			Could not copy — select the value to copy it by hand
		{/if}
	</span>
</span>

<style>
	.sha {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
	}

	.sha__value {
		/* Long digests must wrap inside their container rather than widening the page. */
		overflow-wrap: anywhere;
	}

	.sha__copy {
		padding: 0.1em var(--space-2);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-sm);
		background-color: var(--surface-1);
		color: var(--text-secondary);
		font: inherit;
		font-size: var(--font-size-sm);
		cursor: pointer;
		transition: color var(--duration-fast) var(--easing-standard);
	}

	.sha__copy:hover {
		border-color: var(--border-strong);
		color: var(--text-primary);
	}

	.sha__status {
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}
</style>
