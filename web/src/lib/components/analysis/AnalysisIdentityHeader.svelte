<script lang="ts">
	/*
	 * Who is being analyzed, and at which commit — as far as that is known yet.
	 *
	 * **`commit_sha` is null during `QUEUED` and `RESOLVING`, and that renders as
	 * "resolving…", never as a blank.** The contract makes the field required-but-nullable
	 * precisely so a consumer cannot forget this case: the field is always present, its
	 * value is not. An empty space where a SHA belongs looks like a rendering bug, and on
	 * the one screen whose job is to say what is happening, that is the worst possible
	 * misreading.
	 *
	 * `repository` is available from the moment an analysis is created, which is what lets
	 * `owner/name` paint immediately instead of waiting on the resolve.
	 */
	import CopyableSha from '$lib/components/primitives/CopyableSha.svelte';
	import ExternalCommitLink from '$lib/components/primitives/ExternalCommitLink.svelte';
	import { triggerStatus } from '$lib/contract/enums';
	import { timestamp } from '$lib/contract/format';
	import type { Analysis } from '@repolens/api-client';

	interface Props {
		analysis: Analysis;
	}

	let { analysis }: Props = $props();

	const trigger = $derived(triggerStatus(analysis.execution.trigger_status));
</script>

<div class="identity">
	<p class="identity__repository">
		{analysis.repository.owner}/{analysis.repository.name}
	</p>

	<dl class="identity__facts">
		<div class="identity__fact">
			<dt>Commit</dt>
			<dd>
				{#if analysis.commit_sha}
					<CopyableSha value={analysis.commit_sha} label="analyzed commit SHA" />
					<ExternalCommitLink
						owner={analysis.repository.owner}
						name={analysis.repository.name}
						commitSha={analysis.commit_sha}
					/>
				{:else}
					<!-- Not blank. There genuinely is no commit yet, and saying so is the point. -->
					<span class="identity__pending">resolving…</span>
				{/if}
			</dd>
		</div>

		<div class="identity__fact">
			<dt>Started</dt>
			<dd><time datetime={analysis.created_at}>{timestamp(analysis.created_at)}</time></dd>
		</div>

		<div class="identity__fact">
			<dt>Last changed</dt>
			<!--
				Distinct from "started" on purpose: the pair is what lets a reader see
				"stuck for 20 minutes" rather than only "began 20 minutes ago".
			-->
			<dd><time datetime={analysis.updated_at}>{timestamp(analysis.updated_at)}</time></dd>
		</div>

		<div class="identity__fact">
			<dt>Scheduling</dt>
			<dd>
				<span>{trigger.label}</span>
				{#if analysis.execution.execution_id}
					<code title="Runner execution id, for correlating logs"
						>{analysis.execution.execution_id}</code
					>
				{/if}
			</dd>
		</div>
	</dl>

	{#if analysis.execution.trigger_status === 'FAILED'}
		<!--
			The outage case the contract separates `ExecutionMetadata` from `AnalysisState` to
			expose: an analysis can sit in `QUEUED` with the trigger succeeded (normal, waiting
			for a worker) or with the trigger failed (stuck, and nothing will ever claim it).
			Those look identical without this, and only one of them is fine.
		-->
		<p class="identity__stuck">
			The work was never accepted by a runner. This analysis is not waiting its turn — no worker
			will pick it up.
		</p>
	{/if}
</div>

<style>
	.identity {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin-block-start: var(--space-4);
	}

	.identity__repository {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--font-size-lg);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.identity__facts {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: var(--space-3) var(--space-6);
		margin: 0;
	}

	.identity__fact {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	dt {
		font-size: var(--font-size-xs);
		font-weight: var(--font-weight-medium);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
	}

	dd {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2) var(--space-4);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-primary);
	}

	.identity__pending {
		font-style: italic;
		color: var(--text-muted);
	}

	.identity__stuck {
		max-inline-size: var(--measure);
		margin: 0;
		padding: var(--space-3);
		border: var(--border-width) dotted var(--status-unable-to-verify-border);
		border-radius: var(--radius-md);
		background-color: var(--status-unable-to-verify-bg);
		font-size: var(--font-size-sm);
	}
</style>
