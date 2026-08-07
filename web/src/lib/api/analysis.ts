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
 * What this module still owns is the *outcome*. `openapi-fetch` reports a request three
 * different ways — a parsed body, an `error` beside a non-OK `response`, and a thrown
 * exception when nothing answered at all — and `Fetched<T>` is the single union every caller
 * reads instead of rediscovering that split at each call site.
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
import type { Analysis, ApiError, Report } from '@repolens/api-client';
import { session } from '$lib/auth/session.svelte';

export type { Analysis, ApiError, Report };

/**
 * The result of one request.
 *
 * Three outcomes, not two, and the third is why: a transport failure and a well-formed
 * error response are different facts about the deployment. Collapsing them produces the
 * classic misdiagnosis where a CSP `connect-src` mismatch is reported to the reader as
 * "analysis not found".
 */
export type Fetched<T> =
	| { kind: 'ok'; value: T }
	/** The API answered with a status we cannot use. `error` is present when it sent one. */
	| { kind: 'rejected'; status: number; error: ApiError | null }
	/** The request never reached a server: DNS, CORS, offline, or a CSP violation. */
	| { kind: 'unreachable' };

/**
 * Recognise an `ApiError` body without trusting it.
 *
 * A response body is untyped data no matter what the contract says, and this one arrives on
 * the failure path — precisely where a server is most likely to answer with a proxy's HTML
 * error page instead. The generated types describe what the *API* promises to send, not what
 * a load balancer between it and the browser actually sent, so this stays a runtime check
 * rather than a cast: the shape is verified and anything else becomes `null`, which the UI
 * renders as "the server did not explain" rather than crashing on `error.code.toUpperCase()`.
 *
 * `code` is not validated against the enum here on purpose: an unrecognised code is data the
 * UI must still display, and rejecting it would be the silent drop the contract forbids.
 */
function asApiError(body: unknown): ApiError | null {
	if (typeof body !== 'object' || body === null) return null;
	const candidate = body as Record<string, unknown>;
	if (typeof candidate.code !== 'string' || typeof candidate.message !== 'string') return null;

	const retryAfter = candidate.retry_after_seconds;
	return {
		code: candidate.code as ApiError['code'],
		message: candidate.message,
		retry_after_seconds: typeof retryAfter === 'number' ? retryAfter : null
	};
}

/**
 * What `openapi-fetch` resolves with, narrowed to the three fields this module reads.
 *
 * Structural rather than the exported `FetchResponse<…>` generic, which would have to be
 * threaded an operation type to be written down. Every call site is still checked against
 * the real result — an operation whose success body is not `T` fails to compile here.
 */
interface ApiResult<T> {
	/** Absent when the response carried no body, `null` when the body was literally `null`. */
	data?: T | null;
	/** The failure body, parsed when it was JSON and the raw text when it was not. */
	error?: unknown;
	response: Response;
}

/**
 * Turn one client call into one outcome.
 *
 * `response.ok` decides, never the truthiness of `error`: `openapi-fetch` leaves `error` as
 * the raw text when a failure body is not JSON, so an empty 404 arrives as `''` and an HTML
 * error page as a string — both falsy or truthy for reasons that have nothing to do with
 * whether the request succeeded.
 */
async function fetched<T>(call: Promise<ApiResult<T>>): Promise<Fetched<T>> {
	let result: ApiResult<T>;

	try {
		result = await call;
	} catch {
		/*
		 * A transport failure is *thrown* by `openapi-fetch` rather than returned as `error`:
		 * there is no response, so there is no status to report and nothing to parse. This is
		 * the branch that keeps a CORS or CSP failure from being rendered as "not found".
		 *
		 * One known imprecision, recorded rather than left to be discovered: a **2xx** whose
		 * body is not valid JSON also lands here, because `openapi-fetch` throws inside its
		 * own parse step and hands back no response to read a status from. That reports
		 * `unreachable` for a server that demonstrably answered — the same conflation this
		 * union exists to prevent, in the opposite direction. It is accepted only because
		 * this API sends JSON on every path, and because the realistic case of a proxy
		 * substituting an HTML error page carries a non-2xx status and so still lands in
		 * `rejected` with its status intact. Recovering the status would mean shadowing
		 * `fetch` per request to buffer the body ahead of the client — more machinery in the
		 * transport than the case has earned. Revisit if it ever fires in practice.
		 */
		return { kind: 'unreachable' };
	}

	const { data, response } = result;

	if (!response.ok) {
		return { kind: 'rejected', status: response.status, error: asApiError(result.error) };
	}

	if (typeof data !== 'object' || data === null) {
		// A success status with no usable body is a server fault, not a missing resource, and
		// saying so with the status is more useful than pretending the resource is absent.
		// Catches all three ways that happens: no body at all, a literal `null`, and a scalar
		// where the contract promises an object.
		return { kind: 'rejected', status: response.status, error: null };
	}

	return { kind: 'ok', value: data };
}

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
