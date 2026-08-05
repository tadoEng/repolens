<script lang="ts">
	// `resolve()` rather than a literal href: it applies `kit.paths.base` and is checked
	// against the route table, so a renamed or deleted route becomes a build error instead
	// of a dead link. Enforced by eslint-plugin-svelte's no-navigation-without-resolve.
	import { resolve } from '$app/paths';

	import SystemProbe from '$lib/components/SystemProbe.svelte';
	import '$lib/styles/global.css';

	let { children } = $props();
</script>

<a class="skip-link" href="#main">Skip to content</a>

<div class="app">
	<header class="app-header">
		<div class="app-header__inner">
			<a class="wordmark" href={resolve('/')}>RepoLens</a>
			<!--
				Minimal auth state (§3.1) lands with the Firebase gate at issue #13.
				Nothing is rendered here yet because there is no auth state to render, and
				a placeholder control would have to be un-built rather than filled in.
			-->
		</div>
	</header>

	<main id="main" class="app-main" tabindex="-1">
		{@render children()}
	</main>

	<footer class="app-footer">
		<div class="app-footer__inner">
			<!--
				Analyzer and ruleset versions are first-class report metadata and belong
				here (§3.1). They come from the report DTO, which is blocked on issue #14.
			-->
			<small
				>RepoLens · analyzer and ruleset versions appear here once the report contract lands</small
			>
			<!--
				Walking-skeleton diagnostic (issue #11): proves Cloudflare → generated client →
				Axum → Neon on every route. Scaffolding, to be removed once real report
				metadata renders here.
			-->
			<SystemProbe />
		</div>
	</footer>
</div>

<style>
	.app {
		display: flex;
		flex-direction: column;
		min-block-size: 100vh;
	}

	.app-header {
		border-block-end: var(--border-width) solid var(--border-subtle);
		background-color: var(--surface-0);
	}

	.app-header__inner,
	.app-footer__inner,
	.app-main {
		inline-size: 100%;
		max-inline-size: var(--content-max);
		margin-inline: auto;
		padding-inline: var(--content-gutter);
	}

	.app-header__inner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding-block: var(--space-4);
	}

	.wordmark {
		font-size: var(--font-size-lg);
		font-weight: var(--font-weight-bold);
		letter-spacing: var(--letter-spacing-heading);
		color: var(--text-primary);
		text-decoration: none;
	}

	.app-main {
		flex: 1;
		padding-block: var(--space-12);
	}

	.app-footer {
		border-block-start: var(--border-width) solid var(--border-subtle);
		background-color: var(--surface-1);
	}

	.app-footer__inner {
		padding-block: var(--space-6);
	}
</style>
