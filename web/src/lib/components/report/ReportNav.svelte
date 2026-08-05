<script lang="ts">
	/*
	 * In-page navigation for the report's sections.
	 *
	 * **The click handler moves focus; the browser does the scrolling.** `preventDefault`
	 * is deliberately *not* called: the default action updates the URL hash, which is what
	 * makes `#findings` shareable and what makes `:target` work on a fresh load. All this
	 * handler adds is the half the platform leaves out — focus.
	 *
	 * `preventScroll: true` on the focus call keeps the two from fighting: focus moves
	 * silently, then the anchor's own scroll runs, honouring `scroll-behavior` (which
	 * global.css turns off under `prefers-reduced-motion`).
	 *
	 * Native `<a>` elements, not buttons and not a Bits UI component. These are links to
	 * document positions; a router is not involved and neither is any non-trivial
	 * interaction.
	 */

	import { focusAnchor } from './anchor';
	import type { ReportSectionLink } from './sections';

	interface Props {
		sections: readonly ReportSectionLink[];
	}

	let { sections }: Props = $props();

	/*
	 * A hash in the URL on first load has the same problem as a click: the browser scrolls
	 * but leaves focus at the document root. Someone arriving from a shared
	 * `/reports/…#composition` link should land in the section, not above it.
	 */
	$effect(() => {
		const id = window.location.hash.slice(1);
		if (id) focusAnchor(id);
	});
</script>

<nav class="report-nav" aria-label="Report sections">
	<ul class="report-nav__list">
		{#each sections as section (section.id)}
			<li>
				<a class="report-nav__link" href={`#${section.id}`} onclick={() => focusAnchor(section.id)}>
					{section.label}
				</a>
			</li>
		{/each}
	</ul>
</nav>

<style>
	.report-nav {
		padding-block: var(--space-3);
		border-block: var(--border-width) solid var(--border-subtle);
	}

	.report-nav__list {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.report-nav__link {
		font-size: var(--font-size-sm);
		font-weight: var(--font-weight-medium);
	}
</style>
