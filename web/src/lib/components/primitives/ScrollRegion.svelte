<script lang="ts">
	/*
	 * A container whose content scrolls horizontally instead of the page body.
	 *
	 * The report has genuinely wide content — five-column tables, long repository paths —
	 * and at 360px it cannot all fit. The requirement is that *the page body* never scrolls
	 * sideways; wide blocks scroll inside their own box.
	 *
	 * **Why it is focusable.** A region that scrolls with a mouse wheel or a trackpad but
	 * cannot be reached by keyboard is unreachable content for anyone not using a pointer,
	 * and Safari in particular gives it no keyboard affordance at all. `tabindex="0"` makes
	 * the arrow keys work. This is axe's `scrollable-region-focusable` rule, and it is a
	 * serious failure rather than an advisory one.
	 *
	 * The cost is honest and accepted: the container is a tab stop even on a wide viewport
	 * where nothing overflows. Making focusability depend on measured overflow would move
	 * tab stops around as the window resizes — a keyboard user's mental map of the page
	 * would change under them, which is worse than one predictable extra stop.
	 *
	 * `role="group"` rather than `region`: it takes a name, so the stop announces what it
	 * is, without adding a landmark to a document that already has four sections.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		/** What is inside, announced when the container takes focus. */
		label: string;
		children: Snippet;
	}

	let { label, children }: Props = $props();
</script>

<!--
	`a11y_no_noninteractive_tabindex` is wrong for this specific element, and the two rules
	genuinely conflict: Svelte's says a non-interactive element must not be a tab stop, and
	axe's `scrollable-region-focusable` says a scrollable one must be. Scrolling *is* the
	interaction here, and axe's rule is the one backed by a real user need — content a
	keyboard user otherwise cannot reach. Suppressed narrowly, on the one element that
	scrolls, rather than by disabling the rule.
-->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="scroll-region" tabindex="0" role="group" aria-label={label}>
	{@render children()}
</div>

<style>
	.scroll-region {
		overflow-x: auto;
	}
</style>
