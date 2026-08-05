<script lang="ts">
	import { page } from '$app/state';

	/*
	 * Route skeleton. The report (§3.1: ReportHeader · ReportNav · Overview · Technology ·
	 * Architecture · Composition · EngineeringSystem · Maintenance · Findings ·
	 * EvidenceAppendix) is not built here.
	 *
	 * It is blocked on the `report-v1` fixtures (issue #14). Everything this screen renders
	 * is evidence, and evidence has a shape: deterministic finding order, bounded excerpts
	 * with `truncated` and `digest`, report-level `limitations[]`, a nullable
	 * `LineCountSummary`, structured LOC exclusions, and `analyzer_version` /
	 * `ruleset_version`. None of that is invented in the frontend.
	 */
	const analysisId = $derived(page.params.analysisId);
</script>

<svelte:head>
	<title>Architecture report · RepoLens</title>
</svelte:head>

<h1>Architecture report</h1>

<p class="identity">
	Analysis <code>{analysisId}</code>
</p>

<section class="placeholder" aria-labelledby="route-purpose">
	<h2 id="route-purpose">Route purpose</h2>
	<p>
		The complete, evidence-backed report for one repository at one commit. Public and shareable: it
		must render correctly signed out, including on direct navigation.
	</p>
	<p>
		There is no universal score. The Executive Overview carries the whole summarization load, and
		every finding is separable into severity and confidence, each backed by evidence a reader can
		check against the commit.
	</p>
</section>

<style>
	.identity {
		margin-block-start: var(--space-4);
		color: var(--text-secondary);
	}

	.placeholder {
		margin-block-start: var(--space-12);
		padding: var(--space-6);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.placeholder h2 {
		font-size: var(--font-size-xl);
		margin-block-end: var(--space-4);
	}
</style>
