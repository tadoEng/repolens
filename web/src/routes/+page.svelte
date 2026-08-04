<script lang="ts">
	/*
	 * Route skeleton. The submit form (§3.1: RepositorySubmitForm → UrlField · ScopeNote ·
	 * AuthGate · SubmitErrorSummary) is not built here.
	 *
	 * It is gated on two things that do not exist yet: the Firebase auth gate (issue #13)
	 * and the analysis-creation request/response contract (issue #14). Building the form
	 * now would mean inventing the DTO it posts, which is precisely what the executable
	 * fixtures exist to prevent.
	 */
</script>

<svelte:head>
	<title>RepoLens</title>
	<meta
		name="description"
		content="Deterministic, evidence-backed architecture reports for a public GitHub repository at an exact commit."
	/>
</svelte:head>

<h1>RepoLens</h1>

<p class="lead">
	Analyze one public GitHub repository at an exact commit SHA and produce a deterministic,
	evidence-backed architecture report.
</p>

<section class="placeholder" aria-labelledby="route-purpose">
	<h2 id="route-purpose">Route purpose</h2>
	<p>
		This route carries a single action: submit a public repository URL. Submission is sign-in gated,
		because creating an analysis starts paid work.
	</p>
	<p>
		The form is deliberately not built yet. It is blocked on the Firebase auth gate and on the
		analysis-creation contract; the request and response shapes arrive as executable fixtures rather
		than being guessed here.
	</p>
</section>

<style>
	.lead {
		font-size: var(--font-size-lg);
		color: var(--text-secondary);
		margin-block-start: var(--space-4);
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
