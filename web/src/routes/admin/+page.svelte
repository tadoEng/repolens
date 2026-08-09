<script lang="ts">
	/*
	 * The operational dashboard.
	 *
	 * ## This page is a client, not a gate
	 *
	 * Axum decides who may read operational data: it verifies a Firebase ID token and checks
	 * the uid against `ADMIN_FIREBASE_UIDS`, answering `401` without a credential and `403`
	 * for a signed-in caller who is not allow-listed. Everything below presents that
	 * decision. Nothing here withholds a request because the browser believes it would be
	 * refused — a page that decided for itself would have a second, weaker copy of the rule,
	 * and `/admin` being hard to find has never been access control.
	 *
	 * The one thing the page does short of the network is decline to send a request before
	 * Firebase has finished restoring a session, because `unknown` is not `signed-out`.
	 * Sending then would produce a `401` for somebody who is in fact signed in.
	 *
	 * ## One load, one snapshot, an explicit refresh
	 *
	 * No polling. The endpoint returns counters as they stand and keeps no history, so a
	 * ticking page would imply a time series that does not exist and would invite reading
	 * two consecutive renders as a trend. Refreshing is a button, and the timestamp beside it
	 * says when the figures were read.
	 *
	 * ## Two requests, and why that is still "one load"
	 *
	 * The snapshot carries what the process measured about itself. The schema version and
	 * whether the database answered are deployment facts published by the system probe,
	 * which is already in the contract and already anonymous. Both are fetched once,
	 * together, on load. Asking the backend to duplicate the probe's fields into the
	 * operational DTO would have been new backend surface to save one request.
	 */
	import { fetchAdminOverview, fetchSystemProbe } from '$lib/api/admin';
	import type { AdminOverview, SystemProbeResponse } from '$lib/api/admin';
	import type { Fetched } from '$lib/api/fetched';
	import { session } from '$lib/auth/session.svelte';
	import AdminRefusal, { type Remedy } from '$lib/components/admin/AdminRefusal.svelte';
	import AdminSection from '$lib/components/admin/AdminSection.svelte';
	import MetricCard from '$lib/components/admin/MetricCard.svelte';
	import RouteTable from '$lib/components/admin/RouteTable.svelte';
	import Button from '$lib/components/primitives/Button.svelte';
	import CopyableSha from '$lib/components/primitives/CopyableSha.svelte';
	import { describeProbeStatus } from '@repolens/api-client';
	import { integer, timestamp, uptime } from '$lib/contract/format';

	let overview = $state<Fetched<AdminOverview> | null>(null);
	let probe = $state<Fetched<SystemProbeResponse> | null>(null);
	let loading = $state(false);
	let readAt = $state<string | null>(null);

	/** Restores an existing session, once, after mount. */
	$effect(() => {
		void session.initialize();
	});

	async function load(): Promise<void> {
		if (loading) return;
		loading = true;
		try {
			// Together rather than in sequence: they are independent reads and one is not a
			// precondition for the other, so serialising them would double the wait for no
			// ordering that matters.
			const [snapshot, system] = await Promise.all([fetchAdminOverview(), fetchSystemProbe()]);
			overview = snapshot;
			probe = system;
			readAt = new Date().toISOString();
		} finally {
			loading = false;
		}
	}

	/*
	 * Load once the session has settled either way.
	 *
	 * `signed-out` is included deliberately: an anonymous request is sent and refused, and
	 * the refusal is the API's to make. Skipping it would make the page state and the server
	 * state two separate opinions.
	 */
	$effect(() => {
		const settled = session.state.status !== 'unknown';
		if (settled && overview === null && !loading) void load();
	});

	const snapshot = $derived(overview?.kind === 'ok' ? overview.value : null);
	const system = $derived(probe?.kind === 'ok' ? probe.value : null);

	/** The refusal the API answered with, when it answered at all. */
	const refusal = $derived(overview?.kind === 'rejected' ? overview : null);
	const unreachable = $derived(overview?.kind === 'unreachable');

	/**
	 * Which interaction this refusal calls for.
	 *
	 * Derived from `ErrorCode` — **never from the status and never from the message**. The
	 * division of authority is deliberate: the backend owns *why* a request was refused and
	 * writes the sentence a reader sees; this page owns *what the reader can do next*.
	 * Branching on prose would make rewording a Rust string a behaviour change, and
	 * branching on the status would merge codes that share one — `401` is only ever
	 * `UNAUTHENTICATED` today, but that is a fact about this build rather than a contract.
	 *
	 * An unrecognised code lands in `refused`, which offers no action and shows what the
	 * server said. That is the unknown-variant policy: never crash, never drop it silently,
	 * and never invent an affordance for a refusal this build does not understand.
	 */
	const remedy = $derived<Remedy>(
		refusal === null
			? 'refused'
			: refusal.error?.code === 'UNAUTHENTICATED'
				? 'sign-in'
				: refusal.error?.code === 'FORBIDDEN'
					? 'not-permitted'
					: refusal.error?.code === 'AUTHENTICATION_UNAVAILABLE'
						? 'try-again'
						: 'refused'
	);

	/** Whether a sign-in control can do anything at all in this build. */
	const canSignIn = $derived(
		session.state.status === 'signed-out' || session.state.status === 'signed-in'
	);

	/**
	 * The heading for each remedy.
	 *
	 * Written here, never by the server. `Record<Remedy, string>` rather than a lookup with a
	 * default, so a remedy added later fails `pnpm -r check` instead of rendering blank.
	 */
	const REFUSAL_TITLES: Readonly<Record<Remedy, string>> = {
		'sign-in': 'Sign in required',
		'not-permitted': 'Not permitted',
		'try-again': 'Sign-in cannot be checked',
		refused: 'Request refused'
	};

	/**
	 * What to say when the API refused without sending an envelope.
	 *
	 * Reached when a proxy answered instead of the service, so the status is the only fact
	 * available and it is stated rather than dressed up.
	 */
	function refusalFallback(status: number): string {
		return `The API answered with status ${status} and did not explain why.`;
	}

	/**
	 * Requests this process has finished serving, summed over the published rows.
	 *
	 * A sum of what the table shows, not a figure the server sent — so it can never disagree
	 * with the rows beneath it. Requests folded into the `<overflow>` row are included,
	 * because they happened; the row itself says the registry stopped distinguishing them.
	 *
	 * "Completed", not "total": `in_flight` is counted separately and on purpose, and a
	 * request being served right now is in neither this sum nor the histogram it comes from.
	 * A card labelled "total" would invite adding the two.
	 */
	const completedRequests = $derived(
		snapshot?.http.routes.reduce((running, route) => running + route.requests, 0) ?? 0
	);
</script>

<svelte:head>
	<title>Operations · RepoLens</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<h1>Operations</h1>

{#if session.state.status === 'unknown'}
	<p class="notice">Checking whether you are signed in…</p>
{:else if refusal}
	<!--
		Every refusal goes through one component, which is what makes "how many controls does
		this state offer" a property a test can own directly. It could not be owned from the
		end-to-end suite alone: there is no Firebase configuration in CI, so the sign-in
		control is correctly absent from every capture and a suite that only ever saw that
		branch would have to assert something adjacent to the requirement instead.
	-->
	<AdminRefusal
		{remedy}
		title={REFUSAL_TITLES[remedy]}
		message={refusal.error?.message ?? refusalFallback(refusal.status)}
		{canSignIn}
		busy={remedy === 'try-again' ? loading : session.busy}
		onSignIn={() => void session.signIn()}
		onRetry={() => void load()}
	/>
{:else if unreachable}
	<p class="notice">
		The API could not be reached at all — no response arrived. That is a network, CORS, or
		content-security-policy failure rather than a refusal, so there is nothing to sign in to.
	</p>
{:else if snapshot}
	<!--
		The lead and the refresh control live here, inside the loaded branch, rather than
		above the whole page.

		Both describe figures. On a refusal there are none, so a lead promising that "every
		figure on this page describes the single process that answered" would describe an
		empty page — and a Refresh button would be a second retry affordance sitting directly
		above a refusal that deliberately offers none. FORBIDDEN is the case that makes it
		wrong: signing in again cannot change the answer and neither can repeating the
		request, so any control that re-sends it leads somewhere it cannot go.

		This was shipped and caught by *looking at the rendered page*. The behavioural test
		asserted that no button named "Try again" existed, which was true and irrelevant —
		the button was named "Refresh". A test now counts the controls instead of naming
		them.

		The lead's provenance claim is narrowed to what the two requests actually guarantee.
		It used to say "every figure on this page", which the *composition* made false: the
		operational snapshot genuinely describes the process that answered it, but the
		database and schema facts arrive on a separate system-probe request, and with more
		than one instance — or mid-rollout — nothing routes the two to the same process. Each
		endpoint was truthful; putting them under one sentence was not.

		Narrowed in the wording rather than fixed with sticky routing, because the wording is
		what was wrong. The page can say exactly what it knows.
	-->
	<p class="lead">
		The figures under <strong>API / Axum</strong> and <strong>Runtime</strong> describe
		<strong>the single process that answered the operational snapshot</strong>. There is no
		aggregation across instances and no history: counters start at zero when a process starts, so a
		deploy or a restart resets them, and two instances would each report their own. Deployment mixes
		in facts from a second request, and says which.
	</p>

	<div class="controls">
		<Button onclick={() => void load()} disabled={loading}>
			{loading ? 'Reading…' : 'Refresh'}
		</Button>
		{#if readAt}
			<p class="controls__read-at">Read {timestamp(readAt)}</p>
		{/if}
	</div>

	<AdminSection
		id="api"
		title="API / Axum"
		lead="Requests this process has served, by the router pattern it matched."
	>
		<dl class="cards">
			<MetricCard
				label="Completed requests"
				value={integer(completedRequests)}
				detail="Finished and recorded. Requests still in flight are shown separately below."
			/>
			<MetricCard
				label="In flight"
				value={integer(snapshot.http.in_flight)}
				detail="Includes the request that read this page."
			/>
			<MetricCard
				label="Route labels held"
				value={`${integer(snapshot.http.tracked_routes)} of ${integer(snapshot.http.max_tracked_routes)}`}
				detail="Bounded by construction; further labels fold into one overflow row."
			/>
		</dl>

		{#if snapshot.http.routes.length > 0}
			<RouteTable routes={snapshot.http.routes} />
		{:else}
			<p class="empty">
				This process has not completed a request yet, so there is nothing to tabulate. That is an
				empty table rather than a table of zeroes, because a route nobody has called has no latency.
			</p>
		{/if}
	</AdminSection>

	<AdminSection id="runtime" title="Runtime" lead="What the process knows about itself.">
		<dl class="cards">
			<MetricCard label="Uptime" value={uptime(snapshot.process.uptime_seconds)} />
			{#if snapshot.process.resident_bytes === null}
				<MetricCard
					label="Resident memory"
					value={null}
					unavailable="Read from /proc, which this platform does not have. A figure here would be invented."
				/>
			{:else}
				<MetricCard
					label="Resident memory"
					value={`${integer(Math.round(snapshot.process.resident_bytes / 1_048_576))} MiB`}
					detail="VmRSS, as the kernel reports it."
				/>
			{/if}
			<MetricCard
				label="CPU"
				value={null}
				notInstrumented="No CPU accounting was added in Experimental-v1. This is a decision, not a
					platform limit."
			/>
		</dl>
	</AdminSection>

	<AdminSection
		id="postgresql"
		title="PostgreSQL"
		notInstrumented="Query latency, pool size, pool wait and error counts are not measured. The pool
			can report how many connections it holds, but two available numbers are not database
			observability, and a panel assembled from what happens to be reachable would overstate
			what was measured."
	/>

	<AdminSection
		id="analyzer"
		title="Analyzer"
		notInstrumented="Per-phase timings — resolve, tree, blobs, ruleset, report, writes — are not
			recorded. Only a total would be available, and a total is the one figure that cannot
			answer which layer an analysis spends its time in, which is the question worth asking."
	/>

	<AdminSection
		id="deployment"
		title="Deployment"
		lead="Which build answered the operational snapshot, and — from a separate system-probe
			request, which may have been served by a different instance — what the deployment is
			running against."
	>
		<dl class="cards">
			<div class="card-sha">
				<dt class="card-sha__label">Build</dt>
				<dd class="card-sha__value">
					{#if snapshot.process.build_sha === 'unknown'}
						<span>Not a released build</span>
					{:else}
						<CopyableSha value={snapshot.process.build_sha} label="build SHA" />
					{/if}
				</dd>
			</div>
			{#if system}
				<MetricCard
					label="Database"
					value={describeProbeStatus(system.database).label}
					detail="Reachability only, from the system probe."
				/>
				{#if system.schema_version === null}
					<MetricCard
						label="Schema version"
						value={null}
						unavailable="The database could not be read, so whether migrations have been applied is unknown — which is not the same as none having been."
					/>
				{:else}
					<MetricCard
						label="Schema version"
						value={integer(system.schema_version)}
						detail="Highest applied migration, from the system probe."
					/>
				{/if}
			{:else}
				<MetricCard
					label="Database"
					value={null}
					unavailable="The system probe did not answer, so nothing can be said about it."
				/>
			{/if}
			<MetricCard
				label="Deploy age"
				value={null}
				notInstrumented="Never recorded in Experimental-v1. Process uptime above is not deploy age —
					a restart resets uptime without a deploy having happened."
			/>
		</dl>
	</AdminSection>
{:else}
	<p class="notice">Reading the operational snapshot…</p>
{/if}

<style>
	.lead {
		max-width: var(--measure);
		color: var(--text-secondary);
	}

	.controls {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		flex-wrap: wrap;
	}

	.controls__read-at {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-muted);
	}

	.notice,
	.empty {
		max-width: var(--measure);
		color: var(--text-secondary);
	}

	.cards {
		display: grid;
		/* Wraps rather than scrolls: cards are short enough to reflow at 360px. */
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
		gap: var(--space-4);
		margin: 0;
	}

	.card-sha {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-4);
		border: var(--border-width) solid var(--border-subtle);
		border-radius: var(--radius-md);
		background: var(--surface-1);
	}

	.card-sha__label {
		margin: 0;
		font-size: var(--font-size-sm);
		color: var(--text-secondary);
	}

	.card-sha__value {
		margin: 0;
	}
</style>
