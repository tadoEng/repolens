/**
 * Transport for the analysis and report resources.
 *
 * Both reads go through the generated client. `api.GET` is typed against the OpenAPI
 * document's `paths`, so the URL, the name of the path parameter, and the response shape all
 * come from the contract: a route the API does not publish does not compile, and an id
 * reaches the wire through the client's own path serializer rather than through a template
 * literal written here. Nothing in this file is hand-written for a server change to drift
 * away from.
 *
 * (Something was, until issue #6 landed the endpoints: two path builders and a raw `fetch`,
 * kept deliberately narrow while the contract existed and the routes serving it did not.
 * `paths` now carries `/api/v1/analyses/{analysis_id}` and its `/report`, so they are gone.)
 *
 * The *outcome* used to live here too. `openapi-fetch` reports a request three different
 * ways — a parsed body, an `error` beside a non-OK `response`, and a thrown exception when
 * nothing answered at all — and `Fetched<T>` is the single union every caller reads instead
 * of rediscovering that split at each call site. It moved to `$lib/api/fetched` when
 * `/admin` became a second caller, because a second copy is how one of them quietly stops
 * telling a transport failure from a refusal. Re-exported here so existing importers do
 * not have to care where it lives.
 *
 * ## One mutation, and why there is still no retry
 *
 * `createAnalysis` is the only write this app makes, and it arrived with the credential
 * that makes it safe: issue #13 gates `POST /api/v1/analyses` on a verified Firebase ID
 * token. The two reads stay anonymous by design — the unguessable analysis id *is* the
 * capability, which is what lets a progress page and a report work for someone who has
 * never signed in.
 *
 * **Retry is still withheld**, and for a reason creation does not share. A retry is a
 * second request against work that may already be queued, so its behaviour on a
 * double-click, on a replay, or against a server that has already requeued is undefined,
 * and the failure mode is duplicate paid analyses rather than an error message. Creation
 * has an answer to that arriving in #28 (idempotent reuse keyed on the full
 * output-changing identity); retry has none yet. So the affordance stays withheld,
 * visibly and with the reason stated on screen (`RetryNotice.svelte`), until #28 lands.
 */

import { api } from '$lib/api/client';
import { fetched, type Fetched } from '$lib/api/fetched';
import type { Analysis, ApiError, Report } from '@repolens/api-client';
import { session } from '$lib/auth/session.svelte';

export type { Analysis, ApiError, Report };
export type { Fetched };

/** Current progress for one analysis. Readable anonymously: the ID is the capability. */
export function fetchAnalysis(analysisId: string): Promise<Fetched<Analysis>> {
	return fetched<Analysis>(
		api.GET('/api/v1/analyses/{analysis_id}', { params: { path: { analysis_id: analysisId } } })
	);
}

/**
 * The finished report for one analysis.
 *
 * Nested under the analysis rather than a top-level `/reports/{id}`: a report is identified
 * by the analysis that produced it, and there is no second identifier for it.
 */
export function fetchReport(analysisId: string): Promise<Fetched<Report>> {
	return fetched<Report>(
		api.GET('/api/v1/analyses/{analysis_id}/report', {
			params: { path: { analysis_id: analysisId } }
		})
	);
}

/**
 * Starts an analysis of one public GitHub repository.
 *
 * The only mutation this app makes, and the only request that carries a credential. The
 * ID token goes in `Authorization`, to the configured API origin and nowhere else —
 * `api` is bound to that single origin at construction, so there is no code path here
 * that could send it somewhere Firebase or a redirect chose.
 *
 * A missing token is *not* short-circuited into a local error. The request is sent
 * without it and the API refuses it, which keeps one authority for the rule: a browser
 * that believed itself signed in when it was not would otherwise show a different
 * outcome than the server produced. `UNAUTHENTICATED` comes back in the envelope and the
 * form renders it like any other refusal.
 */
export async function createAnalysis(repositoryUrl: string): Promise<Fetched<Analysis>> {
	const token = await session.idToken();

	return fetched<Analysis>(
		api.POST('/api/v1/analyses', {
			body: { repository_url: repositoryUrl },
			...(token ? { headers: { Authorization: `Bearer ${token}` } } : {})
		})
	);
}
