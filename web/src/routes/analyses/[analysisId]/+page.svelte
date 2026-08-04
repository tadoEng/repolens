<script lang="ts">
	import { page } from '$app/state';

	/*
	 * Route skeleton. The progress surface (§3.1: AnalysisIdentityHeader · ProgressTimeline
	 * → ProgressStep · FailureNotice · LiveRegion) is not built here.
	 *
	 * It is blocked on the `analysis-v1` fixtures (issue #14), which own the parts this
	 * screen cannot render honestly without: a nullable commit SHA during QUEUED/RESOLVING,
	 * stable machine error codes, explicit `retry: {allowed, reason?}` permission, and the
	 * polling hint. Guessing any of those here would bake a wrong assumption into the UI.
	 *
	 * The analysis ID is read from the route so that direct navigation and the Cloudflare
	 * nested-route fallback are exercised end to end by the skeleton.
	 */
	const analysisId = $derived(page.params.analysisId);
</script>

<svelte:head>
	<title>Analysis progress · RepoLens</title>
</svelte:head>

<h1>Analysis progress</h1>

<p class="identity">
	Analysis <code>{analysisId}</code>
</p>

<section class="placeholder" aria-labelledby="route-purpose">
	<h2 id="route-purpose">Route purpose</h2>
	<p>
		Durable progress for one analysis, readable without signing in. Anyone holding the unguessable
		analysis ID can watch it, which is what makes an in-flight analysis shareable.
	</p>
	<p>
		This screen must distinguish a retriable failure from a permanent one, and must never infer
		retry permission from a state name. That distinction is part of the API contract, so the
		timeline stays unbuilt until the contract exists.
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
