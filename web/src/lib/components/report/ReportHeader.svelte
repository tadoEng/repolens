<script lang="ts">
	/*
	 * Report identity: what was analyzed, at which commit, by which analyzer and ruleset.
	 *
	 * Analyzer and ruleset versions are **first-class, not a footnote**. A finding only
	 * means something in the context of the rules that produced it, so a report that hides
	 * its ruleset version is a report whose conclusions cannot be reproduced or compared.
	 *
	 * `tree_sha` is here for the same reason: two commits that share a tree yield identical
	 * evidence, so it is part of the reproducibility key rather than trivia.
	 *
	 * A `<dl>` because these are name/value pairs, and pairing them in the markup is what
	 * lets a screen reader read "Analyzer version, 0.1.0" instead of two loose strings.
	 */
	import CopyableSha from '$lib/components/primitives/CopyableSha.svelte';
	import ExternalCommitLink from '$lib/components/primitives/ExternalCommitLink.svelte';
	import { timestamp } from '$lib/contract/format';
	import type { Report } from '@repolens/api-client';

	interface Props {
		report: Report;
	}

	let { report }: Props = $props();
</script>

<div class="report-header">
	<p class="report-header__repository">
		{report.repository.owner}/{report.repository.name}
	</p>

	<dl class="report-header__facts">
		<div class="report-header__fact">
			<dt>Commit</dt>
			<dd>
				<CopyableSha value={report.commit_sha} label="analyzed commit SHA" />
				<ExternalCommitLink
					owner={report.repository.owner}
					name={report.repository.name}
					commitSha={report.commit_sha}
				/>
			</dd>
		</div>

		<div class="report-header__fact">
			<dt>Tree</dt>
			<dd><CopyableSha value={report.tree_sha} label="analyzed tree SHA" /></dd>
		</div>

		<div class="report-header__fact">
			<dt>Analyzer</dt>
			<dd>version {report.analyzer_version}</dd>
		</div>

		<div class="report-header__fact">
			<dt>Ruleset</dt>
			<dd>version {report.ruleset_version}</dd>
		</div>

		<div class="report-header__fact">
			<dt>Completed</dt>
			<!-- `datetime` keeps the machine-readable instant next to the localised one. -->
			<dd><time datetime={report.completed_at}>{timestamp(report.completed_at)}</time></dd>
		</div>
	</dl>
</div>

<style>
	.report-header {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		margin-block-start: var(--space-4);
	}

	.report-header__repository {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--font-size-lg);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.report-header__facts {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: var(--space-3) var(--space-6);
		margin: 0;
	}

	.report-header__fact {
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
</style>
