<script lang="ts">
	/*
	 * Every piece of evidence in the report, in one always-open index.
	 *
	 * This is the third mitigation for the `<details>` find-in-page problem, and the only
	 * one that does not depend on the reader having pressed anything: paths and digests are
	 * in the document unconditionally, so `Ctrl+F` for `Cargo.toml` finds it whether or not
	 * the finding that cites it has been expanded. It is also what makes a printed or
	 * PDF-exported report complete.
	 *
	 * Excerpts are deliberately *not* repeated here. They live with the finding that draws
	 * a conclusion from them, and duplicating them would double the page weight while
	 * making it possible for two copies of the same excerpt to disagree after an edit.
	 */
	import ScrollRegion from '$lib/components/primitives/ScrollRegion.svelte';
	import { evidenceKind } from '$lib/contract/enums';
	import { shortDigest } from '$lib/contract/format';
	import type { Finding } from '@repolens/api-client';

	import { focusAnchor } from './anchor';

	interface Props {
		findings: Finding[];
	}

	let { findings }: Props = $props();

	const cited = $derived(findings.filter((finding) => finding.evidence.length > 0));
</script>

{#if cited.length === 0}
	<p>
		No finding in this report cites evidence. Where the analyzer could not check something, there is
		nothing to index.
	</p>
{:else}
	<ScrollRegion label="Every cited file, path and digest">
		<table class="appendix">
			<caption>
				Every cited file, path and digest, listed openly so the report stays searchable and
				printable without expanding anything.
			</caption>
			<thead>
				<tr>
					<th scope="col">Finding</th>
					<th scope="col">Kind</th>
					<th scope="col">Path</th>
					<th scope="col">Lines</th>
					<th scope="col">Digest</th>
				</tr>
			</thead>
			<tbody>
				{#each cited as finding (finding.id)}
					{#each finding.evidence as item, index (`${finding.id}-${index}`)}
						{@const kind = evidenceKind(item.kind)}
						<tr>
							{#if index === 0}
								<th scope="row" rowspan={finding.evidence.length}>
									<a
										href={`#finding-${finding.id}`}
										onclick={() => focusAnchor(`finding-${finding.id}`)}
									>
										{finding.title}
									</a>
									<span class="appendix__rule"><code>{finding.rule_id}</code></span>
								</th>
							{/if}
							<td>{kind.label}</td>
							<td>
								{#if item.path}<code class="appendix__path">{item.path}</code>{:else}—{/if}
							</td>
							<td>
								{#if item.line_range}{item.line_range.start}–{item.line_range.end}{:else}—{/if}
							</td>
							<td>
								{#if item.digest}
									<code title={item.digest}>{shortDigest(item.digest)}</code>
								{:else}
									—
								{/if}
							</td>
						</tr>
					{/each}
				{/each}
			</tbody>
		</table>
	</ScrollRegion>
{/if}

<style>
	.appendix {
		min-inline-size: 40rem;
		font-size: var(--font-size-sm);
	}

	.appendix th[scope='row'] {
		vertical-align: top;
		font-size: var(--font-size-sm);
		color: var(--text-primary);
	}

	.appendix__rule {
		display: block;
		margin-block-start: var(--space-1);
		font-weight: var(--font-weight-regular);
		color: var(--text-muted);
	}

	.appendix__path {
		overflow-wrap: anywhere;
	}
</style>
