<script lang="ts">
	/*
	 * One measurement: what it is, what it was, and — when there is no figure — which kind of
	 * nothing that is.
	 *
	 * ## Two absences, and they are not the same claim
	 *
	 * A card with no value is in one of two states, and collapsing them would make the
	 * dashboard say something false about its own instrumentation:
	 *
	 *   `unavailable`      the measurement exists and this platform cannot produce it.
	 *                      Resident memory is read from `/proc`, so a Windows host has
	 *                      nothing to read. We tried; the answer is genuinely unknown.
	 *
	 *   `notInstrumented`  nothing was ever measured, by decision. CPU, deploy age,
	 *                      analyzer phases and the PostgreSQL figures were left out of
	 *                      Experimental-v1 rather than attempted.
	 *
	 * "We could not measure CPU" and "we deliberately never instrumented CPU" send a reader
	 * to entirely different places — one to a platform limitation, the other to a decision
	 * record. This is the same distinction the report contract keeps between `MISSING` and
	 * `UNABLE_TO_VERIFY`, arriving in the operational view.
	 *
	 * Neither is ever a zero. `0 MiB` would read as a measurement of a process using no
	 * memory, which is the one thing that is certainly untrue.
	 *
	 * The absence is *typed*: `value: null` does not compile without exactly one of the two
	 * reasons, so a caller cannot render an unknown by forgetting to say which kind it is.
	 *
	 * ## Why this is a `<div>` inside a `<dl>` rather than a bare `<dt>`/`<dd>` pair
	 *
	 * The parent renders a description list, which is the right semantics for name/value
	 * pairs and what `ReportHeader` already uses. A wrapper is permitted between `<dl>` and
	 * its pairs in HTML, and it is what allows each pair to be a card without the list
	 * losing its meaning to assistive technology.
	 */

	interface Base {
		/** What was measured. Rendered as the term in the parent description list. */
		label: string;
		/** A short qualifier shown beneath the value — a bound, a source, a unit note. */
		detail?: string;
	}

	/**
	 * A figure, already formatted.
	 *
	 * Formatted by the caller rather than here, because the units differ per metric and a
	 * component that guessed at them would be a second place where a microsecond could
	 * quietly become a millisecond.
	 */
	type Measured = Base & { value: string; unavailable?: never; notInstrumented?: never };

	/** Measured elsewhere, unobtainable here. */
	type Unavailable = Base & { value: null; unavailable: string; notInstrumented?: never };

	/** Never measured, on purpose. */
	type NotInstrumented = Base & { value: null; notInstrumented: string; unavailable?: never };

	let {
		label,
		value,
		detail,
		unavailable,
		notInstrumented
	}: Measured | Unavailable | NotInstrumented = $props();
</script>

<div class="card" class:card--absent={value === null}>
	<dt class="card__label">{label}</dt>
	<dd class="card__value">
		{#if unavailable}
			<span class="card__absent">Not measured</span>
			<span class="card__detail">{unavailable}</span>
		{:else if notInstrumented}
			<span class="card__absent">Not instrumented</span>
			<span class="card__detail">{notInstrumented}</span>
		{:else}
			<span class="card__number">{value}</span>
			{#if detail}
				<span class="card__detail">{detail}</span>
			{/if}
		{/if}
	</dd>
</div>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background: var(--surface-1);
	}

	/*
	 * A dashed border, not a colour change. The distinction between a figure and an absence
	 * has to survive greyscale and forced-colours mode, where a hue does not — and the
	 * status word is on screen regardless, because a border is not a label. The two kinds of
	 * absence share this treatment deliberately: the border says "no figure here", and the
	 * words say which kind of no.
	 */
	.card--absent {
		border-style: dashed;
		background: var(--surface-0);
	}

	.card__label {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.card__value {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.card__number {
		font-family: var(--font-mono);
		font-size: var(--font-size-xl);
		line-height: var(--line-height-mono);
		color: var(--text-primary);
		/* A long figure must wrap rather than widen the grid track and scroll the page. */
		overflow-wrap: anywhere;
	}

	.card__absent {
		font-size: var(--font-size-lg);
		color: var(--text-secondary);
	}

	.card__detail {
		font-size: var(--font-size-xs);
		color: var(--text-muted);
	}
</style>
