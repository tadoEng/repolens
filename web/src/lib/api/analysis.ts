/**
 * Transport for the analysis and report resources.
 *
 * ---------------------------------------------------------------------------------------
 * READ THIS BEFORE CHANGING IT — the paths below are the one thing here that is not
 * generated.
 * ---------------------------------------------------------------------------------------
 *
 * Issue #14 fixed the `analysis-v1` *contract*; issue #6 owns the *routes that serve it*.
 * The OpenAPI document therefore registers `Analysis`, `Report` and friends as schemas but
 * declares no paths for them, so `paths` in the generated client contains only the probe
 * and liveness operations — `api.GET('/api/v1/analyses/{id}')` does not type-check, because
 * that operation does not exist yet.
 *
 * What this module does and does not invent is the whole point:
 *
 *   - **Every shape is imported.** `Analysis`, `Report` and `ApiError` come from
 *     `@repolens/api-client`. Not one field is re-declared here, so a change to the Rust
 *     DTOs breaks this file and everything downstream of it at compile time.
 *   - **Only the paths are hand-written**, and they are the same two paths
 *     `@repolens/msw`'s `analysis-handlers.ts` intercepts. That is deliberate: the mock and
 *     the client have to agree, and the cheapest way to guarantee it is a test that asserts
 *     these builders against the handler paths (`src/tests/analysis-api.svelte.test.ts`).
 *   - **Everything here is a `GET`, and that is a boundary rather than a coincidence.**
 *
 * ## Why there is no retry request
 *
 * There was one, and it is gone. A provisional path is defensible for a read: it is
 * idempotent, it is anonymous by design — the unguessable analysis ID *is* the capability —
 * and it is pinned to a matching MSW handler, so a drifting URL fails a test rather than a
 * deployment. None of that transfers to `POST …/retry`. That request **starts work**: it is
 * an authenticated mutation on the paid path, and hand-writing it here would have meant
 * inventing four things the contract does not yet carry — the operation, its request and
 * error schemas, the Firebase bearer credential (#13), and its idempotency semantics.
 *
 * A retry sent without those is not a smaller version of the real feature. It is a request
 * whose behaviour on a double-click, on a replay, or against a server that has already
 * requeued the work is undefined, and the failure mode is duplicate paid analyses rather
 * than an error message. So the affordance is withheld, visibly and with the reason stated
 * on screen (`RetryNotice.svelte`) rather than silently dropped.
 *
 * When #6 lands the endpoints, the regenerated `paths` makes the reads callable and each
 * function below collapses to a single `api.GET(...)`; retry arrives at the same time as
 * its generated, authenticated operation, not before. The exported types and the outcome
 * union do not change, so no component is touched.
 */

import { PUBLIC_API_ORIGIN } from '$env/static/public';
import { resolveApiOrigin, type Analysis, type ApiError, type Report } from '@repolens/api-client';

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

const ORIGIN = resolveApiOrigin(PUBLIC_API_ORIGIN);

/** `GET` this for one analysis's progress. Provisional — see the banner above. */
export function analysisPath(analysisId: string): string {
	return `/api/v1/analyses/${encodeURIComponent(analysisId)}`;
}

/**
 * `GET` this for one completed report.
 *
 * Nested under the analysis rather than a top-level `/reports/{id}`: a report is identified
 * by the analysis that produced it, and there is no second identifier for it.
 */
export function reportPath(analysisId: string): string {
	return `${analysisPath(analysisId)}/report`;
}

/** Absolute URL for a path, against the single configured API origin. */
export function apiUrl(path: string): string {
	return `${ORIGIN}${path}`;
}

/**
 * Recognise an `ApiError` body without trusting it.
 *
 * A response body is untyped data no matter what the contract says, and this one arrives on
 * the failure path — precisely where a server is most likely to answer with a proxy's HTML
 * error page instead. So the shape is checked at runtime and anything else becomes `null`,
 * which the UI renders as "the server did not explain" rather than crashing on
 * `error.code.toUpperCase()`.
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
 * One `GET`.
 *
 * The method is fixed here rather than passed in. A `method` parameter is a one-line change
 * away from a mutation, and the whole point of this module's boundary is that a mutation
 * has to arrive through a generated, authenticated operation instead.
 */
async function request<T>(path: string): Promise<Fetched<T>> {
	let response: Response;
	try {
		response = await fetch(apiUrl(path), {
			method: 'GET',
			headers: { accept: 'application/json' }
		});
	} catch {
		return { kind: 'unreachable' };
	}

	const body: unknown = await response.json().catch(() => null);

	if (!response.ok) {
		return { kind: 'rejected', status: response.status, error: asApiError(body) };
	}

	if (body === null || typeof body !== 'object') {
		// A 200 with an unusable body is a server fault, not a missing resource, and saying
		// so with the status is more useful than pretending the resource is absent.
		return { kind: 'rejected', status: response.status, error: null };
	}

	return { kind: 'ok', value: body as T };
}

/** Current progress for one analysis. Readable anonymously: the ID is the capability. */
export function fetchAnalysis(analysisId: string): Promise<Fetched<Analysis>> {
	return request<Analysis>(analysisPath(analysisId));
}

/** The finished report for one analysis. */
export function fetchReport(analysisId: string): Promise<Fetched<Report>> {
	return request<Report>(reportPath(analysisId));
}
