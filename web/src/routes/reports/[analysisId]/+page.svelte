<script lang="ts">
	/*
	 * The report.
	 *
	 * Public and shareable: it renders signed out, on direct navigation, with nothing but
	 * the analysis ID in the URL. That is what makes a report a thing you can send someone.
	 *
	 * There is no universal score anywhere on this page, by design. The Executive Overview
	 * carries the summarization load, every finding separates state, severity and
	 * confidence, and every conclusion is traceable to evidence a reader can check against
	 * the commit.
	 */
	import { page } from '$app/state';

	import { fetchReport, type Report } from '$lib/api/analysis';
	import CompositionSection from '$lib/components/report/CompositionSection.svelte';
	import EvidenceAppendix from '$lib/components/report/EvidenceAppendix.svelte';
	import FindingsSection from '$lib/components/report/FindingsSection.svelte';
	import OverviewSection from '$lib/components/report/OverviewSection.svelte';
	import ReportHeader from '$lib/components/report/ReportHeader.svelte';
	import ReportNav from '$lib/components/report/ReportNav.svelte';
	import ReportSection from '$lib/components/report/ReportSection.svelte';
	import { REPORT_SECTIONS } from '$lib/components/report/sections';
	import { errorCode } from '$lib/contract/enums';

	const analysisId = $derived(page.params.analysisId ?? '');

	type LoadState =
		| { kind: 'loading' }
		| { kind: 'loaded'; report: Report }
		| { kind: 'missing' }
		| { kind: 'rejected'; status: number; code: string | null; message: string | null }
		| { kind: 'unreachable' };

	let load = $state<LoadState>({ kind: 'loading' });

	$effect(() => {
		const id = analysisId;
		if (!id) return;

		let cancelled = false;
		load = { kind: 'loading' };

		void (async () => {
			const result = await fetchReport(id);
			if (cancelled) return;

			if (result.kind === 'unreachable') {
				load = { kind: 'unreachable' };
				return;
			}

			if (result.kind === 'rejected') {
				// 404 is a distinct outcome, not a generic failure: "there is no report at this
				// ID" is something the reader can act on, and everything else is not.
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

			load = { kind: 'loaded', report: result.value };
		})();

		return () => {
			cancelled = true;
		};
	});

	const title = $derived(
		load.kind === 'loaded'
			? `${load.report.repository.owner}/${load.report.repository.name} · RepoLens`
			: 'Architecture report · RepoLens'
	);
</script>

<svelte:head>
	<title>{title}</title>
</svelte:head>

<h1>Architecture report</h1>

<!--
	One live region for the whole load, rendered from the start rather than created at the
	moment of the announcement — a region that appears together with its first message is
	frequently missed by assistive technology.
-->
<p class="report__status" role="status">
	{#if load.kind === 'loading'}
		Loading the report…
	{/if}
</p>

{#if load.kind === 'loading'}
	<p class="report__placeholder">
		Analysis <code>{analysisId}</code>
	</p>
{:else if load.kind === 'missing'}
	<div class="report__problem">
		<h2>No report at this address</h2>
		<p>
			Nothing was found for analysis <code>{analysisId}</code>. A report exists only once its
			analysis has completed, so this address may simply be early — the progress page for the same
			ID will say which.
		</p>
	</div>
{:else if load.kind === 'unreachable'}
	<div class="report__problem">
		<h2>The API could not be reached</h2>
		<p>
			The request never reached a server. This is a transport or configuration failure rather than a
			missing report, so the report may well exist.
		</p>
	</div>
{:else if load.kind === 'rejected'}
	<div class="report__problem">
		<h2>The report could not be loaded</h2>
		<p>
			The API answered with status {load.status}.
			{#if load.message}{load.message}{/if}
		</p>
		{#if load.code}
			<!-- The machine code verbatim: it is what a reader quotes when reporting this. -->
			<p class="report__code">
				{errorCode(load.code).label} · <code>{load.code}</code>
			</p>
		{/if}
	</div>
{:else}
	<ReportHeader report={load.report} />

	<ReportNav sections={REPORT_SECTIONS} />

	<ReportSection
		id="overview"
		title="Overview"
		lead="Evidence-backed summary statements. There is no single score: each statement carries its own confidence and points at the findings that support it."
	>
		<OverviewSection
			overview={load.report.overview}
			findings={load.report.findings}
			limitations={load.report.limitations}
		/>
	</ReportSection>

	<ReportSection
		id="findings"
		title="Findings"
		lead="Every conclusion the ruleset reached, in the order the server fixed. State, severity and confidence are three separate answers and are never merged."
	>
		<FindingsSection findings={load.report.findings} />
	</ReportSection>

	<ReportSection id="composition" title="Composition">
		<CompositionSection
			composition={load.report.composition}
			limitations={load.report.limitations}
		/>
	</ReportSection>

	<ReportSection
		id="evidence"
		title="Evidence"
		lead="Every cited path and digest in one index, so the report stays searchable and printable."
	>
		<EvidenceAppendix findings={load.report.findings} />
	</ReportSection>
{/if}

<style>
	.report__status {
		margin: 0;
		color: var(--text-secondary);
	}

	.report__placeholder {
		margin-block-start: var(--space-4);
		color: var(--text-secondary);
	}

	.report__problem {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin-block-start: var(--space-8);
		padding: var(--space-6);
		border: var(--border-width) solid var(--border-strong);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.report__problem h2 {
		margin: 0;
		font-size: var(--font-size-xl);
	}

	.report__problem p {
		margin: 0;
	}

	.report__code {
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}
</style>
