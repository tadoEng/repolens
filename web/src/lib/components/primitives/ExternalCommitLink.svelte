<script lang="ts">
	/*
	 * A link to the exact analyzed commit on GitHub.
	 *
	 * The point of the whole product is that a report is checkable, and this is the
	 * shortest path from a claim to the thing it is a claim about. The URL is built from
	 * the repository identity and the resolved SHA — never from anything the report says
	 * about itself — so it cannot point at a different commit than the one analyzed.
	 *
	 * No `target="_blank"`. Opening a new tab is a decision that belongs to the reader
	 * (middle-click, modifier-click), and forcing it breaks the back button for everyone
	 * who did not want it.
	 */

	interface Props {
		owner: string;
		name: string;
		/** Resolved commit SHA. Callers must not render this component before it exists. */
		commitSha: string;
	}

	let { owner, name, commitSha }: Props = $props();

	const href = $derived(
		`https://github.com/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/commit/${encodeURIComponent(commitSha)}`
	);
</script>

<!--
	`no-navigation-without-resolve` exists to catch app links that bypass the route table.
	This one leaves the app entirely: `resolve()` would prefix `kit.paths.base` onto an
	absolute `https://github.com/…` URL, which is exactly the wrong thing to do. Disabled on
	the single line rather than configured away, so the rule keeps working everywhere else.
-->
<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
<a {href} rel="noreferrer">
	View this commit on GitHub
	<span class="visually-hidden">for {owner}/{name}</span>
</a>
