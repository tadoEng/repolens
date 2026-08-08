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
	 *
	 * ## Provenance is the header, not the small print
	 *
	 * The chart is not the most important thing on screen. Commit, tree, counter version,
	 * exclusion-policy version and classification-policy version are: they are what turns a
	 * number a reader can quote into a number a reader can *argue with*. A count with no
	 * provenance is a rumour, so those values open the section with the weight of the claim
	 * rather than trailing it as a grey sentence.
	 *
	 * These are the section's **composition provenance**, not the report's reproducibility
	 * key. The key is the wider set in `repolens_core::reproducibility` — it also carries the
	 * repository coordinate, the evidence source and its API version, the analyzer version
	 * and the ruleset version. Naming this subset "the key" would claim that matching these
	 * five values is enough for two reports to be comparable, which is exactly the kind of
	 * overstatement this section exists not to make. What these five settle is narrower and
	 * still worth stating plainly: whether two *counts* may be compared.
	 *
	 * Commit and tree come from the report rather than from `LineCountSummary`, which is why
	 * this component takes them as props. They also appear in `ReportHeader`; that is
	 * deliberate duplication, because this section is the one people screenshot and quote,
	 * and a figure separated from its provenance is the thing that gets misused.
	 *
	 * ## Totals before detail
	 *
	 * The reading order is totals, then the breakdowns, then what was left out. A reader who
	 * wants one number gets it in the first screen; a reader who wants to check it reads on.
	 * Nothing in the figure row is derived — every value is a field the server sent, and
	 * `Languages listed` is named for what it counts, because the contract does not promise
	 * the per-language breakdown is exhaustive and `detected` would claim it does.
	 */
	import { proportionBar } from './proportion';
	import CopyableSha from '$lib/components/primitives/CopyableSha.svelte';
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
		/** Analyzed commit. Part of the composition provenance this section's header states. */
		commitSha: string;
		/** Root tree the collectors walked. Two commits sharing one yield identical counts. */
		treeSha: string;
	}

	let { composition, limitations, commitSha, treeSha }: Props = $props();

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
	<!--
		Composition provenance, as a header. Same `<dl>` idiom as `ReportHeader`, because
		these are the same kind of value and a reader who has learned to look for provenance
		once should not have to learn a second shape for it.
	-->
	<div class="composition__provenance">
		<dl class="composition__key">
			<div class="composition__key-fact">
				<dt>Commit</dt>
				<dd><CopyableSha value={commitSha} label="analyzed commit SHA" /></dd>
			</div>
			<div class="composition__key-fact">
				<dt>Tree</dt>
				<dd><CopyableSha value={treeSha} label="analyzed tree SHA" /></dd>
			</div>
			<div class="composition__key-fact">
				<dt>Counter</dt>
				<dd><code>{composition.counter}</code> {composition.counter_version}</dd>
			</div>
			<div class="composition__key-fact">
				<dt>Exclusion policy</dt>
				<dd>version {composition.exclusion_policy_version}</dd>
			</div>
			<!--
				Beside the exclusion policy, not folded into it. The two answer different
				questions — what was left out, and what the rest *is* — and either can change
				without the other. A changed classifier moves the production share without a
				single file changing, so a count that cannot name its classifier cannot be
				compared with another.
			-->
			<div class="composition__key-fact">
				<dt>Classification policy</dt>
				<dd>version {composition.classification_policy_version}</dd>
			</div>
		</dl>

		<p class="composition__lead">
			Different counter versions count differently, and a different classification policy moves the
			share each role holds without any file changing. Both versions are part of the result for that
			reason.
		</p>
	</div>

	<!--
		Totals first. Four fields the server sent, none of them arithmetic done here, and
		`Physical lines` kept distinct from `Lines of code` because collapsing the two is the
		commonest way a LOC figure ends up overstated by a quarter.
	-->
	<dl class="composition__figures">
		<div class="composition__figure">
			<dt>Lines of code</dt>
			<dd>{integer(composition.code_lines)}</dd>
		</div>
		<div class="composition__figure">
			<dt>Physical lines</dt>
			<dd>{integer(composition.total_lines)}</dd>
		</div>
		<div class="composition__figure">
			<dt>Files counted</dt>
			<dd>{integer(composition.total_files)}</dd>
		</div>
		<div class="composition__figure">
			<dt>Languages listed</dt>
			<dd>{integer(composition.languages.length)}</dd>
		</div>
	</dl>

	<div class="composition__view">
		<MetricTable
			caption="Lines of code by language, as a share of all counted code lines."
			rowHeader="Language"
			valueHeader="Lines of code"
			rows={languageCodeRows(composition)}
		/>
	</div>

	<div class="composition__view">
		<MetricTable
			caption="Lines of code by top-level area, as a share of all counted code lines."
			rowHeader="Area"
			valueHeader="Lines of code"
			rows={areaCodeRows(composition)}
		/>
	</div>

	<div class="composition__view">
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
	</div>

	<!--
		Production / test / generated, with one share bar embedded in the table.

		This is not a third chart. The two *comparative* views — by language and by area —
		exist to rank items against each other, and they are the only ones a reader scans for
		"which is biggest". This bar answers a different question about a single row: what
		proportion of the repository this role accounts for. It reads along the row it
		describes rather than as a figure in its own right.

		The percentage stays as text beside it. The bar is the fast read; the number is the
		one a reader can quote, and a sub-1% role has to remain legible when its bar is a
		sliver.
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
		<div class="composition__view">
			<ScrollRegion label="Code lines by role">
				<table class="composition__table">
					<caption>
						Code lines by role. Structural evidence, not a judgement: generated code is named as
						generated so it is not counted as hand-written work. `Unclassified` appears only when it
						is above zero, and when it does it is counted against the same total as every other row
						— the shares are not rebased over the roles that were recognised.
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
								<td
									class="composition__number composition__share"
									{@attach proportionBar(row.proportion)}
								>
									<span class="composition__share-value">{percent(row.proportion)}</span>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</ScrollRegion>

			{#if coverage && !coverage.complete && coverage.listed < coverage.total}
				<!--
					A genuine arithmetic gap in what the server sent, not the classifier declining to
					attribute a file — that case has its own role now, `UNCLASSIFIED`, and it arrives
					as a row rather than as a shortfall. So this fires only when the rows really do
					not add up, and it is still stated rather than absorbed into an invented row:
					`CodeRole` is a closed contract enum, and putting a value on screen the API can
					never send is the fabrication this whole pipeline exists to prevent.
				-->
				<p class="composition__lead">
					The listed roles account for {integer(coverage.listed)} of {integer(coverage.total)} counted
					code lines. The remainder was not attributed to a role.
				</p>
			{/if}
		</div>
	{/if}

	<!--
		Largest source files. A table because the payload is a long path plus a number, and
		bars handle long labels badly. The list is server-ordered and bounded by the contract.
	-->
	{#if composition.largest_files.length === 0}
		<p class="composition__lead">This analysis reported no largest-file list.</p>
	{:else}
		<div class="composition__view">
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
		</div>
	{/if}

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

	<div class="composition__ledger-block">
		<h3 class="composition__ledger-heading" id="composition-ledger">What was counted</h3>

		<p class="composition__lead">
			LOC misleads exactly when nobody can see what was left out, so the ledger is part of the
			result rather than a note about it. Each figure expands to the rule behind it.
		</p>

		{#if ledger}
			<!--
				`Label: value` stays one contiguous string in each item rather than becoming a
				`<dt>`/`<dd>` pair: a definition list here reads as "Counted842" to anything that
				concatenates text nodes, which includes the suites that assert on these figures and
				a reader copying the line out.
			-->
			<p class="composition__ledger">
				<span>Counted: <strong>{integer(ledger.counted)}</strong></span>
				<span>Excluded: <strong>{integer(ledger.excluded)}</strong></span>
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

				<!--
					Open on arrival, alone among the three. This is the disclosure that carries the
					claim the section is built on — not "we counted 78,310 lines" but "here is what
					we did not count, and the rule that says why" — and the file's own argument about
					footnotes applies to a closed `<details>` just as well: it is where a caveat goes
					to be skipped. The other two summarise in their hint; this one has a table.
				-->
				<Disclosure
					summary="Excluded"
					hint={`${integer(ledger.excluded)} files · ${bytes(ledger.excludedBytes)}`}
					open
				>
					{#if composition.exclusions.length === 0}
						<p>No exclusion rule matched anything in this repository.</p>
					{:else}
						<ScrollRegion label="Every exclusion, with the policy rule that produced it">
							<table class="composition__table composition__table--exclusions">
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
							rather than folded silently into a bucket, because a count that absorbs its own
							unknowns cannot be checked.
						</span>
					</p>
				</Disclosure>
			</div>
		{/if}
	</div>
{/if}

<style>
	/*
	 * The section is a stack of unlike things — a caveat, a key, four figures, five views
	 * and a ledger — and the enclosing `ReportSection` sets one gap for all of them. Left
	 * alone it reads as an undifferentiated pile: the disclaimer sits as far from the
	 * provenance as the fourth table sits from the fifth. These margins add to that gap
	 * rather than replacing it, so the rhythm is three tiers instead of one.
	 */
	.composition__figures,
	.composition__ledger-block {
		margin-block-start: var(--space-4);
	}

	.composition__view + .composition__view {
		margin-block-start: var(--space-2);
	}

	.composition__disclaimer {
		max-inline-size: var(--measure);
		margin: 0;
		padding: var(--space-3) var(--space-4);
		border-inline-start: 3px solid var(--border-strong);
		border-start-end-radius: var(--radius-md);
		border-end-end-radius: var(--radius-md);
		background-color: var(--surface-1);
		font-weight: var(--font-weight-medium);
		text-wrap: balance;
	}

	.composition__lead {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	/*
	 * The provenance header. Its own panel, because the four values inside it are one
	 * thing — the key that makes every number below reproducible — and a grid of loose
	 * pairs would read as four unrelated facts.
	 */
	.composition__provenance {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background-color: var(--surface-1);
	}

	.composition__key {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
		gap: var(--space-3) var(--space-6);
		margin: 0;
	}

	.composition__key-fact {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	/*
	 * Figures, not a chart. Three of these are totals and one is a count of rows; all four
	 * are read across, so the labels stay uniform and quiet and the numbers carry the size.
	 */
	.composition__figures {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
		gap: var(--space-4) var(--space-6);
		margin-block-end: var(--space-2);
	}

	.composition__figure {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.composition__figure dd {
		margin: 0;
		font-size: var(--font-size-xl);
		font-weight: var(--font-weight-medium);
		line-height: var(--line-height-heading);
		/*
		 * Proportional figures, deliberately. `tabular-nums` gives every digit the width of
		 * a zero, which is what makes a column of numbers align and what makes a single
		 * large number look gappy. The columns inside the tables keep it; these do not.
		 */
		font-variant-numeric: proportional-nums;
	}

	/* One label treatment for both name/value grids, matching `ReportHeader`'s. */
	dt {
		font-size: var(--font-size-xs);
		font-weight: var(--font-weight-medium);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
	}

	.composition__key-fact dd {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-primary);
	}

	/*
	 * Each view in its own panel. There are no sub-headings here on purpose — a `<caption>`
	 * labels data and a heading does not — so the boundary has to come from the box, or the
	 * five views run together into one column of rows and the reader loses which numbers
	 * belong to which question.
	 */
	.composition__view {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
	}

	.composition__table {
		min-inline-size: 28rem;
	}

	.composition__table--files {
		min-inline-size: 34rem;
	}

	.composition__table--exclusions {
		min-inline-size: 44rem;
	}

	/* Same treatment as `MetricTable`'s, so the five views read as one set. */
	.composition__table caption {
		padding-block-end: var(--space-3);
	}

	.composition__table thead th {
		border-block-end: var(--border-width) solid var(--border-strong);
	}

	.composition__table tbody tr:last-child :is(th, td) {
		border-block-end: none;
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
		background-color: var(--surface-1);
		color: var(--text-secondary);
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

	/*
	 * The embedded share bar. Same mechanism and same geometry as MetricTable: a track and
	 * a fill in their own lane under the number, sized by --proportion, which `proportionBar`
	 * sets through CSSOM because the CSP forbids an inline style attribute and every bar
	 * would otherwise render at zero width in production only.
	 *
	 * The bar is a background; it never sizes the text, and it no longer runs behind it
	 * either. A role at 0.4% still shows a fully legible percentage above a sliver.
	 */
	.composition__share {
		position: relative;
		isolation: isolate;
		/* Claimed, for the same reason MetricTable claims its value column: a share bar
		 * squeezed into the width of `4.1%` is not a track anyone can read a share off. */
		inline-size: 22%;
		padding-block-end: calc(var(--space-2) + var(--chart-bar-size) + var(--space-1));
	}

	.composition__share::before,
	.composition__share::after {
		content: '';
		position: absolute;
		inset-block-end: var(--space-2);
		inset-inline-end: var(--space-3);
		block-size: var(--chart-bar-size);
		border-start-start-radius: var(--chart-bar-size);
		border-end-start-radius: var(--chart-bar-size);
	}

	.composition__share::after {
		inset-inline-start: var(--space-3);
		background-color: var(--chart-track);
		z-index: -2;
	}

	.composition__share::before {
		inline-size: calc(var(--proportion, 0) * (100% - 2 * var(--space-3)));
		background-color: var(--chart-fill);
		z-index: -1;
	}

	.composition__share-value {
		position: relative;
		z-index: 1;
	}

	/*
	 * Windows High Contrast strips background colours, so the bar disappears. The
	 * percentage is still there, which is why dropping the bar degrades rather than
	 * breaks.
	 */
	@media (forced-colors: active) {
		.composition__share::before,
		.composition__share::after {
			display: none;
		}
	}

	.composition__number {
		text-align: end;
		font-variant-numeric: tabular-nums;
	}

	.composition__reason {
		min-inline-size: 18rem;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	/*
	 * The ledger, given the weight of the claim it carries. This is the part of the section
	 * that no raw counter produces: not "78,310 lines" but "78,310 lines, and here is
	 * exactly what was left out and under which rule". A hairline above it separates the
	 * audit trail from the views it makes checkable.
	 */
	.composition__ledger-block {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding-block-start: var(--space-8);
		border-block-start: var(--border-width) solid var(--border-subtle);
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

	/* The three figures as their own objects. The interpunct separators they replace made
	 * one run-on line where there are three separate counts. */
	.composition__ledger span {
		padding: var(--space-1) var(--space-3);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-sm);
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.composition__ledger strong {
		color: var(--text-primary);
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
