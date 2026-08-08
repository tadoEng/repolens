<script lang="ts">
	/*
	 * A bar chart that is a table.
	 *
	 * **Accessibility here is structural, not additive.** The alternative — an SVG chart
	 * plus an `aria-label` describing it, or a visually-hidden duplicate table — has two
	 * sources of truth that drift the first time someone changes one of them. A real
	 * `<table>` with the bar drawn as a cell background is screen-reader correct by
	 * construction, sorts and copies like data, prints, and degrades to a plain table with
	 * CSS off.
	 *
	 * **The bar is a background layer, never a box that sizes the text.** A `<span>` sized
	 * by `--proportion` that *contains* the number gives a language at 0.4% of the codebase
	 * a bar narrower than its own label, so the label clips or overflows. Here the number
	 * sits in normal flow inside the `<td>` and the bar is a `::before` behind it: the text
	 * is fully legible at any proportion, and the cell keeps its real numeric content.
	 *
	 * **The bar occupies its own lane under the number rather than sitting behind it.** A
	 * fill that ends inside the digits cuts them in half — `10,|670` at 14% — which is
	 * how a bar drawn as a cell background looks at every proportion that is not close to
	 * 0 or 1. Moving it to a rule beneath the value costs nothing a reader wanted and is
	 * the difference between a highlighted cell and a chart.
	 *
	 * **The track is the other half of the reading.** A bar states a part; it can only be
	 * read as a proportion against a visible whole, so the unfilled rail is drawn rather
	 * than implied. Both come from `--chart-*`, which is where the measured contrast
	 * behind those two steps is written down.
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

	.metric-table caption {
		/* Air between the sentence that names the view and the data it names. */
		padding-block-end: var(--space-3);
	}

	/* The header row anchors the columns; every other rule in the table is subtle, so
	 * this one has to be the firm one or the numbers float free of their names. */
	.metric-table thead th {
		border-block-end: var(--border-width) solid var(--border-strong);
	}

	.metric-table tbody tr:last-child :is(th, td) {
		border-block-end: none;
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
	 * The bar. `position: relative` on the cell, both layers absolutely positioned behind
	 * the content, the fill sized by the custom property. The `<td>`'s own width is decided
	 * by its text, so a 0.4% row is exactly as legible as a 61% one.
	 */
	.metric-cell {
		position: relative;
		isolation: isolate;
	}

	/*
	 * A percentage of the cell's width is a percentage of a column whose width the widest
	 * number decided — roughly six characters, which is not a track anyone can read a
	 * proportion off. Claiming the column outright is what turns the bars into a chart.
	 */
	.metric-table--bars .metric-cell {
		inline-size: 52%;
		padding-block-end: calc(var(--space-2) + var(--chart-bar-size) + var(--space-1));
	}

	.metric-table--bars .metric-cell::before,
	.metric-table--bars .metric-cell::after {
		content: '';
		position: absolute;
		inset-block-end: var(--space-2);
		/* Bars grow from the number's side, so they read against the value they measure. */
		inset-inline-end: var(--space-3);
		block-size: var(--chart-bar-size);
		/* Rounded at the data end, square at the baseline — for the track as well as the
		 * fill, so the two share one edge instead of the fill's square corner sitting
		 * proud of the track's rounded one. */
		border-start-start-radius: var(--chart-bar-size);
		border-end-start-radius: var(--chart-bar-size);
	}

	/* The whole: every row's rail is the same length, which is what makes the fills
	 * comparable down the column. */
	.metric-table--bars .metric-cell::after {
		inset-inline-start: var(--space-3);
		background-color: var(--chart-track);
		z-index: -2;
	}

	/* The part. Square where it meets the baseline, rounded at the data end. */
	.metric-table--bars .metric-cell::before {
		inline-size: calc(var(--proportion, 0) * (100% - 2 * var(--space-3)));
		background-color: var(--chart-fill);
		z-index: -1;
	}

	.metric-cell > span {
		position: relative;
		z-index: 1;
	}

	/*
	 * Windows High Contrast strips background colours wholesale, so the bar would vanish
	 * regardless. Dropping it explicitly is the difference between degrading and breaking:
	 * the number is right there in the same cell, so nothing is lost. The track goes with
	 * it — a rail with no fill in it would state that every row is at zero.
	 */
	@media (forced-colors: active) {
		.metric-table--bars .metric-cell::before,
		.metric-table--bars .metric-cell::after {
			display: none;
		}
	}
</style>
