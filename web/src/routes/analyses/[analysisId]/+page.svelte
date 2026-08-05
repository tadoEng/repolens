<script lang="ts">
	/*
	 * Durable progress for one analysis.
	 *
	 * Readable without signing in: the analysis ID is a UUIDv7 with 74 random bits, so
	 * holding the URL *is* the capability. That is what makes an in-flight analysis
	 * shareable, and it is why nothing on this page is gated.
	 *
	 * Three contract details do the heavy lifting here, and each one is a defect avoided:
	 *
	 *   - `commit_sha` is null during QUEUED and RESOLVING. It renders "resolving…", never
	 *     a blank space that reads as broken markup.
	 *   - `poll_after_ms` is server-supplied and absent in terminal states. Polling follows
	 *     it rather than a hardcoded interval, so the server can widen the gap as an
	 *     analysis ages instead of paying for a frontend constant multiplied by every open
	 *     tab.
	 *   - `retry.allowed` decides the retry button. Never the state name.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';

	import { fetchAnalysis, requestRetry, type Analysis } from '$lib/api/analysis';
	import AnalysisIdentityHeader from '$lib/components/analysis/AnalysisIdentityHeader.svelte';
	import FailureNotice from '$lib/components/analysis/FailureNotice.svelte';
	import ProgressTimeline from '$lib/components/analysis/ProgressTimeline.svelte';
	import { errorCode, isFailure, isTerminal } from '$lib/contract/enums';

	const analysisId = $derived(page.params.analysisId ?? '');

	type LoadState =
		| { kind: 'loading' }
		| { kind: 'loaded'; analysis: Analysis }
		| { kind: 'missing' }
		| { kind: 'rejected'; status: number; code: string | null; message: string | null }
		| { kind: 'unreachable' };

	let load = $state<LoadState>({ kind: 'loading' });
	let retrying = $state(false);
	let retryProblem = $state<string | null>(null);

	/** Bumped after a successful retry, to restart the fetch-and-poll cycle. */
	let reloadToken = $state(0);

	$effect(() => {
		const id = analysisId;
		// Read so a retry restarts polling; the effect otherwise depends only on the route.
		void reloadToken;
		if (!id) return;

		let cancelled = false;
		let timer: ReturnType<typeof setTimeout> | undefined;

		async function poll(): Promise<void> {
			const result = await fetchAnalysis(id);
			if (cancelled) return;

			if (result.kind === 'unreachable') {
				load = { kind: 'unreachable' };
				return;
			}

			if (result.kind === 'rejected') {
				load =
					result.status === 404
						? { kind: 'missing' }
						: {
								kind: 'rejected',
								status: result.status,
								code: result.error?.code ?? null,
								message: result.error?.message ?? null
							};
				return;
			}

			load = { kind: 'loaded', analysis: result.value };

			/*
			 * `poll_after_ms` is absent in terminal states — there is nothing left to poll
			 * for — so its presence, not a state check, is what keeps the loop alive. Both
			 * are honoured: a terminal state with a stale interval still stops.
			 */
			const wait = result.value.poll_after_ms;
			if (!isTerminal(result.value.state) && typeof wait === 'number' && wait > 0) {
				timer = setTimeout(() => void poll(), wait);
			}
		}

		load = { kind: 'loading' };
		void poll();

		return () => {
			cancelled = true;
			if (timer !== undefined) clearTimeout(timer);
		};
	});

	async function retry(): Promise<void> {
		retrying = true;
		retryProblem = null;

		const result = await requestRetry(analysisId);
		retrying = false;

		if (result.kind === 'ok') {
			load = { kind: 'loaded', analysis: result.value };
			// Resume polling against the restarted analysis.
			reloadToken += 1;
			return;
		}

		retryProblem =
			result.kind === 'unreachable'
				? 'The retry request never reached the server. Nothing was started.'
				: (result.error?.message ?? `The server refused the retry with status ${result.status}.`);
	}
</script>

<svelte:head>
	<title>Analysis progress · RepoLens</title>
</svelte:head>

<h1>Analysis progress</h1>

{#if load.kind === 'loading'}
	<p class="progress__status" role="status">Loading this analysis…</p>
	<p class="progress__placeholder">Analysis <code>{analysisId}</code></p>
{:else if load.kind === 'missing'}
	<div class="progress__problem">
		<h2>No analysis at this address</h2>
		<p>
			Nothing was found for <code>{analysisId}</code>. An analysis ID is unguessable, so a mistyped
			or truncated one does not match anything.
		</p>
	</div>
{:else if load.kind === 'unreachable'}
	<div class="progress__problem">
		<h2>The API could not be reached</h2>
		<p>
			The request never reached a server, so this says nothing about the analysis itself — it may
			well be running. This is a transport or configuration failure.
		</p>
	</div>
{:else if load.kind === 'rejected'}
	<div class="progress__problem">
		<h2>This analysis could not be loaded</h2>
		<p>
			The API answered with status {load.status}.
			{#if load.message}{load.message}{/if}
		</p>
		{#if load.code}
			<p class="progress__code">{errorCode(load.code).label} · <code>{load.code}</code></p>
		{/if}
	</div>
{:else}
	<AnalysisIdentityHeader analysis={load.analysis} />

	<section class="progress__section" aria-labelledby="pipeline">
		<h2 id="pipeline" tabindex="-1">Pipeline</h2>
		<ProgressTimeline state={load.analysis.state} />
	</section>

	{#if isFailure(load.analysis.state)}
		<section class="progress__section" aria-labelledby="failure">
			<h2 id="failure" tabindex="-1">This analysis failed</h2>
			<FailureNotice
				error={load.analysis.error}
				retry={load.analysis.retry}
				onRetry={retry}
				busy={retrying}
				problem={retryProblem}
			/>
		</section>
	{/if}

	{#if load.analysis.report_available}
		<!--
			`report_available` rather than `state === 'COMPLETED'`: report availability and
			analysis completion are separate facts once reports are retained, pruned, or
			regenerated under a newer ruleset.
		-->
		<section class="progress__section" aria-labelledby="report-ready">
			<h2 id="report-ready" tabindex="-1">The report is ready</h2>
			<p>
				<a href={resolve('/reports/[analysisId]', { analysisId })}>
					Open the report for this analysis
				</a>
			</p>
		</section>
	{/if}
{/if}

<style>
	.progress__status,
	.progress__placeholder {
		margin-block-start: var(--space-4);
		color: var(--text-secondary);
	}

	.progress__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin-block-start: var(--space-12);
	}

	.progress__section h2 {
		margin: 0;
		font-size: var(--font-size-2xl);
		scroll-margin-block-start: var(--space-8);
	}

	.progress__problem {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin-block-start: var(--space-8);
		padding: var(--space-6);
		border: var(--border-width) solid var(--border-strong);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.progress__problem h2 {
		margin: 0;
		font-size: var(--font-size-xl);
	}

	.progress__problem p {
		margin: 0;
	}

	.progress__code {
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}
</style>
