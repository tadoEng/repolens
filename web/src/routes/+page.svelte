<script lang="ts">
	/*
	 * The one action this product has: submit a public GitHub repository URL.
	 *
	 * This route was a documented placeholder until now, blocked on two things that have
	 * since landed — the Firebase gate (#13) and `POST /api/v1/analyses` in the generated
	 * contract (#6). Neither the request shape nor the credential is invented here: the body
	 * is built by `createAnalysis`, which is typed against the OpenAPI document, and the ID
	 * token is attached by the same function.
	 *
	 * ## The route owns the transport, the components own the pixels
	 *
	 * `AuthGate` and `SubmitErrorSummary` take props and hand back callbacks. That is the
	 * standing rule in `components/README.md` and it pays for itself here: every state worth
	 * asserting — a session that has not resolved, a deployment with no sign-in at all, a
	 * refusal from the API — is reachable from a literal rather than from a network
	 * condition somebody has to reproduce.
	 *
	 * ## Nothing is decided locally that the server decides
	 *
	 * The submit control is disabled while nobody is signed in, and that is an affordance,
	 * not a security boundary — `POST /api/v1/analyses` verifies the token and refuses
	 * without it. So a submission is never short-circuited into a locally invented error:
	 * whatever the API says (`UNAUTHENTICATED`, `AUTHENTICATION_UNAVAILABLE`,
	 * `INVALID_REPOSITORY_URL`) is what the reader is shown, and there is exactly one
	 * authority for the rule.
	 *
	 * The same reasoning keeps URL validation thin. `type="url"` and `required` are the
	 * browser's own checks and cost nothing; deciding here what a *repository* URL looks like
	 * would be a second, weaker copy of a rule the API already enforces, and it would fail by
	 * rejecting inputs the server would have accepted.
	 */
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	import { createAnalysis, type ApiError } from '$lib/api/analysis';
	import { session } from '$lib/auth/session.svelte';
	import Button from '$lib/components/primitives/Button.svelte';
	import AuthGate from '$lib/components/submit/AuthGate.svelte';
	import SubmitErrorSummary from '$lib/components/submit/SubmitErrorSummary.svelte';

	/** The two failing arms of `Fetched<Analysis>`; the third one navigates away. */
	type SubmitOutcome =
		{ kind: 'rejected'; status: number; error: ApiError | null } | { kind: 'unreachable' };

	let repositoryUrl = $state('');
	let submitting = $state(false);
	let outcome = $state<SubmitOutcome | null>(null);

	/*
	 * `session.busy` is included deliberately. A sign-in popup is open at that moment, so
	 * the token that would be attached is the one from *before* it — submitting through it
	 * is how a user who is mid-sign-in gets an `UNAUTHENTICATED` refusal they cannot explain.
	 */
	const canSubmit = $derived(session.state.status === 'signed-in' && !submitting && !session.busy);

	/*
	 * Restore an existing session on mount.
	 *
	 * An `$effect` with no reactive reads, so it runs once after the component is in the
	 * DOM, and only in the browser — `ssr` is off for every route. `initialize()` is
	 * idempotent and returns immediately when the build has no Firebase configuration, which
	 * is what keeps a read-only deployment from paying to load the SDK.
	 */
	$effect(() => {
		void session.initialize();
	});

	async function submit(event: SubmitEvent): Promise<void> {
		// No `action`, so the default submission would reload the page and lose the state.
		event.preventDefault();
		if (!canSubmit) return;

		submitting = true;
		outcome = null;

		try {
			const result = await createAnalysis(repositoryUrl.trim());

			if (result.kind === 'ok') {
				// The created analysis is durable and readable anonymously, so the progress page
				// is the destination — and its URL is the thing worth sharing.
				await goto(resolve('/analyses/[analysisId]', { analysisId: result.value.id }));
				return;
			}

			outcome = result;
		} finally {
			submitting = false;
		}
	}
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

<section class="submit" aria-labelledby="submit-heading">
	<h2 id="submit-heading">Analyze a repository</h2>

	<AuthGate
		state={session.state}
		busy={session.busy}
		error={session.error}
		onSignIn={() => void session.signIn()}
		onSignOut={() => void session.signOut()}
	/>

	<form class="submit__form" onsubmit={submit}>
		<div class="submit__field">
			<label class="submit__label" for="repository-url">Public GitHub repository URL</label>
			<input
				id="repository-url"
				class="submit__input"
				type="url"
				name="repository_url"
				required
				autocomplete="off"
				spellcheck="false"
				inputmode="url"
				aria-describedby="repository-url-hint"
				bind:value={repositoryUrl}
			/>
			<p id="repository-url-hint" class="submit__hint">
				One public repository, analyzed at the exact commit its default branch points to when the
				analysis starts. For example <code>https://github.com/rust-lang/crates.io</code>.
			</p>
		</div>

		<Button type="submit" variant="primary" disabled={!canSubmit} busy={submitting}>
			{submitting ? 'Starting analysis…' : 'Start analysis'}
		</Button>
	</form>

	{#if outcome}
		<SubmitErrorSummary {outcome} />
	{/if}
</section>

<style>
	.lead {
		font-size: var(--font-size-lg);
		color: var(--text-secondary);
		margin-block-start: var(--space-4);
	}

	.submit {
		max-inline-size: var(--measure);
		margin-block-start: var(--space-12);
	}

	.submit h2 {
		font-size: var(--font-size-2xl);
		margin-block-end: var(--space-6);
	}

	.submit__form {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-6);
		margin-block-start: var(--space-6);
		/* The form is the widest thing here; without this the input can outrun 360px. */
		inline-size: 100%;
	}

	.submit__field {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		inline-size: 100%;
	}

	.submit__label {
		font-weight: var(--font-weight-medium);
	}

	.submit__input {
		inline-size: 100%;
		padding: var(--space-2) var(--space-3);
		border: var(--border-width) solid var(--border-strong);
		border-radius: var(--radius-sm);
		background-color: var(--surface-0);
		color: var(--text-primary);
		/* Machine-produced string, copied verbatim — the same rule the report follows. */
		font-family: var(--font-mono);
		font-size: var(--font-size-sm);
		line-height: var(--line-height-mono);
	}

	.submit__hint {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}
</style>
