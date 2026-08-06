<script lang="ts">
	/*
	 * Repository composition — the section most easily misread as a score.
	 *
	 * ## Two charts and three tables, meant literally
	 *
	 * LOC is genuinely comparative, so bars earn their place **exactly twice**: code by
	 * language, and code by top-level area (the more architecturally useful of the pair —
	 * it answers frontend-heavy, test-heavy or tooling-dominated at a glance). Everything
	 * else is a table, because the payload is either three series at once, where stacked
	 * bars hide small values and read poorly aloud, or a single composition of a known
	 * whole, which a share column states exactly and a bar only approximates.
	 *
	 *   1. Code by language ............ bars   (comparing magnitudes)
	 *   2. Code by top-level area ...... bars   (comparing magnitudes)
	 *   3. Code / comments / blanks .... table  (3 series x N languages)
	 *   4. Production / test / generated table  (one composition)
	 *   5. Largest source files ........ table  (long paths, not magnitudes)
	 *
	 * Views 4 and 5 render `LineCountSummary.roles` and `.largest_files`, which the contract
	 * now carries. An earlier head substituted other data for them; that is why the count of
	 * bar-drawing views is asserted in both the browser and end-to-end suites rather than
	 * left to review.
	 *
	 * The exclusion ledger follows, **in addition to** the five views rather than in place of
	 * any of them: it is what makes the five checkable.
	 *
	 * ## Role is rendered, not just counted
	 *
	 * `largest_files` carries a `CodeRole` per row because size alone is a review-priority
	 * signal and a *generated* file at the top of the list is not the same fact as a
	 * hand-written one. Dropping the column is the most common way this particular list
	 * misleads.
	 *
	 * ## The disclaimer is in-section, not a footnote
	 *
	 * Same discipline as having no universal score. A footnote is where a caveat goes to be
	 * skipped, and LOC is the easiest number in this product to mistake for a verdict.
	 */
	import Disclosure from '$lib/components/primitives/Disclosure.svelte';
	import ScrollRegion from '$lib/components/primitives/ScrollRegion.svelte';
	import StatusChip from '$lib/components/primitives/StatusChip.svelte';
	import {
		areaCodeRows,
		exclusionLedger,
		languageCodeRows,
		overCountedBreakdowns,
		roleCoverage,
		roleRows
	} from '$lib/contract/composition';
	import { codeRole } from '$lib/contract/enums';
	import { bytes, integer, percent } from '$lib/contract/format';
	import type { Limitation, LineCountSummary } from '@repolens/api-client';

	import LimitationsList from './LimitationsList.svelte';
	import MetricTable from './MetricTable.svelte';

	interface Props {
		/**
		 * Nullable by contract. `null` is a designed state — the analysis ran and produced no
		 * counts — not an error and not a zero.
		 */
		composition: LineCountSummary | null;
		/** Report-level limitations, which are what explain a `null` composition. */
		limitations: Limitation[];
	}

	let { composition, limitations }: Props = $props();

	const ledger = $derived(composition ? exclusionLedger(composition) : null);
	const roles = $derived(composition ? roleRows(composition) : []);
	const coverage = $derived(composition ? roleCoverage(composition) : null);
	const overCounted = $derived(composition ? overCountedBreakdowns(composition) : []);
</script>

<!--
	First, before any number. A reader who stops here has still been told the one thing
	that stops this section from being read as a verdict.
-->
<p class="composition__disclaimer">
	RepoLens measures repository composition, not productivity or code quality.
</p>

{#if composition === null}
	<!--
		`null` maps onto the existing `UNABLE_TO_VERIFY` state rather than a sixth one, and
		it renders as that state rather than as zeros. Rendering `0` here would state that
		the repository contains no code, which is a claim the analysis never made.
	-->
	<div class="composition__unavailable">
		<p class="composition__unavailable-lead">
			<StatusChip state="UNABLE_TO_VERIFY" />
			<span>No line counts were produced for this analysis.</span>
		</p>
		<p>
			This is not a claim that the repository has no code. The counts are absent, and the
			report-level limitation below says why.
		</p>
		<LimitationsList {limitations} label="Why the counts are absent" />
	</div>
{:else}
	<p class="composition__provenance">
		Counted by <code>{composition.counter}</code> version {composition.counter_version}, under
		exclusion policy version {composition.exclusion_policy_version}. Different counter versions
		count differently, which is why the version is part of the result.
	</p>

	<MetricTable
		caption="Lines of code by language, as a share of all counted code lines."
		rowHeader="Language"
		valueHeader="Lines of code"
		rows={languageCodeRows(composition)}
	/>

	<MetricTable
		caption="Lines of code by top-level area, as a share of all counted code lines."
		rowHeader="Area"
		valueHeader="Lines of code"
		rows={areaCodeRows(composition)}
	/>

	<ScrollRegion label="Code, comment and blank lines per language">
		<table class="composition__table">
			<caption>
				Code, comment and blank lines per language. Three series at once: a table rather than a
				stacked bar, which would hide the small values and read poorly aloud.
			</caption>
			<thead>
				<tr>
					<th scope="col">Language</th>
					<th scope="col" class="composition__number">Files</th>
					<th scope="col" class="composition__number">Code</th>
					<th scope="col" class="composition__number">Comments</th>
					<th scope="col" class="composition__number">Blank</th>
				</tr>
			</thead>
			<tbody>
				{#each composition.languages as language (language.language)}
					<tr>
						<th scope="row">{language.language}</th>
						<td class="composition__number">{integer(language.files)}</td>
						<td class="composition__number">{integer(language.code_lines)}</td>
						<td class="composition__number">{integer(language.comment_lines)}</td>
						<td class="composition__number">{integer(language.blank_lines)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</ScrollRegion>

	<!--
		Production / test / generated. A table with a share column and no bar: the acceptance
		criterion is two bar views, and the two comparative ones have them.
	-->
	{#if roles.length === 0}
		<p class="composition__lead">
			<StatusChip state="UNABLE_TO_VERIFY" />
			<span>
				This analysis reported no breakdown by role, so what share of the repository is production
				code is not established here. That is not a claim that none of it is.
			</span>
		</p>
	{:else}
		<ScrollRegion label="Code lines by role: production, test, generated and tooling">
			<table class="composition__table">
				<caption>
					Code lines by role. Structural evidence, not a judgement: generated code is named as
					generated so it is not counted as hand-written work.
				</caption>
				<thead>
					<tr>
						<th scope="col">Role</th>
						<th scope="col" class="composition__number">Files</th>
						<th scope="col" class="composition__number">Code</th>
						<th scope="col" class="composition__number">Share</th>
					</tr>
				</thead>
				<tbody>
					{#each roles as row (row.role)}
						{@const role = codeRole(row.role)}
						<tr>
							<th scope="row" data-role={role.raw}>{role.label}</th>
							<td class="composition__number">{integer(row.files)}</td>
							<td class="composition__number">{integer(row.codeLines)}</td>
							<td class="composition__number">{percent(row.proportion)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</ScrollRegion>

		{#if coverage && !coverage.complete && coverage.listed < coverage.total}
			<!--
				Stated rather than absorbed into a synthesised sixth role. `CodeRole` is a closed
				contract enum, and inventing a row the API can never send is the fabrication this
				whole pipeline exists to prevent.
			-->
			<p class="composition__lead">
				The listed roles account for {integer(coverage.listed)} of {integer(coverage.total)} counted code
				lines. The remainder was not attributed to a role.
			</p>
		{/if}
	{/if}

	<!--
		Largest source files. A table because the payload is a long path plus a number, and
		bars handle long labels badly. The list is server-ordered and bounded by the contract.
	-->
	{#if composition.largest_files.length === 0}
		<p class="composition__lead">This analysis reported no largest-file list.</p>
	{:else}
		<ScrollRegion label="Largest source files by line count">
			<table class="composition__table composition__table--files">
				<caption>
					The largest source files by line count, server-ordered and capped at ten. Size is a
					review-priority signal rather than a defect — and the role column is what stops a large
					generated file being read as a large hand-written one.
				</caption>
				<thead>
					<tr>
						<th scope="col">Path</th>
						<th scope="col">Language</th>
						<th scope="col">Role</th>
						<th scope="col" class="composition__number">Code</th>
					</tr>
				</thead>
				<tbody>
					{#each composition.largest_files as file (file.path)}
						{@const role = codeRole(file.role)}
						<tr>
							<th scope="row"><code class="composition__path">{file.path}</code></th>
							<td>{file.language}</td>
							<td><span class="composition__role" data-role={role.raw}>{role.label}</span></td>
							<td class="composition__number">{integer(file.code_lines)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</ScrollRegion>
	{/if}

	<p class="composition__total">
		{integer(composition.total_lines)} physical lines across {integer(composition.total_files)} counted
		files.
	</p>

	{#if overCounted.length > 0}
		<!--
			A breakdown that sums to more than the total it is a breakdown *of*. Each bar is
			clamped independently, so without this the reader sees a set of plausible bars whose
			sum is impossible and no indication that anything is wrong.
		-->
		<div class="composition__inconsistency">
			<p class="composition__inconsistency-lead">
				<StatusChip state="UNABLE_TO_VERIFY" />
				<span>Part of this breakdown does not add up, and is reported rather than adjusted.</span>
			</p>
			<ul>
				{#each overCounted as entry (entry.label)}
					<li>
						The {entry.label} rows total {integer(entry.listed)} code lines, more than the
						{integer(entry.total)} the analysis reported overall.
					</li>
				{/each}
			</ul>
		</div>
	{/if}

	<h3 class="composition__ledger-heading" id="composition-ledger">What was counted</h3>

	<p class="composition__lead">
		LOC misleads exactly when nobody can see what was left out, so the ledger is part of the result
		rather than a note about it. Each figure expands to the rule behind it.
	</p>

	{#if ledger}
		<p class="composition__ledger">
			<span>Counted: <strong>{integer(ledger.counted)}</strong></span>
			<span aria-hidden="true">·</span>
			<span>Excluded: <strong>{integer(ledger.excluded)}</strong></span>
			<span aria-hidden="true">·</span>
			<span>Unable to classify: <strong>{integer(ledger.unclassified)}</strong></span>
		</p>

		<div class="composition__ledger-detail">
			<Disclosure summary="Counted" hint={`${integer(ledger.counted)} files`}>
				<p>
					Files the exclusion policy admitted and the counter could attribute to a language. Every
					number above this point is drawn from exactly these files.
				</p>
				<p>
					Counter <code>{composition.counter}</code>
					{composition.counter_version} · exclusion policy version {composition.exclusion_policy_version}.
				</p>
			</Disclosure>

			<Disclosure
				summary="Excluded"
				hint={`${integer(ledger.excluded)} files · ${bytes(ledger.excludedBytes)}`}
			>
				{#if composition.exclusions.length === 0}
					<p>No exclusion rule matched anything in this repository.</p>
				{:else}
					<ScrollRegion label="Every exclusion, with the policy rule that produced it">
						<table class="composition__table">
							<caption>
								Every exclusion, with the policy rule that produced it. The rule is what makes the
								decision traceable rather than merely stated.
							</caption>
							<thead>
								<tr>
									<th scope="col">Path or glob</th>
									<th scope="col">Matched rule</th>
									<th scope="col" class="composition__number">Files</th>
									<th scope="col" class="composition__number">Bytes</th>
									<th scope="col">Reason</th>
								</tr>
							</thead>
							<tbody>
								{#each composition.exclusions as exclusion (exclusion.path_or_rule)}
									<tr>
										<th scope="row"><code>{exclusion.path_or_rule}</code></th>
										<td><code>{exclusion.matched_rule}</code></td>
										<td class="composition__number">{integer(exclusion.file_count)}</td>
										<td class="composition__number">{bytes(exclusion.bytes)}</td>
										<td class="composition__reason">{exclusion.reason}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</ScrollRegion>
				{/if}
			</Disclosure>

			<Disclosure summary="Unable to classify" hint={`${integer(ledger.unclassified)} files`}>
				<p class="composition__unclassified">
					<StatusChip state="UNABLE_TO_VERIFY" />
					<span>
						Files the policy admitted but could not attribute to a language. They are reported
						rather than folded silently into a bucket, because a count that absorbs its own unknowns
						cannot be checked.
					</span>
				</p>
			</Disclosure>
		</div>
	{/if}
{/if}

<style>
	.composition__disclaimer {
		max-inline-size: var(--measure);
		margin: 0;
		padding: var(--space-3);
		border-inline-start: 3px solid var(--border-strong);
		background-color: var(--surface-1);
		font-weight: var(--font-weight-medium);
	}

	.composition__provenance,
	.composition__total,
	.composition__lead {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.composition__table {
		min-inline-size: 28rem;
	}

	.composition__table--files {
		min-inline-size: 34rem;
	}

	.composition__path {
		overflow-wrap: anywhere;
	}

	/*
	 * Neutral for every role, deliberately. A role is structural evidence, not a verdict:
	 * colouring `GENERATED` as a warning would smuggle in a judgement the analyzer never
	 * made. The word carries the whole meaning, which is also what makes it correct in
	 * greyscale and forced colours.
	 */
	.composition__role {
		display: inline-block;
		padding: 0 var(--space-2);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-sm);
		font-size: var(--font-size-sm);
		white-space: nowrap;
	}

	.composition__inconsistency {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-inline-size: var(--measure);
		padding: var(--space-3);
		border: var(--border-width) dashed var(--border-strong);
		border-radius: var(--radius-sm);
	}

	.composition__inconsistency-lead {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		margin: 0;
	}

	.composition__inconsistency ul {
		margin: 0;
		padding-inline-start: var(--space-6);
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.composition__number {
		text-align: end;
		font-variant-numeric: tabular-nums;
	}

	.composition__reason {
		min-inline-size: 16rem;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.composition__ledger-heading {
		margin: 0;
		scroll-margin-block-start: var(--space-8);
	}

	.composition__ledger {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-3);
		max-inline-size: none;
		margin: 0;
		font-variant-numeric: tabular-nums;
	}

	.composition__ledger-detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.composition__unavailable {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.composition__unavailable-lead,
	.composition__unclassified {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		max-inline-size: var(--measure);
		margin: 0;
	}
</style>
