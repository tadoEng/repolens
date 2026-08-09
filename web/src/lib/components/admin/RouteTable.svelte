<script lang="ts">
	/*
	 * The route table — probably the clearest single answer to "how much of a request is
	 * Axum?", which is the question this whole dashboard exists to serve.
	 *
	 * ## It renders what the server bounded, and derives nothing
	 *
	 * Every row is one label the server already normalised: a matched router pattern, or one
	 * of the two fixed strings that are not patterns. This component does not group, merge,
	 * re-key, or sort. Cardinality is a memory bound in the process that produced these
	 * figures, and a UI that invented a grouping would be inventing a label set the server
	 * deliberately closed — and would break the one property that keeps an analysis id out
	 * of a map key.
	 *
	 * ## Percentiles are interpolations, and the table says so
	 *
	 * Each percentile is a linear interpolation inside one fixed bucket, so it is an estimate
	 * with a known resolution rather than a measurement. The caption states that once, for
	 * the whole table, rather than repeating six bounds per row into a table nobody could
	 * read.
	 *
	 * The one case where the distinction changes what the number *means* is rendered
	 * inline: past the last bucket bound the histogram knows only that an observation was
	 * slower, so the contract sends a null upper bound and the figure is a **floor**.
	 * Printing `10.00 s` there would state a measurement; `≥ 10.00 s` states what was
	 * observed. That is the difference between "requests take ten seconds" and "some request
	 * took at least ten seconds", and a cold start crossing that boundary is exactly the
	 * observation issue #37 wants separated from handler cost.
	 */
	import ScrollRegion from '$lib/components/primitives/ScrollRegion.svelte';
	import { integer, micros } from '$lib/contract/format';
	import { describeHttpMethodClass } from '@repolens/api-client';
	import type { LatencyPercentile, RouteOverview } from '@repolens/api-client';

	interface Props {
		routes: readonly RouteOverview[];
	}

	let { routes }: Props = $props();

	/**
	 * One percentile, with the overflow bucket rendered as the floor it is.
	 *
	 * `≥` rather than `>`: the last bound is itself in the bucket below, so an observation
	 * landing here was at least that slow, not strictly slower.
	 */
	function percentile(estimate: LatencyPercentile): string {
		return estimate.upper_bound_micros === null
			? `≥ ${micros(estimate.lower_bound_micros)}`
			: micros(estimate.micros);
	}
</script>

<ScrollRegion label="Route table">
	<table class="routes">
		<!--
			The table's accessible name, and only that. The explanation lives outside the
			scroll region below.

			A visible `<caption>` was the first attempt and the 360px baseline showed why it
			cannot be: a caption is laid out at the *table's* width, so inside a horizontally
			scrolling box it is clipped along with the columns. At 360px the sentence that
			stops an interpolated estimate being read as a measurement was only reachable by
			scrolling a nine-column table sideways — which is to say, not reachable. Hidden
			here, present in full below, and readable at every width.
		-->
		<caption class="visually-hidden">Requests by matched route and method</caption>
		<thead>
			<tr>
				<th scope="col">Route</th>
				<th scope="col">Method</th>
				<th scope="col" class="routes__number">Requests</th>
				<th scope="col" class="routes__number">p50</th>
				<th scope="col" class="routes__number">p95</th>
				<th scope="col" class="routes__number">p99</th>
				<th scope="col" class="routes__number">2xx</th>
				<th scope="col" class="routes__number">4xx</th>
				<th scope="col" class="routes__number">5xx</th>
			</tr>
		</thead>
		<tbody>
			{#each routes as route (`${route.route} ${route.method}`)}
				<tr>
					<!--
						`scope="row"` on the route rather than a plain cell: the pattern is what
						identifies the row, and a screen reader reading a lone `24 ms` out of
						context is reading a number with no subject.
					-->
					<th scope="row" class="routes__label"><code>{route.route}</code></th>
					<!--
						Through the generated label map, so a method class the contract gains
						later renders as a named unknown rather than as a raw token — and fails
						`pnpm -r check` before that, which is the gate that matters.
					-->
					<td>{describeHttpMethodClass(route.method).label}</td>
					<td class="routes__number">{integer(route.requests)}</td>
					<td class="routes__number">{percentile(route.latency.p50)}</td>
					<td class="routes__number">{percentile(route.latency.p95)}</td>
					<td class="routes__number">{percentile(route.latency.p99)}</td>
					<td class="routes__number">{integer(route.responses.success)}</td>
					<td class="routes__number">{integer(route.responses.client_error)}</td>
					<td class="routes__number">{integer(route.responses.server_error)}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</ScrollRegion>

<p class="routes__note">
	Requests served by this process, by matched route and method. Latency figures are estimated by
	interpolation within fixed histogram buckets, so they carry the resolution of those buckets rather
	than of individual requests. A figure shown as <code>≥</code> is a floor: the request was slower than
	the largest bucket, and how much slower was not recorded.
</p>

<style>
	.routes {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--font-size-sm);
	}

	.routes__note {
		/*
		 * Outside the scroll region, so it wraps to the viewport rather than to the table.
		 * This is the sentence that stops an interpolated estimate being read as a
		 * measurement; it has to be readable at 360px without touching the table.
		 */
		margin: var(--space-3) 0 0;
		max-width: var(--measure);
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.routes th,
	.routes td {
		padding: var(--space-2) var(--space-3);
		border-block-end: var(--border-width) solid var(--border-subtle);
		text-align: start;
		white-space: nowrap;
	}

	.routes thead th {
		color: var(--text-secondary);
		font-weight: var(--font-weight-medium);
	}

	.routes__label {
		font-weight: var(--font-weight-regular);
	}

	.routes__number {
		text-align: end;
		font-family: var(--font-mono);
		font-variant-numeric: tabular-nums;
	}
</style>
