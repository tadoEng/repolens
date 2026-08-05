<script lang="ts">
	/*
	 * Walking-skeleton diagnostic (issue #11).
	 *
	 * This exists to prove one thing end to end: a statically hosted page on Cloudflare can
	 * reach the Axum service on Cloud Run, through the *generated* client, and read a value
	 * that came from Neon. Every layer it crosses is a layer that has never been exercised
	 * together before.
	 *
	 * It lives in the footer rather than on a fourth route, so the path is exercised by
	 * every page without widening the navigation the plan deliberately keeps to three
	 * routes. It is scaffolding: once analyses render real report metadata here, this
	 * becomes redundant and should be removed rather than left to rot.
	 *
	 * Not a health dashboard. No polling, no alerting, no history — one request on mount.
	 */
	import { api } from '$lib/api/client';
	import type { components } from '@repolens/api-client';

	type ProbeResponse = components['schemas']['SystemProbeResponse'];

	// Four states, because "still loading" and "the request failed" are different facts and
	// collapsing them is how a spinner ends up lying about a network error.
	type ProbeState =
		| { kind: 'loading' }
		| { kind: 'loaded'; probe: ProbeResponse }
		| { kind: 'unreachable'; reason: string };

	let state = $state<ProbeState>({ kind: 'loading' });

	$effect(() => {
		let cancelled = false;

		void (async () => {
			try {
				const { data, error } = await api.GET('/api/v1/system/probe');
				if (cancelled) return;

				if (error || !data) {
					// The API answered, but not with a probe. Distinct from a transport
					// failure: the origin is right and CORS passed, so this is a server
					// fault rather than a configuration one.
					state = { kind: 'unreachable', reason: 'the API responded without a probe result' };
					return;
				}

				state = { kind: 'loaded', probe: data };
			} catch {
				if (cancelled) return;
				// Transport failure. On a deployed build this most often means the CSP
				// connect-src allowlist and PUBLIC_API_ORIGIN disagree, which is exactly
				// the failure this skeleton exists to surface early.
				state = { kind: 'unreachable', reason: 'the API could not be reached' };
			}
		})();

		return () => {
			cancelled = true;
		};
	});

	function shortSha(sha: string): string {
		// `unknown` is a real value from a local build, not an error, so it is shown as-is
		// rather than truncated into something that looks like a SHA.
		return sha === 'unknown' ? sha : sha.slice(0, 7);
	}
</script>

<!--
	aria-live="polite" because this resolves after paint: a screen-reader user who has
	already moved on should be told the result, not interrupted by it.
-->
<p class="probe" aria-live="polite">
	{#if state.kind === 'loading'}
		<span class="probe__label">System</span>
		<span class="probe__value">checking…</span>
	{:else if state.kind === 'unreachable'}
		<span class="probe__label">System</span>
		<span class="probe__value probe__value--degraded">
			<!-- Status is never colour alone (§3.4); the word carries the meaning. -->
			API unavailable — {state.reason}
		</span>
	{:else}
		<span class="probe__label">System</span>
		<span class="probe__value">
			API {state.probe.api} · database {state.probe.database} · build
			<code>{shortSha(state.probe.build_sha)}</code>
			{#if state.probe.schema_version !== null && state.probe.schema_version !== undefined}
				· schema v{state.probe.schema_version}
			{:else}
				<!--
					Null is a designed state, not a gap: the database was unreachable, so the
					schema version is unknown. Rendering it as "0" would let a connection
					failure read as an empty database.
				-->
				· schema unknown
			{/if}
		</span>
	{/if}
</p>

<style>
	.probe {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.probe__label {
		font-weight: var(--font-weight-medium);
		color: var(--text-secondary);
	}

	.probe__value--degraded {
		color: var(--status-unable-to-verify-fg);
	}

	code {
		font-family: var(--font-mono);
		font-size: inherit;
	}
</style>
