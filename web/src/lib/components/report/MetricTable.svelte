<script lang="ts">
	/*
	 * A bar chart that is a table.
	 *
	 * **Accessibility here is structural, not additive.** The alternative — an SVG chart
	 * plus an `aria-label` describing it, or a visually-hidden duplicate table — has two
	 * sources of truth that drift the first time someone changes one of them. A real
	 * `<table>` with the bar drawn as a row background is screen-reader correct by
	 * construction, sorts and copies like data, prints, and degrades to a plain table with
	 * CSS off.
	 *
	 * **The bar is a background layer, never a box that sizes the text.** A `<span>` sized
	 * by `--proportion` that *contains* the number gives a language at 0.4% of the codebase
	 * a bar narrower than its own label, so the label clips or overflows. Here the number
	 * sits in normal flow inside the `<td>` and the bar is a `::before` behind it: the text
	 * is fully legible at any proportion, and the cell keeps its real numeric content.
	 *
	 * `--proportion` is set through CSSOM by `proportionBar` rather than as an inline
	 * `style` attribute — see the note in `proportion.ts`; the CSP blocks the attribute.
	 */
	import ScrollRegion from '$lib/components/primitives/ScrollRegion.svelte';
	import { integer, percent } from '$lib/contract/format';
	import type { MetricRow } from '$lib/contract/composition';

	import { proportionBar } from './proportion';

	interface Props {
		/** Describes the table. `<caption>` rather than a heading: this labels data. */
		caption: string;
		/** Header for the label column, e.g. `Language`. */
		rowHeader: string;
		/** Header for the value column, e.g. `Lines of code`. */
		valueHeader: string;
		rows: MetricRow[];
		/**
		 * Draw the proportion bars. Off for the plain tables, on for the two bar views —
		 * the whole point of the split is that bars are earned, not default.
		 */
		bars?: boolean;
	}

	let { caption, rowHeader, valueHeader, rows, bars = true }: Props = $props();
</script>

<ScrollRegion label={caption}>
	<table class="metric-table" class:metric-table--bars={bars}>
		<caption>{caption}</caption>
		<thead>
			<tr>
				<th scope="col">{rowHeader}</th>
				<th scope="col" class="metric-table__number">{valueHeader}</th>
				<th scope="col" class="metric-table__number">Share</th>
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.label)}
				<tr>
					<th scope="row">
						{row.label}
						{#if row.derived}
							<!--
								Marked because it is arithmetic done in the browser — the total minus
								the itemised rows — rather than a figure the analyzer reported. A
								derived number that looks measured is the quiet way a report starts
								overstating what it knows.
							-->
							<span class="metric-table__derived">derived</span>
						{/if}
					</th>
					<td class="metric-cell metric-table__number" {@attach proportionBar(row.proportion)}>
						<span>{integer(row.value)}</span>
					</td>
					<td class="metric-table__number">{percent(row.proportion)}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</ScrollRegion>

<style>
	.metric-table {
		min-inline-size: 20rem;
	}

	.metric-table__number {
		text-align: end;
		font-variant-numeric: tabular-nums;
	}

	.metric-table__derived {
		display: inline-block;
		margin-inline-start: var(--space-2);
		padding: 0 var(--space-1);
		border: var(--border-width) dashed var(--border-strong);
		border-radius: var(--radius-sm);
		font-size: var(--font-size-xs);
		font-weight: var(--font-weight-regular);
		color: var(--text-muted);
	}

	/*
	 * The bar. `position: relative` on the cell, the bar absolutely positioned behind the
	 * content, sized by the custom property. The `<td>`'s own width is decided by its text,
	 * so a 0.4% row is exactly as legible as a 61% one.
	 */
	.metric-cell {
		position: relative;
		isolation: isolate;
	}

	.metric-table--bars .metric-cell::before {
		content: '';
		position: absolute;
		inset-block: var(--space-1);
		/* Bars grow from the number's side, so they read against the value they measure. */
		inset-inline-end: 0;
		inline-size: calc(var(--proportion, 0) * 100%);
		background-color: var(--surface-2);
		border-radius: var(--radius-sm);
		z-index: -1;
	}

	.metric-cell > span {
		position: relative;
		z-index: 1;
	}

	/*
	 * Windows High Contrast strips background colours wholesale, so the bar would vanish
	 * regardless. Dropping it explicitly is the difference between degrading and breaking:
	 * the number is right there in the same cell, so nothing is lost.
	 */
	@media (forced-colors: active) {
		.metric-table--bars .metric-cell::before {
			display: none;
		}
	}
</style>
